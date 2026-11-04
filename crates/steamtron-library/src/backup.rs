//! Portable metadata backup for SteamTron-managed games.
//!
//! Backups contain Lua files, appmanifests, and fixes directories. They never
//! contain game content, prefixes, saves, Steam credentials, or depot payloads.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use steamtron_core::config::Config;
use steamtron_core::steam;

use steamtron_core::fixes::{fixes_root, fixes_path};
use crate::LibraryError;

const MANIFEST: &str = "steamtron-backup.json";
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_UNCOMPRESSED: u64 = 1024 * 1024 * 1024;
const MAX_FILES: usize = 20_000;

#[derive(Debug, Serialize)]
pub struct BackupSummary {
    pub path: String,
    pub games: usize,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct RestoreSummary {
    pub games: usize,
    pub lua_files: usize,
    pub appmanifests: usize,
    pub fixes: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupManifest {
    version: u32,
    created_at: i64,
    games: Vec<BackupGame>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupGame {
    appid: String,
    title: String,
    enabled: bool,
    launch_option_disabled: bool,
    has_fixes: bool,
}

pub fn export_backup(path: &Path, appids: Option<&BTreeSet<String>>) -> Result<BackupSummary, LibraryError> {
    let root = steam::steam_root().map_err(|error| LibraryError::Archive(error.to_string()))?;
    let config = Config::load().map_err(|error| LibraryError::Archive(error.to_string()))?;
    let installed: BTreeMap<String, steam::SteamGame> = steam::games()
        .map_err(|error| LibraryError::Archive(error.to_string()))?
        .into_iter()
        .map(|game| (game.appid.clone(), game))
        .collect();
    let managed = steam::managed_appids().map_err(|error| LibraryError::Archive(error.to_string()))?;
    let selected: Vec<String> = managed
        .into_iter()
        .filter(|appid| appids.map_or(true, |wanted| wanted.contains(appid)))
        .collect();
    if selected.is_empty() {
        return Err(LibraryError::Archive("No managed games to back up.".into()));
    }

    let file = fs::File::create(path).map_err(|error| LibraryError::io(path, error))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut games = Vec::new();
    let mut files = 0usize;
    let mut bytes = 0u64;

    let add = |writer: &mut ZipWriter<fs::File>, name: &str, data: &[u8], files: &mut usize, bytes: &mut u64| -> Result<(), LibraryError> {
        writer.start_file(name, options).map_err(LibraryError::Zip)?;
        writer.write_all(data).map_err(|error| LibraryError::io(name, error))?;
        *files += 1;
        *bytes = bytes.saturating_add(data.len() as u64);
        Ok(())
    };

    for appid in selected {
        let lua_path = root.join("config/stplug-in").join(format!("{appid}.lua"));
        let lua = match fs::read(&lua_path) {
            Ok(data) => Some(data),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(LibraryError::io(&lua_path, error)),
        };
        let game = installed.get(&appid);
        let appmanifest = game.map(|game| game.manifest.as_path());
        let config_entry = config.entry(&appid);
        let fixes = config_entry.map(|entry| entry.fixes.as_path());
        let has_fixes = fixes.is_some_and(|path| path.is_dir());
        let title = game
            .map(|game| game.name.clone())
            .or_else(|| config_entry.map(|_| format!("Steam App {appid}")))
            .unwrap_or_else(|| format!("Steam App {appid}"));

        games.push(BackupGame {
            appid: appid.clone(),
            title,
            enabled: config_entry.is_some_and(|entry| entry.enabled),
            launch_option_disabled: config_entry.is_some_and(|entry| entry.launch_option_disabled),
            has_fixes,
        });
        if let Some(lua) = lua {
            add(&mut writer, &format!("lua/{appid}.lua"), &lua, &mut files, &mut bytes)?;
        }
        if let Some(manifest) = appmanifest {
            if let Ok(data) = fs::read(manifest) {
                add(&mut writer, &format!("appmanifests/appmanifest_{appid}.acf"), &data, &mut files, &mut bytes)?;
            }
        }
        if let Some(fixes) = fixes.filter(|path| path.is_dir()) {
            add_fixes(&mut writer, &appid, fixes, &mut files, &mut bytes)?;
        }
    }

    let manifest = BackupManifest {
        version: 1,
        created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64,
        games,
    };
    let encoded = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| LibraryError::Archive(error.to_string()))?;
    add(&mut writer, MANIFEST, &encoded, &mut files, &mut bytes)?;
    writer.finish().map_err(LibraryError::Zip)?;
    Ok(BackupSummary {
        path: path.display().to_string(),
        games: manifest.games.len(),
        files,
        bytes,
    })
}

fn add_fixes(
    writer: &mut ZipWriter<fs::File>,
    appid: &str,
    root: &Path,
    files: &mut usize,
    bytes: &mut u64,
) -> Result<(), LibraryError> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let entries = fs::read_dir(&path).map_err(|error| LibraryError::io(&path, error))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| LibraryError::io(&path, error))?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|error| LibraryError::Archive(error.to_string()))?;
            let name = format!("fixes/{appid}/{}", relative.to_string_lossy().replace('\\', "/"));
            let data = fs::read(&path).map_err(|error| LibraryError::io(&path, error))?;
            let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer.start_file(name, options).map_err(LibraryError::Zip)?;
            writer.write_all(&data).map_err(|error| LibraryError::io(&path, error))?;
            *files += 1;
            *bytes = bytes.saturating_add(data.len() as u64);
            if *files > MAX_FILES || *bytes > MAX_ARCHIVE {
                return Err(LibraryError::Archive("Backup size limit exceeded.".into()));
            }
        }
    }
    Ok(())
}

pub fn restore_backup(path: &Path) -> Result<RestoreSummary, LibraryError> {
    let size = fs::metadata(path).map_err(|error| LibraryError::io(path, error))?.len();
    if size > MAX_ARCHIVE {
        return Err(LibraryError::Archive("Backup archive is too large.".into()));
    }
    let mut archive = ZipArchive::new(fs::File::open(path).map_err(|error| LibraryError::io(path, error))?)
        .map_err(LibraryError::Zip)?;
    let mut manifest = None;
    let mut files = Vec::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(LibraryError::Zip)?;
        if entry.is_dir() {
            continue;
        }
        let name = safe_name(entry.name())?;
        if entry.encrypted() {
            return Err(LibraryError::Archive("Encrypted backup entries are not supported.".into()));
        }
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err(LibraryError::Archive("Backup contains a symbolic link.".into()));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_UNCOMPRESSED || files.len() > MAX_FILES {
            return Err(LibraryError::Archive("Backup size limit exceeded.".into()));
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|error| LibraryError::io(&path, error))?;
        if name == MANIFEST {
            manifest = Some(serde_json::from_slice::<BackupManifest>(&data).map_err(|error| LibraryError::Archive(error.to_string()))?);
        } else {
            files.push((name, data));
        }
    }
    let manifest = manifest.ok_or_else(|| LibraryError::Archive("Missing backup manifest.".into()))?;
    if manifest.version != 1 {
        return Err(LibraryError::Archive(format!("Unsupported backup version {}.", manifest.version)));
    }
    let root = steam::steam_root().map_err(|error| LibraryError::Archive(error.to_string()))?;
    let lua_dir = root.join("config/stplug-in");
    fs::create_dir_all(&lua_dir).map_err(|error| LibraryError::io(&lua_dir, error))?;
    let library = steam::libraries()
        .map_err(|error| LibraryError::Archive(error.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| LibraryError::Archive("No Steam library found.".into()))?;
    let allowed: BTreeSet<&str> = manifest.games.iter().map(|game| game.appid.as_str()).collect();
    let mut lua_files = 0;
    let mut appmanifests = 0;
    let mut fixes_count = 0;
    for (name, data) in files {
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() < 2 || !allowed.contains(parts[1].trim_end_matches(".lua")) && parts[0] != "fixes" && parts[0] != "lua" && parts[0] != "appmanifests" {
            return Err(LibraryError::Archive(format!("Unexpected backup entry: {name}")));
        }
        if let Some(rest) = name.strip_prefix("lua/") {
            let appid = rest.strip_suffix(".lua").ok_or_else(|| LibraryError::Archive("Invalid Lua entry.".into()))?;
            if !allowed.contains(appid) { return Err(LibraryError::Archive("Unexpected Lua App ID.".into())); }
            atomic_write(&lua_dir.join(format!("{appid}.lua")), &data)?;
            lua_files += 1;
        } else if let Some(rest) = name.strip_prefix("appmanifests/appmanifest_") {
            let appid = rest.strip_suffix(".acf").ok_or_else(|| LibraryError::Archive("Invalid appmanifest entry.".into()))?;
            if !allowed.contains(appid) { return Err(LibraryError::Archive("Unexpected appmanifest App ID.".into())); }
            atomic_write(&library.join(format!("appmanifest_{appid}.acf")), &data)?;
            appmanifests += 1;
        } else if let Some(rest) = name.strip_prefix("fixes/") {
            let (appid, relative) = rest.split_once('/').ok_or_else(|| LibraryError::Archive("Invalid fixes entry.".into()))?;
            if !allowed.contains(appid) { return Err(LibraryError::Archive("Unexpected fixes App ID.".into())); }
            let target = fixes_path(appid).map_err(|error| LibraryError::Archive(error.to_string()))?;
            let destination = target.join(safe_relative(relative)?);
            if let Some(parent) = destination.parent() { fs::create_dir_all(parent).map_err(|error| LibraryError::io(parent, error))?; }
            atomic_write(&destination, &data)?;
        }
    }
    let mut config = Config::load().map_err(|error| LibraryError::Archive(error.to_string()))?;
    for game in &manifest.games {
        let target = fixes_root().join(&game.appid);
        if game.has_fixes && target.is_dir() {
            let entry = config
                .upsert(&game.appid, &target, game.enabled)
                .map_err(|error| LibraryError::Archive(error.to_string()))?;
            if entry.launch_option_disabled != game.launch_option_disabled {
                config.set_launch_option_disabled(&game.appid, game.launch_option_disabled)
                    .map_err(|error| LibraryError::Archive(error.to_string()))?;
            }
            fixes_count += 1;
        }
    }
    Ok(RestoreSummary {
        games: manifest.games.len(),
        lua_files,
        appmanifests,
        fixes: fixes_count,
    })
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<(), LibraryError> {
    let temporary = path.with_extension(format!("{}.tmp", path.extension().and_then(|value| value.to_str()).unwrap_or("file")));
    fs::write(&temporary, data).map_err(|error| LibraryError::io(&temporary, error))?;
    fs::rename(&temporary, path).map_err(|error| LibraryError::io(path, error))
}

fn safe_name(name: &str) -> Result<String, LibraryError> {
    let value = name.replace('\\', "/");
    if value.starts_with('/') || value.contains('\0') {
        return Err(LibraryError::Archive("Unsafe backup path.".into()));
    }
    let mut parts = Vec::new();
    for part in value.split('/') {
        if part.is_empty() || part == "." { continue; }
        if part == ".." { return Err(LibraryError::Archive("Path traversal in backup.".into())); }
        parts.push(part);
    }
    if parts.is_empty() { return Err(LibraryError::Archive("Empty backup path.".into())); }
    Ok(parts.join("/"))
}

fn safe_relative(name: &str) -> Result<PathBuf, LibraryError> {
    Ok(PathBuf::from(safe_name(name)?))
}
