use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};


const MARKER: &str = "steamtron-fixes.json";
const MAX_ARCHIVE: u64 = 64 * 1024 * 1024 * 1024;
const MAX_UNCOMPRESSED: u64 = 64 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 20_000;

#[derive(Debug, Error)]
pub enum FixesError {
    #[error("invalid App ID")]
    InvalidAppId,
    #[error("fixes ZIP is unsafe: {0}")]
    Unsafe(String),
    #[error("fixes ZIP is invalid: {0}")]
    Invalid(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

impl FixesError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub fn fixes_root() -> PathBuf {
    if let Some(value) = std::env::var_os("STEAMTRON_FIXES") {
        return PathBuf::from(value);
    }
    crate::paths::layout().legacy_fixes_root()
}

pub fn fixes_path(appid: &str) -> Result<PathBuf, FixesError> {
    if appid.is_empty() || !appid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(FixesError::InvalidAppId);
    }
    Ok(fixes_root().join(appid))
}

fn prepare_root(root: &Path) -> Result<(), FixesError> {
    fs::create_dir_all(root).map_err(|error| FixesError::io(root, error))?;
    if fs::symlink_metadata(root)
        .map_err(|error| FixesError::io(root, error))?
        .file_type()
        .is_symlink()
    {
        return Err(FixesError::Unsafe(
            "fixes root must not be a symbolic link".into(),
        ));
    }
    Ok(())
}

fn safe_name(name: &str) -> Result<String, FixesError> {
    let value = name.replace('\\', "/");
    if value.starts_with('/') || value.contains('\0') {
        return Err(FixesError::Unsafe("absolute or NUL path".into()));
    }
    let mut parts = Vec::new();
    for part in value.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(FixesError::Unsafe("parent traversal".into()));
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(FixesError::Unsafe("empty path".into()));
    }
    Ok(parts.join("/"))
}

/// Extract a ZIP exactly as structured, excluding the SteamTron marker.
pub fn install_zip(appid: &str, zip_path: &Path) -> Result<PathBuf, FixesError> {
    install_zip_into(appid, zip_path, &fixes_path(appid)?)
}

/// Extract a fixes ZIP into an explicit target folder (managed games).
///
/// Staging and backups live beside the target so a rename never crosses
/// filesystems when the fixes folder is on a game drive.
pub fn install_zip_into(
    appid: &str,
    zip_path: &Path,
    target: &Path,
) -> Result<PathBuf, FixesError> {
    if appid.is_empty() || !appid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(FixesError::InvalidAppId);
    }
    let archive_size = fs::metadata(zip_path)
        .map_err(|error| FixesError::io(zip_path, error))?
        .len();
    if archive_size > MAX_ARCHIVE {
        return Err(FixesError::Invalid("archive is too large".into()));
    }
    let root = target
        .parent()
        .ok_or_else(|| FixesError::Invalid("fixes target has no parent".into()))?;
    prepare_root(root)?;
    let target = target.to_path_buf();
    let staging = root.join(format!(
        ".{appid}.staging-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&staging).map_err(|error| FixesError::io(&staging, error))?;
    let content = staging.join("content");
    fs::create_dir(&content).map_err(|error| FixesError::io(&content, error))?;
    let result = (|| -> Result<(), FixesError> {
        let mut archive = ZipArchive::new(fs::File::open(zip_path).map_err(|error| {
            FixesError::io(zip_path, error)
        })?)?;
        let mut count = 0usize;
        let mut total = 0u64;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            if entry.is_dir() {
                continue;
            }
            let name = safe_name(entry.name())?;
            if name == MARKER {
                continue;
            }
            if entry.encrypted() {
                return Err(FixesError::Invalid(
                    "encrypted entries are not supported".into(),
                ));
            }
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(FixesError::Unsafe("symbolic link in ZIP".into()));
            }
            count += 1;
            total = total.saturating_add(entry.size());
            if count > MAX_FILES || total > MAX_UNCOMPRESSED {
                return Err(FixesError::Invalid("ZIP limits exceeded".into()));
            }
            let destination = content.join(&name);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| FixesError::io(parent, error))?;
            }
            let mut output =
                fs::File::create(&destination).map_err(|error| FixesError::io(&destination, error))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| FixesError::io(&destination, error))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut mode = entry.unix_mode().unwrap_or(0o644);
                if destination
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
                {
                    mode |= 0o111;
                }
                fs::set_permissions(&destination, fs::Permissions::from_mode(mode))
                    .map_err(|error| FixesError::io(&destination, error))?;
            }
        }
        if count == 0 {
            return Err(FixesError::Invalid("ZIP is empty".into()));
        }
        if target.is_symlink() {
            return Err(FixesError::Unsafe("target fixes path is a symlink".into()));
        }
        if target.exists() {
            let backup = root.join(format!(
                ".{appid}.old-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            ));
            fs::rename(&target, &backup).map_err(|error| FixesError::io(&target, error))?;
            fs::rename(&content, &target).map_err(|error| FixesError::io(&target, error))?;
            let _ = fs::remove_dir_all(backup);
        } else {
            fs::rename(&content, &target).map_err(|error| FixesError::io(&target, error))?;
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result.map(|_| target)
}

/// Export a fixes directory as a portable ZIP containing the format marker.
pub fn export_zip(target: &Path, destination: &Path, _appid: &str) -> Result<(), FixesError> {
    if !target.is_dir() || target.is_symlink() {
        return Err(FixesError::Invalid("no fixes directory".into()));
    }
    if destination
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some("zip")
    {
        return Err(FixesError::Invalid("destination must be a .zip".into()));
    }
    if let Some(parent) = destination.parent() {
        let metadata = fs::symlink_metadata(parent)
            .map_err(|error| FixesError::io(parent, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(FixesError::Invalid(
                "destination folder must be a real directory".into(),
            ));
        }
        fs::create_dir_all(parent).map_err(|error| FixesError::io(parent, error))?;
    }
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(FixesError::Invalid(
                "destination must be a regular ZIP file".into(),
            ));
        }
    }
    let temporary = destination.with_extension(format!("zip.{}.tmp", std::process::id()));
    let result = (|| -> Result<(), FixesError> {
        let file = fs::File::create(&temporary).map_err(|error| FixesError::io(&temporary, error))?;
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer
            .start_file(MARKER, options)
            .map_err(FixesError::Zip)?;
        writer
            .write_all(b"{\"format\":1}\n")
            .map_err(|error| FixesError::io(&temporary, error))?;
        let mut files = Vec::new();
        collect_files(target, target, &mut files)?;
        if files.is_empty() {
            return Err(FixesError::Invalid("fixes directory is empty".into()));
        }
        for (path, name) in files {
            writer.start_file(name, options).map_err(FixesError::Zip)?;
            let mut input = fs::File::open(&path).map_err(|error| FixesError::io(&path, error))?;
            std::io::copy(&mut input, &mut writer)
                .map_err(|error| FixesError::io(&path, error))?;
        }
        writer.finish().map_err(FixesError::Zip)?;
        fs::rename(&temporary, destination)
            .map_err(|error| FixesError::io(destination, error))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn collect_files(
    base: &Path,
    directory: &Path,
    output: &mut Vec<(PathBuf, String)>,
) -> Result<(), FixesError> {
    for entry in fs::read_dir(directory).map_err(|error| FixesError::io(directory, error))? {
        let entry = entry.map_err(|error| FixesError::io(directory, error))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| FixesError::io(&path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(FixesError::Unsafe(format!(
                "symbolic link in fixes directory: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_files(base, &path, output)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(base)
                .map_err(|_| FixesError::Unsafe("path escaped fixes root".into()))?;
            output.push((
                path.clone(),
                relative.to_string_lossy().replace('\\', "/"),
            ));
        }
    }
    Ok(())
}

pub fn export_name(title: &str) -> String {
    let mut value: String = title
        .chars()
        .map(|character| {
            if matches!(character, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                ' '
            } else {
                character
            }
        })
        .collect();
    value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    value = value.trim_matches('.').trim().to_owned();
    if value.is_empty() {
        value = "Game".to_owned();
    }
    value.truncate(180);
    format!("{value}.zip")
}

/// Export the fixes folder in the backup layout:
/// `fixes/<folder>/<relative files>`, uncompressed tar.
pub fn export_tar(
    target: &Path,
    destination: &Path,
    folder: &str,
    _appid: &str,
) -> Result<String, FixesError> {
    use sha2::{Digest, Sha256};

    if !target.is_dir() || target.is_symlink() {
        return Err(FixesError::Invalid("no fixes directory".into()));
    }
    if destination
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some("tar")
    {
        return Err(FixesError::Invalid("destination must be a .tar".into()));
    }
    let folder = safe_name(folder)?;
    if let Some(parent) = destination.parent() {
        let metadata =
            fs::symlink_metadata(parent).map_err(|error| FixesError::io(parent, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(FixesError::Invalid(
                "destination folder must be a real directory".into(),
            ));
        }
    }
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(FixesError::Invalid(
                "destination must be a regular TAR file".into(),
            ));
        }
    }
    let temporary = destination.with_extension(format!("tar.{}.tmp", std::process::id()));
    let result = (|| -> Result<String, FixesError> {
        let file =
            fs::File::create(&temporary).map_err(|error| FixesError::io(&temporary, error))?;
        let mut archive = tar::Builder::new(file);
        let top = format!("fixes/{folder}");
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(&mut header, &top, std::io::empty())
            .map_err(|error| FixesError::Invalid(error.to_string()))?;
        let mut files = Vec::new();
        collect_files(target, target, &mut files)?;
        files.retain(|(_, name)| name != MARKER);
        if files.is_empty() {
            return Err(FixesError::Invalid("fixes directory is empty".into()));
        }
        for (path, name) in files {
            archive
                .append_path_with_name(&path, format!("{top}/{name}"))
                .map_err(|error| FixesError::Invalid(error.to_string()))?;
        }
        let file = archive
            .into_inner()
            .map_err(|error| FixesError::Invalid(error.to_string()))?;
        file.sync_data().map_err(|error| FixesError::io(&temporary, error))?;
        fs::rename(&temporary, destination)
            .map_err(|error| FixesError::io(destination, error))?;
        let mut file =
            fs::File::open(destination).map_err(|error| FixesError::io(destination, error))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 1024 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| FixesError::io(destination, error))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Prefix used by an existing fixes TAR (`fixes/<folder>`), if any.
pub fn tar_prefix(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries().ok()? {
        let entry = entry.ok()?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = entry.path().ok()?.to_string_lossy().replace('\\', "/");
        let mut parts = name.split('/');
        if parts.next()? != "fixes" {
            return None;
        }
        let folder = parts.next()?;
        if folder.is_empty() {
            return None;
        }
        return Some(format!("fixes/{folder}"));
    }
    None
}

/// Extract a backup-style fixes TAR into a game fixes folder.
pub fn install_tar_into(
    appid: &str,
    tar_path: &Path,
    target: &Path,
) -> Result<PathBuf, FixesError> {
    if appid.is_empty() || !appid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(FixesError::InvalidAppId);
    }
    let archive_size = fs::metadata(tar_path)
        .map_err(|error| FixesError::io(tar_path, error))?
        .len();
    if archive_size > MAX_ARCHIVE {
        return Err(FixesError::Invalid("archive is too large".into()));
    }
    let root = target
        .parent()
        .ok_or_else(|| FixesError::Invalid("fixes target has no parent".into()))?;
    prepare_root(root)?;
    let target = target.to_path_buf();
    let staging = root.join(format!(
        ".{appid}.staging-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&staging).map_err(|error| FixesError::io(&staging, error))?;
    let content = staging.join("content");
    fs::create_dir(&content).map_err(|error| FixesError::io(&content, error))?;
    let prefix = tar_prefix(tar_path);
    let result = (|| -> Result<(), FixesError> {
        let file =
            fs::File::open(tar_path).map_err(|error| FixesError::io(tar_path, error))?;
        let mut archive = tar::Archive::new(file);
        let mut count = 0usize;
        let mut total = 0u64;
        for entry in archive
            .entries()
            .map_err(|error| FixesError::Invalid(error.to_string()))?
        {
            let mut entry =
                entry.map_err(|error| FixesError::Invalid(error.to_string()))?;
            if entry.header().entry_type().is_dir() {
                continue;
            }
            if !entry.header().entry_type().is_file() {
                return Err(FixesError::Unsafe(
                    "TAR contains a link or special file".into(),
                ));
            }
            let name = entry
                .path()
                .map_err(|error| FixesError::Invalid(error.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let relative = match prefix.as_deref() {
                Some(prefix) => name
                    .strip_prefix(prefix)
                    .and_then(|value| value.strip_prefix('/'))
                    .unwrap_or(&name),
                None => name.as_str(),
            };
            let relative = safe_name(relative)?;
            if relative == MARKER {
                continue;
            }
            count += 1;
            total = total.saturating_add(entry.size());
            if count > MAX_FILES || total > MAX_UNCOMPRESSED {
                return Err(FixesError::Invalid("TAR limits exceeded".into()));
            }
            let destination = content.join(&relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| FixesError::io(parent, error))?;
            }
            let mut output = fs::File::create(&destination)
                .map_err(|error| FixesError::io(&destination, error))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|error| FixesError::io(&destination, error))?;
        }
        if count == 0 {
            return Err(FixesError::Invalid("TAR is empty".into()));
        }
        if target.is_symlink() {
            return Err(FixesError::Unsafe("target fixes path is a symlink".into()));
        }
        if target.exists() {
            let backup = root.join(format!(
                ".{appid}.old-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            ));
            fs::rename(&target, &backup).map_err(|error| FixesError::io(&target, error))?;
            fs::rename(&content, &target).map_err(|error| FixesError::io(&target, error))?;
            let _ = fs::remove_dir_all(backup);
        } else {
            fs::rename(&content, &target).map_err(|error| FixesError::io(&target, error))?;
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result.map(|_| target)
}

/// Install a ZIP or TAR fixes archive based on its magic bytes.
pub fn install_archive_into(
    appid: &str,
    archive_path: &Path,
    target: &Path,
) -> Result<PathBuf, FixesError> {
    let mut header = [0u8; 512];
    let read = {
        let mut file =
            fs::File::open(archive_path).map_err(|error| FixesError::io(archive_path, error))?;
        file.read(&mut header)
            .map_err(|error| FixesError::io(archive_path, error))?
    };
    if read >= 4 && header[..4] == [0x50, 0x4b, 0x03, 0x04] {
        return install_zip_into(appid, archive_path, target);
    }
    if read >= 262 && &header[257..262] == b"ustar" {
        return install_tar_into(appid, archive_path, target);
    }
    Err(FixesError::Invalid(
        "archive must be a ZIP or TAR fixes pack".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tar_round_trips_backup_layout() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("fixes");
        fs::create_dir_all(source.join("sub")).unwrap();
        fs::write(source.join("a.txt"), b"alpha").unwrap();
        fs::write(source.join("sub/b.bin"), b"beta").unwrap();
        fs::write(source.join(MARKER), b"{}").unwrap();
        let tar = temp.path().join("fixes.tar");
        let digest = export_tar(&source, &tar, "Sample Game", "42").unwrap();
        assert_eq!(digest.len(), 64);
        assert!(tar.is_file());
        assert_eq!(tar_prefix(&tar).as_deref(), Some("fixes/Sample Game"));
        let target = temp.path().join("installed");
        install_tar_into("42", &tar, &target).unwrap();
        assert_eq!(fs::read(target.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(fs::read(target.join("sub/b.bin")).unwrap(), b"beta");
        assert!(!target.join(MARKER).exists());
        let second = temp.path().join("installed2");
        install_archive_into("42", &tar, &second).unwrap();
        assert!(second.join("a.txt").is_file());
    }

    #[test]
    fn archive_detection_rejects_unknown_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("notes.txt");
        fs::write(&path, b"not an archive").unwrap();
        let target = temp.path().join("out");
        assert!(install_archive_into("42", &path, &target).is_err());
    }

    #[test]
    fn export_name_uses_sanitized_title() {
        assert_eq!(export_name("Resident Evil Village"), "Resident Evil Village.zip");
        assert_eq!(export_name("Game: Deluxe/Test"), "Game Deluxe Test.zip");
    }

    #[test]
    fn exports_marker_and_preserves_structure() {
        let temp = tempfile::tempdir().unwrap();
        let fixes = temp.path().join("fixes");
        fs::create_dir_all(fixes.join("bin")).unwrap();
        fs::write(fixes.join("bin/game.dll"), b"fix").unwrap();
        let target = temp.path().join("out.zip");
        export_zip(&fixes, &target, "42").unwrap();
        let mut archive = ZipArchive::new(fs::File::open(target).unwrap()).unwrap();
        assert!(archive.by_name(MARKER).is_ok());
        assert!(archive.by_name("bin/game.dll").is_ok());
    }

    #[test]
    fn safe_names_reject_parent() {
        assert!(safe_name("../evil").is_err());
        assert_eq!(safe_name("bin/game.dll").unwrap(), "bin/game.dll");
    }
}
