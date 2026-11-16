//! Portable metadata backup for SteamTron-managed games.
//!
//! Backups contain Lua files, appmanifests, and fixes directories. They never
//! contain game content, prefixes, saves, Steam credentials, or depot payloads.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use steamtron_core::config::Config;
use steamtron_core::library::{managed_lua_path, Library};
use steamtron_core::steam;

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
    pub library: bool,
    pub warnings: Vec<String>,
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
    let library = Library::load().ok();
    let library_bytes = fs::read(Library::default_path()).ok();
    let root = steam::steam_root().ok();
    let config = Config::load().ok();
    let installed: BTreeMap<String, steam::SteamGame> = steam::games()
        .unwrap_or_default()
        .into_iter()
        .map(|game| (game.appid.clone(), game))
        .collect();
    let mut managed: BTreeSet<String> = library
        .as_ref()
        .map(|library| {
            library
                .games()
                .iter()
                .filter_map(|game| game.metadata_app_id.map(|id| id.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if let Ok(ids) = steam::managed_appids() {
        managed.extend(ids);
    }
    let selected: Vec<String> = managed
        .into_iter()
        .filter(|appid| appids.map_or(true, |wanted| wanted.contains(appid)))
        .collect();
    if selected.is_empty() && library_bytes.is_none() {
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

    if let Some(data) = library_bytes {
        add(&mut writer, "library.json", &data, &mut files, &mut bytes)?;
    }

    for appid in selected {
        let library_game = library.as_ref().and_then(|library| {
            appid
                .parse::<u32>()
                .ok()
                .and_then(|id| library.game_by_app_id(id))
        });
        let lua_path = library_game
            .and_then(managed_lua_path)
            .filter(|path| path.is_file())
            .or_else(|| {
                root.as_ref()
                    .map(|root| root.join("config/stplug-in").join(format!("{appid}.lua")))
                    .filter(|path| path.is_file())
            });
        let lua = lua_path.and_then(|path| fs::read(path).ok());
        let game = installed.get(&appid);
        let appmanifest = game.map(|game| game.manifest.as_path());
        let config_entry = config.as_ref().and_then(|config| config.entry(&appid));
        let title = library_game
            .map(|game| game.title.clone())
            .or_else(|| game.map(|game| game.name.clone()))
            .or_else(|| config_entry.map(|_| format!("Steam App {appid}")))
            .unwrap_or_else(|| format!("Steam App {appid}"));

        games.push(BackupGame {
            appid: appid.clone(),
            title,
            enabled: config_entry.is_some_and(|entry| entry.enabled),
            launch_option_disabled: config_entry.is_some_and(|entry| entry.launch_option_disabled),
            has_fixes: false,
        });
        if let Some(lua) = lua {
            add(&mut writer, &format!("lua/{appid}.lua"), &lua, &mut files, &mut bytes)?;
        }
        if let Some(manifest) = appmanifest {
            if let Ok(data) = fs::read(manifest) {
                add(&mut writer, &format!("appmanifests/appmanifest_{appid}.acf"), &data, &mut files, &mut bytes)?;
            }
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

pub fn restore_backup(path: &Path) -> Result<RestoreSummary, LibraryError> {
    let size = fs::metadata(path).map_err(|error| LibraryError::io(path, error))?.len();
    if size > MAX_ARCHIVE {
        return Err(LibraryError::Archive("Backup archive is too large.".into()));
    }
    let mut archive = ZipArchive::new(fs::File::open(path).map_err(|error| LibraryError::io(path, error))?)
        .map_err(LibraryError::Zip)?;
    let mut manifest = None;
    let mut library_data = None;
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
        } else if name == "library.json" {
            library_data = Some(data);
        } else {
            files.push((name, data));
        }
    }
    let manifest = manifest.ok_or_else(|| LibraryError::Archive("Missing backup manifest.".into()))?;
    if manifest.version != 1 {
        return Err(LibraryError::Archive(format!("Unsupported backup version {}.", manifest.version)));
    }
    let library_imported = if let Some(data) = library_data {
        let temporary = std::env::temp_dir().join(format!(
            "steamtron-restore-{}-{}.json",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
        ));
        fs::write(&temporary, &data).map_err(|error| LibraryError::io(&temporary, error))?;
        let imported = steamtron_core::library::import_library_file(&temporary);
        let _ = fs::remove_file(&temporary);
        imported.map_err(|error| LibraryError::Archive(error.to_string()))?;
        true
    } else {
        false
    };
    let library = Library::load().ok();
    let root = steam::steam_root().ok();
    let lua_dir = root.as_ref().map(|root| root.join("config/stplug-in"));
    let steam_library = steam::libraries()
        .ok()
        .and_then(|libraries| libraries.into_iter().next());
    let allowed: BTreeSet<&str> = manifest.games.iter().map(|game| game.appid.as_str()).collect();
    let mut lua_files = 0;
    let mut appmanifests = 0;
    let mut warnings = Vec::new();
    for (name, data) in files {
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() < 2 || !allowed.contains(parts[1].trim_end_matches(".lua")) && parts[0] != "fixes" && parts[0] != "lua" && parts[0] != "appmanifests" {
            return Err(LibraryError::Archive(format!("Unexpected backup entry: {name}")));
        }
        if let Some(rest) = name.strip_prefix("lua/") {
            let appid = rest.strip_suffix(".lua").ok_or_else(|| LibraryError::Archive("Invalid Lua entry.".into()))?;
            if !allowed.contains(appid) { return Err(LibraryError::Archive("Unexpected Lua App ID.".into())); }
            let game = appid.parse::<u32>().ok().and_then(|id| library.as_ref()?.game_by_app_id(id));
            let local = game.and_then(managed_lua_path);
            if let Some(target) = local {
                if let Some(parent) = target.parent() { fs::create_dir_all(parent).map_err(|error| LibraryError::io(parent, error))?; }
                atomic_write(&target, &data)?;
                lua_files += 1;
            } else if let Some(dir) = &lua_dir {
                fs::create_dir_all(dir).map_err(|error| LibraryError::io(dir, error))?;
                atomic_write(&dir.join(format!("{appid}.lua")), &data)?;
                lua_files += 1;
            } else {
                warnings.push(format!("Lua manifest for App {appid} was skipped: no local game or Steam install."));
            }
        } else if let Some(rest) = name.strip_prefix("appmanifests/appmanifest_") {
            let appid = rest.strip_suffix(".acf").ok_or_else(|| LibraryError::Archive("Invalid appmanifest entry.".into()))?;
            if !allowed.contains(appid) { return Err(LibraryError::Archive("Unexpected appmanifest App ID.".into())); }
            if let Some(library) = &steam_library {
                atomic_write(&library.join(format!("appmanifest_{appid}.acf")), &data)?;
                appmanifests += 1;
            } else {
                warnings.push(format!("appmanifest for App {appid} was skipped: Steam is not installed."));
            }
        } else if name.starts_with("fixes/") {
            warnings.push(format!("Fixes entry {name} was skipped: fixes are covered by per-game backups."));
        }
    }
    Ok(RestoreSummary {
        games: manifest.games.len(),
        lua_files,
        appmanifests,
        fixes: 0,
        library: library_imported,
        warnings,
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


#[derive(Debug, Serialize)]
pub struct GameExportSummary {
    pub title: String,
    pub path: String,
    pub lua: bool,
    pub manifests: usize,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct LibraryExportSummary {
    pub folder: String,
    pub games: usize,
    pub files: usize,
    pub bytes: u64,
    pub generated_lua: usize,
    pub missing_manifests: usize,
    pub exports: Vec<GameExportSummary>,
}

fn export_safe_name(title: &str) -> String {
    let mut value: String = title
        .chars()
        .map(|character| if matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0') { '_' } else { character })
        .collect();
    value = value.trim().trim_matches('.').to_owned();
    if value.is_empty() { "Game".into() } else { value.chars().take(120).collect() }
}

fn lua_manifest_ids(lua: &str) -> Vec<(String, String)> {
    let mut depots = Vec::new();
    for line in lua.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("setManifestid(") else { continue };
        let mut parts = rest.splitn(2, ',');
        let depot = parts.next().unwrap_or("").trim().to_owned();
        let tail = parts.next().unwrap_or("").trim();
        let Some(start) = tail.find('"') else { continue };
        let Some(end) = tail[start + 1..].find('"') else { continue };
        let manifest = tail[start + 1..start + 1 + end].to_owned();
        if !depot.is_empty() && !manifest.is_empty() {
            depots.push((depot, manifest));
        }
    }
    depots
}

pub fn game_appmanifest(game: &steamtron_core::library::Game) -> serde_json::Value {
    let folder = game.installation.root.file_name().and_then(|name| name.to_str()).unwrap_or("Game");
    let prefix_name = game.installation.prefix.file_name().and_then(|name| name.to_str()).unwrap_or(folder);
    let primary = game.launch_targets.iter().find(|target| target.primary).or_else(|| game.launch_targets.first());
    let launcher_relative = primary
        .and_then(|target| target.executable.strip_prefix(&game.installation.root).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let targets = game.launch_targets.iter().map(|target| {
        serde_json::json!({
            "id": target.id,
            "name": target.name,
            "executable": target.executable.strip_prefix(&game.installation.root)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| target.executable.to_string_lossy().into_owned()),
            "arguments": target.arguments,
            "primary": target.primary,
        })
    }).collect::<Vec<_>>();
    serde_json::json!({
        "format": 1,
        "game_id": game.id,
        "title": game.title,
        "steam_app_id": game.metadata_app_id,
        "content_directory": format!("common/{folder}"),
        "fixes_directory": format!("fixes/{folder}"),
        "launcher_relative_path": launcher_relative,
        "compatdata_directory": format!("compatdata/{prefix_name}"),
        "proton": game.launch.proton,
        "launch_options": game.launch.arguments,
        "launch_targets": targets,
    })
}

/// Export one ZIP per library game: metadata.json, appmanifest JSON,
/// lua/<appid>.lua, and every depot manifest. Missing Lua/manifests are
/// generated from the signed-in API.
pub fn export_game_backups(
    destination: &Path,
    progress: &mut dyn FnMut(usize, usize, &str) -> Result<(), String>,
) -> Result<LibraryExportSummary, LibraryError> {
    use steamtron_core::library::{managed_lua_path, Game};

    fs::create_dir_all(destination).map_err(|error| LibraryError::io(destination, error))?;
    let library = Library::load().map_err(|error| LibraryError::Archive(error.to_string()))?;
    let games: Vec<Game> = library.games().iter().filter(|game| game.metadata_app_id.is_some()).cloned().collect();
    if games.is_empty() {
        return Err(LibraryError::Archive("No managed games to export.".into()));
    }
    let manifests_root = steamtron_core::paths::layout().manifests();
    let _ = manifests_root;
    let total = games.len();
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut generated_lua = 0usize;
    let mut missing_manifests = 0usize;
    let mut summaries = Vec::new();

    for (index, game) in games.iter().enumerate() {
        let appid = game.metadata_app_id.map(|id| id.to_string()).unwrap_or_default();
        let manifests_dir = steamtron_core::paths::layout().manifests_for(&appid);
        fs::create_dir_all(&manifests_dir).map_err(|error| LibraryError::io(&manifests_dir, error))?;
        let title = export_safe_name(&game.title);
        progress(index, total, &format!("Exporting {} of {} · {}", index + 1, total, game.title))
            .map_err(LibraryError::Archive)?;
        let mut lua_path = managed_lua_path(game).filter(|path| path.is_file());
        if lua_path.is_none() {
            if let Ok(root) = steam::steam_root() {
                let candidate = root.join("config/stplug-in").join(format!("{appid}.lua"));
                if candidate.is_file() { lua_path = Some(candidate); }
            }
        }
        let lua = if let Some(path) = lua_path {
            fs::read_to_string(&path).map_err(|error| LibraryError::io(&path, error))?
        } else {
            let bundle = crate::download_bundle(&appid)?;
            generated_lua += 1;
            for (name, data) in &bundle.manifests {
                let safe = Path::new(name).file_name().and_then(|value| value.to_str()).unwrap_or("manifest");
                let _ = fs::write(manifests_dir.join(safe), data);
            }
            bundle.lua
        };
        let mut manifest_data = Vec::new();
        for (depot, manifest) in lua_manifest_ids(&lua) {
            let name = format!("{depot}_{manifest}.manifest");
            let cached = manifests_dir.join(&name);
            if let Ok(data) = fs::read(&cached) {
                manifest_data.push((name, data));
                continue;
            }
            if let Ok(data) = fs::read(manifests_root.join(&name)) {
                manifest_data.push((name, data));
                continue;
            }
            let mut found = None;
            if let Some(path) = managed_lua_path(game).and_then(|path| path.parent().map(|parent| parent.to_path_buf())) {
                if let Ok(data) = fs::read(path.join(&name)) { found = Some(data); }
            }
            if found.is_none() {
                if let Ok(bundle) = crate::download_bundle(&appid) {
                    for (bundle_name, data) in &bundle.manifests {
                        let safe = Path::new(bundle_name).file_name().and_then(|value| value.to_str()).unwrap_or("manifest");
                        let _ = fs::write(manifests_dir.join(safe), data);
                    }
                }
                if let Ok(data) = fs::read(&cached) { found = Some(data); }
            }
            if let Some(data) = found {
                manifest_data.push((name, data));
            } else {
                missing_manifests += 1;
            }
        }

        let zip_path = destination.join(format!("{title}.zip"));
        let temporary = destination.join(format!(".{title}.zip.tmp"));
        let file = fs::File::create(&temporary).map_err(|error| LibraryError::io(&temporary, error))?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let metadata = serde_json::json!({
            "format": 1,
            "created_at": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            "appid": game.metadata_app_id,
            "title": game.title,
            "game": game,
        });
        let encoded = serde_json::to_vec_pretty(&metadata).map_err(|error| LibraryError::Archive(error.to_string()))?;
        let appmanifest = serde_json::to_vec_pretty(&game_appmanifest(game)).map_err(|error| LibraryError::Archive(error.to_string()))?;
        for (name, data) in [
            (format!("{title}/metadata.json"), encoded),
            (format!("{title}/appmanifest_{appid}.json"), appmanifest),
            (format!("{title}/lua/{appid}.lua"), lua.into_bytes()),
        ] {
            zip.start_file(name, options).map_err(LibraryError::Zip)?;
            zip.write_all(&data).map_err(|error| LibraryError::io("backup zip", error))?;
            files += 1;
            bytes = bytes.saturating_add(data.len() as u64);
        }
        let manifest_count = manifest_data.len();
        for (name, data) in manifest_data {
            let entry = format!("{title}/manifests/{name}");
            zip.start_file(entry, options).map_err(LibraryError::Zip)?;
            zip.write_all(&data).map_err(|error| LibraryError::io("backup zip", error))?;
            files += 1;
            bytes = bytes.saturating_add(data.len() as u64);
        }
        let file = zip.finish().map_err(LibraryError::Zip)?;
        file.sync_all().map_err(|error| LibraryError::io(&temporary, error))?;
        fs::rename(&temporary, &zip_path).map_err(|error| LibraryError::io(&zip_path, error))?;
        summaries.push(GameExportSummary {
            title: game.title.clone(),
            path: zip_path.display().to_string(),
            lua: true,
            manifests: manifest_count,
            bytes: fs::metadata(&zip_path).map(|metadata| metadata.len()).unwrap_or(0),
        });
    }
    progress(total, total, "Export complete").map_err(LibraryError::Archive)?;
    Ok(LibraryExportSummary {
        folder: destination.display().to_string(),
        games: total,
        files,
        bytes,
        generated_lua,
        missing_manifests,
        exports: summaries,
    })
}
