//! Portable metadata backup for SteamTron-managed games.
//!
//! Backups contain Lua files, appmanifests, and fixes directories. They never
//! contain game content, prefixes, saves, Steam credentials, or depot payloads.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use steamtron_core::library::{
    game_layout, Artwork, Game, InstallMode, Installation, LaunchSettings, LaunchTarget, Library,
};
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

fn smd_lua_text(content: &str, app_id: u32) -> String {
    let mut output = String::with_capacity(content.len());
    let app = app_id.to_string();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&format!("addappid({app}")) && trimmed.contains(',') {
            let indent = &line[..line.len() - trimmed.len()];
            output.push_str(indent);
            output.push_str(&format!("addappid({app})\n"));
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

/// Export one folder per game using the per-game backup layout without
/// content.tar or fixes.tar: appmanifest JSON plus manifest-bundle.tar.
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
            let mut found = None;
            if let Some(path) = managed_lua_path(game).and_then(|path| path.parent().map(|parent| parent.to_path_buf())) {
                if let Ok(data) = fs::read(path.join(&name)) { found = Some(data); }
            }
            if found.is_none() {
                if let Ok(bundle) = crate::download_bundle(&appid) {
                    if let Some(data) = bundle.manifests.get(&name).cloned() {
                        found = Some(data);
                    }
                    for (bundle_name, data) in &bundle.manifests {
                        let safe = Path::new(bundle_name).file_name().and_then(|value| value.to_str()).unwrap_or("manifest");
                        let _ = fs::write(manifests_dir.join(safe), data);
                    }
                }
            }
            if let Some(data) = found {
                manifest_data.push((name, data));
            } else {
                missing_manifests += 1;
            }
        }

        let stale_folder = destination.join(&title);
        if stale_folder.is_dir()
            && stale_folder.join("manifest-bundle.tar").is_file()
            && !stale_folder.join("content.tar").exists()
            && !stale_folder.join("fixes.tar").exists()
        {
            fs::remove_dir_all(&stale_folder).map_err(|error| LibraryError::io(&stale_folder, error))?;
        }
        let zip_path = destination.join(format!("{title}.zip"));
        let temporary = destination.join(format!(".{title}.zip.tmp"));
        crate::managed::ensure_appmanifest(game)?;
        let appmanifest = if game.installation.manifest.is_file() {
            fs::read(&game.installation.manifest).map_err(|error| LibraryError::io(&game.installation.manifest, error))?
        } else {
            serde_json::to_vec_pretty(&game_appmanifest(game)).map_err(|error| LibraryError::Archive(error.to_string()))?
        };

        let mut tar_bytes = Vec::new();
        let manifest_count = manifest_data.len();
        {
            let mut archive = tar::Builder::new(&mut tar_bytes);
            let top = format!("manifests/{title}");
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(0o755);
            header.set_cksum();
            archive.append_data(&mut header, &top, std::io::empty()).map_err(|error| LibraryError::Archive(error.to_string()))?;
            let mut append = |name: &str, data: &[u8]| -> Result<(), LibraryError> {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                archive.append_data(&mut header, format!("{top}/{name}"), data)
                    .map_err(|error| LibraryError::Archive(error.to_string()))?;
                Ok(())
            };
            append(&format!("{appid}.lua"), lua.as_bytes())?;
            if let Some(id) = game.metadata_app_id {
                append(&format!("{id}.smd.lua"), smd_lua_text(&lua, id).as_bytes())?;
            }
            for (name, data) in manifest_data {
                append(&name, &data)?;
            }
            archive.finish().map_err(|error| LibraryError::Archive(error.to_string()))?;
        }
        {
            let file = fs::File::create(&temporary).map_err(|error| LibraryError::io(&temporary, error))?;
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file(format!("{title}/appmanifest_{appid}.json"), options).map_err(LibraryError::Zip)?;
            zip.write_all(&appmanifest).map_err(|error| LibraryError::io(&temporary, error))?;
            zip.start_file(format!("{title}/manifest-bundle.tar"), options).map_err(LibraryError::Zip)?;
            zip.write_all(&tar_bytes).map_err(|error| LibraryError::io(&temporary, error))?;
            let file = zip.finish().map_err(LibraryError::Zip)?;
            file.sync_data().map_err(|error| LibraryError::io(&temporary, error))?;
            files += 2;
            bytes = bytes.saturating_add(appmanifest.len() as u64 + tar_bytes.len() as u64);
            fs::rename(&temporary, &zip_path).map_err(|error| LibraryError::io(&zip_path, error))?;
            summaries.push(GameExportSummary {
                title: game.title.clone(),
                path: zip_path.display().to_string(),
                lua: true,
                manifests: manifest_count,
                bytes: fs::metadata(&zip_path).map(|metadata| metadata.len()).unwrap_or(0),
            });
        }
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


#[derive(Debug, Serialize)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

#[derive(Default)]
struct ImportedGame {
    appmanifest: Vec<u8>,
    lua: Option<String>,
    manifests: BTreeMap<String, Vec<u8>>,
    source: String,
}

fn parse_manifest_tar(data: &[u8], game: &mut ImportedGame) -> Result<(), LibraryError> {
    let mut archive = tar::Archive::new(Cursor::new(data));
    let entries = archive.entries().map_err(|error| LibraryError::Archive(error.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| LibraryError::Archive(error.to_string()))?;
        let path = entry.path().map_err(|error| LibraryError::Archive(error.to_string()))?;
        let Some(name) = path.file_name().and_then(|value| value.to_str()).map(str::to_owned) else { continue };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|error| LibraryError::Archive(error.to_string()))?;
        if name.ends_with(".lua") && !name.ends_with(".smd.lua") {
            game.lua = Some(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_owned());
        } else if name.ends_with(".manifest") {
            game.manifests.insert(name.to_owned(), bytes);
        }
    }
    Ok(())
}

fn read_folder(path: &Path) -> Result<Option<ImportedGame>, LibraryError> {
    let mut appmanifest = None;
    let mut game = ImportedGame { source: path.display().to_string(), ..Default::default() };
    for entry in fs::read_dir(path).map_err(|error| LibraryError::io(path, error))?.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_file() { continue; }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("appmanifest_") && name.ends_with(".json") {
            appmanifest = Some(fs::read(&entry_path).map_err(|error| LibraryError::io(&entry_path, error))?);
        } else if name == "manifest-bundle.tar" {
            let data = fs::read(&entry_path).map_err(|error| LibraryError::io(&entry_path, error))?;
            parse_manifest_tar(&data, &mut game)?;
        }
    }
    let lua_dir = path.join("lua");
    if let Ok(entries) = fs::read_dir(&lua_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".lua") && !name.ends_with(".smd.lua") {
                game.lua = Some(fs::read_to_string(entry.path()).map_err(|error| LibraryError::io(entry.path(), error))?);
            }
        }
    }
    let manifests_dir = path.join("manifests");
    if let Ok(entries) = fs::read_dir(&manifests_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".manifest") {
                game.manifests.insert(name, fs::read(entry.path()).map_err(|error| LibraryError::io(entry.path(), error))?);
            }
        }
    }
    let Some(appmanifest) = appmanifest else { return Ok(None) };
    game.appmanifest = appmanifest;
    Ok(Some(game))
}

fn read_zip(path: &Path) -> Result<Option<ImportedGame>, LibraryError> {
    let mut archive = ZipArchive::new(fs::File::open(path).map_err(|error| LibraryError::io(path, error))?)
        .map_err(|error| LibraryError::Archive(error.to_string()))?;
    let mut game = ImportedGame { source: path.display().to_string(), ..Default::default() };
    let mut legacy = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| LibraryError::Archive(error.to_string()))?;
        if entry.is_dir() { continue; }
        let name = entry.name().replace('\\', "/");
        let base = name.rsplit('/').next().unwrap_or("").to_owned();
        if name == "steamtron-backup.json" { legacy = true; continue; }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|error| LibraryError::Archive(error.to_string()))?;
        if base.starts_with("appmanifest_") && base.ends_with(".json") {
            game.appmanifest = data;
        } else if base == "manifest-bundle.tar" {
            parse_manifest_tar(&data, &mut game)?;
        } else if base.ends_with(".lua") && !base.ends_with(".smd.lua") {
            game.lua = Some(String::from_utf8_lossy(&data).trim_start_matches('\u{feff}').to_owned());
        } else if base.ends_with(".manifest") {
            game.manifests.insert(base, data);
        }
    }
    if game.appmanifest.is_empty() {
        return Ok(None);
    }
    let _ = legacy;
    Ok(Some(game))
}

fn collect_imports(inputs: &[PathBuf], warnings: &mut Vec<String>) -> Vec<ImportedGame> {
    let mut games = Vec::new();
    for input in inputs {
        if input.is_dir() {
            if let Ok(Some(game)) = read_folder(input) {
                games.push(game);
                continue;
            }
            if let Ok(entries) = fs::read_dir(input) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Ok(Some(game)) = read_folder(&path) { games.push(game); }
                    } else if path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("zip")) {
                        match read_zip(&path) {
                            Ok(Some(game)) => games.push(game),
                            Ok(None) => warnings.push(format!("Skipped {}: no appmanifest found.", path.display())),
                            Err(error) => warnings.push(format!("Skipped {}: {error}", path.display())),
                        }
                    }
                }
            }
        } else if input.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("zip")) {
            match read_zip(input) {
                Ok(Some(game)) => games.push(game),
                Ok(None) => warnings.push(format!("Skipped {}: no appmanifest found.", input.display())),
                Err(error) => warnings.push(format!("Skipped {}: {error}", input.display())),
            }
        } else if input.is_file() {
            if let Ok(Some(game)) = read_folder(input.parent().unwrap_or(input)) { games.push(game); }
        }
    }
    games
}

fn import_one(library: &mut Library, drive: &Path, imported: &ImportedGame) -> Result<bool, LibraryError> {
    let manifest: Value = serde_json::from_slice(&imported.appmanifest)
        .map_err(|error| LibraryError::Archive(error.to_string()))?;
    let appid = manifest.get("steam_app_id").and_then(|value| value.as_u64()).map(|value| value as u32)
        .ok_or_else(|| LibraryError::Archive("Appmanifest has no Steam App ID.".into()))?;
    let title = manifest.get("title").and_then(|value| value.as_str()).unwrap_or("Steam Game").to_owned();
    let game_id = manifest.get("game_id").and_then(|value| value.as_str()).map(str::to_owned).unwrap_or_else(|| appid.to_string());
    let proton = manifest.get("proton").and_then(|value| value.as_str()).unwrap_or("").to_owned();
    let existing = library.game_by_app_id(appid).cloned().or_else(|| library.game(&game_id).cloned());
    let confirmed_before = existing.as_ref().is_some_and(|game| game.installation.confirmed);
    let setup_before = existing.as_ref().and_then(|game| game.setup_completed_at);
    let steam_before = existing.as_ref().and_then(|game| game.steam.clone());
    let artwork_before = existing.as_ref().map(|game| game.artwork.clone()).unwrap_or_default();
    let mut game = existing.unwrap_or_else(|| Game {
        id: game_id.clone(),
        title: title.clone(),
        metadata_app_id: Some(appid),
        metadata_source: None,
        installation: Installation {
            mode: InstallMode::Managed,
            root: PathBuf::new(),
            prefix: PathBuf::new(),
            proton: String::new(),
            manifest: PathBuf::new(),
            confirmed: false,
        },
        steam: None,
        launch: LaunchSettings::default(),
        launch_targets: Vec::new(),
        fixes_enabled: true,
        artwork: Artwork::default(),
        setup_completed_at: None,
    });
    game.title = title.clone();
    game.metadata_app_id = Some(appid);
    if game.installation.root.as_os_str().is_empty() {
        let layout = game_layout(drive, &game).map_err(|error| LibraryError::Archive(error.to_string()))?;
        game.installation.root = layout.content;
        game.installation.prefix = layout.prefix;
        game.installation.manifest = layout.manifest;
    }
    game.installation.proton = proton.clone();
    game.launch.proton = proton;
    if let Some(settings) = manifest.get("launch_settings") {
        if let Some(value) = settings.get("prefix_mode").and_then(|value| value.as_str()) {
            game.launch.prefix_mode = value.to_owned();
        }
        if let Some(value) = settings.get("requires_steam").and_then(|value| value.as_bool()) {
            game.launch.requires_steam = value;
        }
        if let Some(values) = settings.get("environment").and_then(|value| value.as_object()) {
            game.launch.environment = values.iter()
                .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
                .collect();
        }
    }
    if let Some(values) = manifest.get("launch_options").and_then(|value| value.as_array()) {
        game.launch.arguments = values.iter().filter_map(|value| value.as_str().map(str::to_owned)).collect();
    }
    let entries = manifest.get("launch_targets").or_else(|| manifest.get("launcher_targets")).and_then(|value| value.as_array());
    let mut targets = Vec::new();
    if let Some(entries) = entries {
        for (index, entry) in entries.iter().enumerate() {
            let Some(executable) = entry.get("executable").and_then(|value| value.as_str()) else { continue };
            let path = if Path::new(executable).is_absolute() { PathBuf::from(executable) } else { game.installation.root.join(executable) };
            targets.push(LaunchTarget {
                id: entry.get("id").and_then(|value| value.as_str()).map(str::to_owned).unwrap_or_else(|| format!("t{}", index + 1)),
                name: entry.get("name").and_then(|value| value.as_str()).unwrap_or("").to_owned(),
                executable: path,
                arguments: entry.get("arguments").and_then(|value| value.as_array()).map(|values| values.iter().filter_map(|value| value.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
                primary: entry.get("primary").and_then(|value| value.as_bool()).unwrap_or(index == 0),
            });
        }
    }
    if targets.is_empty() {
        if let Some(relative) = manifest.get("launcher_relative_path").and_then(|value| value.as_str()).filter(|value| !value.is_empty()) {
            targets.push(LaunchTarget { id: "t1".into(), name: String::new(), executable: game.installation.root.join(relative), arguments: Vec::new(), primary: true });
        }
    }
    if !targets.is_empty() { game.launch_targets = targets; }
    if let Some(manifest_path) = Some(game.installation.manifest.clone()) {
        if let Some(parent) = manifest_path.parent() { fs::create_dir_all(parent).map_err(|error| LibraryError::io(parent, error))?; }
        fs::write(&manifest_path, &imported.appmanifest).map_err(|error| LibraryError::io(&manifest_path, error))?;
    }
    if let Some(lua) = &imported.lua {
        let app = appid.to_string();
        crate::write_persistent_bundle(&app, lua, &imported.manifests)?;
        crate::managed::write_managed_manifests(&app, &imported.manifests)?;
    }
    game.installation.confirmed = confirmed_before || game.installation.root.is_dir();
    game.setup_completed_at = setup_before.or_else(|| game.installation.confirmed.then_some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64));
    game.steam = steam_before;
    game.artwork = artwork_before;
    library.upsert_game(game).map_err(|error| LibraryError::Archive(error.to_string()))?;
    Ok(true)
}

/// Import metadata folders, one or more ZIPs, or a folder containing ZIPs.
pub fn import_game_metadata(inputs: &[PathBuf]) -> Result<ImportSummary, LibraryError> {
    import_game_metadata_with_progress(inputs, &mut |_, _, _| Ok(()))
}

pub fn import_game_metadata_with_progress(
    inputs: &[PathBuf],
    progress: &mut dyn FnMut(usize, usize, &str) -> Result<(), String>,
) -> Result<ImportSummary, LibraryError> {
    import_game_metadata_at_with_progress(inputs, &Library::default_path(), progress)
}

pub fn import_game_metadata_at(inputs: &[PathBuf], library_path: &Path) -> Result<ImportSummary, LibraryError> {
    import_game_metadata_at_with_progress(inputs, library_path, &mut |_, _, _| Ok(()))
}

pub fn import_game_metadata_at_with_progress(
    inputs: &[PathBuf],
    library_path: &Path,
    progress: &mut dyn FnMut(usize, usize, &str) -> Result<(), String>,
) -> Result<ImportSummary, LibraryError> {
    let mut warnings = Vec::new();
    let games = collect_imports(inputs, &mut warnings);
    if games.is_empty() {
        return Err(LibraryError::Archive("No importable game metadata found.".into()));
    }
    let mut library = Library::open(library_path).map_err(|error| LibraryError::Archive(error.to_string()))?;
    let drive = library.drives().first().map(|drive| drive.root.clone())
        .ok_or_else(|| LibraryError::Archive("Add a library drive before importing.".into()))?;
    let total = games.len();
    let mut imported = 0;
    for (index, game) in games.iter().enumerate() {
        progress(index, total, &format!("Importing {} of {} · {}", index + 1, total, game.source))
            .map_err(LibraryError::Archive)?;
        match import_one(&mut library, &drive, game) {
            Ok(_) => imported += 1,
            Err(error) => warnings.push(format!("Skipped {}: {error}", game.source)),
        }
    }
    progress(total, total, "Import complete").map_err(LibraryError::Archive)?;
    library.save().map_err(|error| LibraryError::Archive(error.to_string()))?;
    Ok(ImportSummary { imported, skipped: total - imported, warnings })
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn imports_a_game_folder_and_writes_the_exact_appmanifest() {
        let temp = tempfile::tempdir().unwrap();
        let drive = temp.path().join("drive");
        fs::create_dir_all(&drive).unwrap();
        let library_path = temp.path().join("library.json");
        let mut library = Library::open(&library_path).unwrap();
        library.add_drive(&drive, "test", true).unwrap();
        library.save().unwrap();

        let folder = temp.path().join("Sample");
        fs::create_dir_all(&folder).unwrap();
        let appmanifest = br#"{"format":1,"game_id":"42","title":"Sample","steam_app_id":42,"content_directory":"common/Sample","launch_targets":[]}"#;
        fs::write(folder.join("appmanifest_42.json"), appmanifest).unwrap();

        let summary = import_game_metadata_at(&[folder], &library_path).unwrap();
        assert_eq!(summary.imported, 1);

        let library = Library::open(&library_path).unwrap();
        let game = library.game_by_app_id(42).expect("imported game");
        assert_eq!(game.title, "Sample");
        assert_eq!(fs::read(&game.installation.manifest).unwrap(), appmanifest);
    }

    #[test]
    fn imports_an_exported_zip() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let temp = tempfile::tempdir().unwrap();
        let drive = temp.path().join("drive");
        fs::create_dir_all(&drive).unwrap();
        let library_path = temp.path().join("library.json");
        let mut library = Library::open(&library_path).unwrap();
        library.add_drive(&drive, "test", true).unwrap();
        library.save().unwrap();

        let zip_path = temp.path().join("Sample.zip");
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let appmanifest = br#"{"format":1,"game_id":"43","title":"Sample Zip","steam_app_id":43,"content_directory":"common/Sample Zip","launch_targets":[]}"#;
        zip.start_file("Sample Zip/appmanifest_43.json", SimpleFileOptions::default()).unwrap();
        zip.write_all(appmanifest).unwrap();
        zip.finish().unwrap();

        let summary = import_game_metadata_at(std::slice::from_ref(&zip_path), &library_path).unwrap();
        assert_eq!(summary.imported, 1);
        let library = Library::open(&library_path).unwrap();
        let game = library.game_by_app_id(43).expect("imported zip game");
        assert_eq!(game.title, "Sample Zip");
    }
}
