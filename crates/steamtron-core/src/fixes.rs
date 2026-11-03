use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::data_home;

const MARKER: &str = "steamtron-fixes.json";
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_UNCOMPRESSED: u64 = 2 * 1024 * 1024 * 1024;
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
    data_home().join("steamtron/fixes")
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
    let target = fixes_path(appid)?;
    let archive_size = fs::metadata(zip_path)
        .map_err(|error| FixesError::io(zip_path, error))?
        .len();
    if archive_size > MAX_ARCHIVE {
        return Err(FixesError::Invalid("archive is too large".into()));
    }
    let root = fixes_root();
    prepare_root(&root)?;
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
pub fn export_zip(target: &Path, destination: &Path, appid: &str) -> Result<(), FixesError> {
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
        fs::create_dir_all(parent).map_err(|error| FixesError::io(parent, error))?;
    }
    let temporary = destination.with_extension("zip.steamtron.tmp");
    let result = (|| -> Result<(), FixesError> {
        let file = fs::File::create(&temporary).map_err(|error| FixesError::io(&temporary, error))?;
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer
            .start_file(MARKER, options)
            .map_err(FixesError::Zip)?;
        writer
            .write_all(format!("{{\"format\":1,\"appid\":\"{appid}\"}}\n").as_bytes())
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

pub fn export_name(title: &str, appid: &str) -> String {
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
        value = format!("Steam App {appid}");
    }
    value.truncate(180);
    format!("{value}.zip")
}

#[cfg(test)]
mod tests {
    use super::*;

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
