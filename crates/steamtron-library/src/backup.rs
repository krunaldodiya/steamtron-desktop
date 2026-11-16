//! Portable metadata backup for SteamTron-managed games.
//!
//! Backups contain Lua files, appmanifests, and fixes directories. They never
//! contain game content, prefixes, saves, Steam credentials, or depot payloads.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use steamtron_core::library::Library;
use steamtron_core::steam;

use crate::LibraryError;

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
