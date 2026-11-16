//! DLC selection, per-file install records, and SMD downloads.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use steamtron_core::library::Library;
use steamtron_core::process::hidden_command;

use crate::download::{self, DepotProgress, DownloadState};

const STATE_FILE: &str = ".steamtron-dlc.json";
const BACKUP_DIR: &str = ".steamtron-dlc-backup";
const SMD_DLC_FILE: &str = ".smd-dlcs.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DlcState {
    #[serde(default)]
    pub selected: BTreeSet<u32>,
    #[serde(default)]
    pub installed: BTreeMap<u32, DlcInstall>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DlcInstall {
    pub title: String,
    pub files: BTreeMap<String, DlcFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DlcFile {
    pub sha256: String,
    pub backed_up: bool,
}

fn state_path(root: &Path) -> PathBuf {
    root.join(STATE_FILE)
}

pub fn load_state(root: &Path) -> DlcState {
    fs::read(state_path(root))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

fn save_state(root: &Path, state: &DlcState) -> Result<(), String> {
    let path = state_path(root);
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?)
        .map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, &path).map_err(|error| format!("{}: {error}", path.display()))
}

fn safe_relative(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() { return None; }
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::Normal(value) => result.push(value),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    (!result.as_os_str().is_empty()).then_some(result)
}

fn file_hash(path: &Path) -> Option<String> {
    use sha2::Digest;
    let data = fs::read(path).ok()?;
    Some(sha2::Sha256::digest(&data).iter().map(|byte| format!("{byte:02x}")).collect())
}

fn backup_path(root: &Path, app_id: u32, relative: &Path) -> PathBuf {
    root.join(BACKUP_DIR).join(app_id.to_string()).join(relative)
}

fn write_smd_dlc_choice(root: &Path, state: &DlcState) {
    let mut ids: Vec<String> = state.installed.keys().map(u32::to_string).collect();
    ids.sort();
    let value = serde_json::json!({ "dlcs": ids });
    let _ = fs::write(root.join(SMD_DLC_FILE), serde_json::to_vec_pretty(&value).unwrap_or_default());
}

pub fn set_enabled(game_id: &str, app_id: u32, enabled: bool) -> Result<DlcState, String> {
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library.game(game_id).ok_or_else(|| format!("unknown game {game_id}"))?;
    let root = &game.installation.root;
    let mut state = load_state(root);
    if enabled {
        state.selected.insert(app_id);
        save_state(root, &state)?;
        return Ok(state);
    }
    state.selected.remove(&app_id);
    remove_installed(root, app_id, &mut state);
    save_state(root, &state)?;
    write_smd_dlc_choice(root, &state);
    Ok(state)
}

fn remove_installed(root: &Path, app_id: u32, state: &mut DlcState) {
    let Some(install) = state.installed.remove(&app_id) else { return };
    for (relative, record) in &install.files {
        let Some(relative) = safe_relative(Path::new(relative)) else { continue };
        let destination = root.join(&relative);
        if file_hash(&destination).as_deref() != Some(record.sha256.as_str()) {
            continue;
        }
        if record.backed_up {
            let backup = backup_path(root, app_id, &relative);
            if backup.is_file() {
                if let Some(parent) = destination.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::copy(&backup, &destination);
                let _ = fs::remove_file(&backup);
            }
            continue;
        }
        let _ = fs::remove_file(&destination);
        let mut parent = destination.parent();
        while let Some(directory) = parent {
            if directory == root || fs::remove_dir(directory).is_err() { break; }
            parent = directory.parent();
        }
    }
    let _ = fs::remove_dir_all(root.join(BACKUP_DIR).join(app_id.to_string()));
}

fn walk_files(root: &Path, path: &Path, files: &mut Vec<(PathBuf, PathBuf)>) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?.flatten() {
        let child = entry.path();
        if child.is_dir() {
            let name = child.file_name().and_then(|value| value.to_str()).unwrap_or("");
            if name.starts_with(".DepotDownloader") || name.starts_with(".smd") || name.starts_with(".steamtron") {
                continue;
            }
            walk_files(root, &child, files)?;
        } else if child.is_file() {
            let name = child.file_name().and_then(|value| value.to_str()).unwrap_or("");
            if name.starts_with(".smd") || name.starts_with(".steamtron") || name.starts_with(".DepotDownloader") {
                continue;
            }
            let relative = child.strip_prefix(root).map_err(|error| error.to_string())?.to_path_buf();
            files.push((child, relative));
        }
    }
    Ok(())
}

fn parse_lua_depots(lua: &str) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let manifest = regex::Regex::new(r#"setManifestid\s*\(\s*(\d+)\s*,\s*["'](\d+)["']"#).unwrap();
    let key = regex::Regex::new(r#"addappid\s*\(\s*(\d+)\s*,\s*\d+\s*,\s*["']([0-9a-fA-F]{64})["']"#).unwrap();
    let manifests = manifest.captures_iter(lua)
        .filter_map(|captures| Some((captures[1].to_owned(), captures[2].to_owned())))
        .collect();
    let keys = key.captures_iter(lua)
        .map(|captures| (captures[1].to_owned(), captures[2].to_owned()))
        .collect();
    (manifests, keys)
}

fn download_with_ddm(
    app: &AppHandle,
    state: &mut DownloadState,
    base_appid: u32,
    dlc_app_id: u32,
    depots: &[String],
    bundle: &steamtron_library::Bundle,
    staging: &Path,
    progress_base: f64,
    progress_span: f64,
) -> Result<(), String> {
    let ddm = crate::depot_downloader_path().ok_or("DepotDownloaderMod is not available.")?;
    let (manifests, keys) = parse_lua_depots(&bundle.lua);
    fs::create_dir_all(staging).map_err(|error| format!("{}: {error}", staging.display()))?;
    let key_data = depots.iter().filter_map(|depot| keys.get(depot).map(|key| format!("{depot};{key}\n"))).collect::<String>();
    fs::write(staging.join("steam.keys"), key_data).map_err(|error| error.to_string())?;
    for (index, depot) in depots.iter().enumerate() {
        let manifest = manifests.get(depot).ok_or_else(|| format!("No manifest for DLC depot {depot}."))?;
        let name = format!("{depot}_{manifest}.manifest");
        let data = bundle.manifests.get(&name).ok_or_else(|| format!("No manifest file for DLC depot {depot}."))?;
        fs::write(staging.join(&name), data).map_err(|error| error.to_string())?;
        let mut command = hidden_command(&ddm);
        command
            .current_dir(staging)
            .args(["-app", &base_appid.to_string(), "-depot", depot, "-manifest", manifest, "-depotkeys", "steam.keys", "-manifestfile", &name, "-dir", ".", "-max-downloads", "8"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start DepotDownloaderMod: {error}"))?;
        download::set_active_pid(&state.game_id, child.id());
        state.status = "downloading".into();
        download::publish_and_store(app, state);
        let reader = BufReader::new(child.stdout.take().ok_or("DepotDownloaderMod produced no output")?);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(position) = line.find('%') {
                if let Ok(percent) = line[..position].trim().parse::<f64>() {
                    state.percent = progress_base + progress_span * (index as f64 + percent / 100.0) / depots.len() as f64;
                    if state.total > 0 {
                        state.completed = (state.percent / 100.0 * state.total as f64) as u64;
                    }
                    if let Some(row) = state.depots.iter_mut().find(|row| row.depot_id == dlc_app_id.to_string()) {
                        row.percent = state.percent;
                        row.status = "downloading".into();
                        row.completed = if row.total > 0 { (row.percent / 100.0 * row.total as f64) as u64 } else { 0 };
                    }
                    state.message = format!("Downloading DLC depot {depot}…");
                    download::publish_and_store(app, state);
                }
            }
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        download::clear_active_pid(&state.game_id);
        if !status.success() {
            return Err(format!("DepotDownloaderMod failed for DLC depot {depot}."));
        }
        state.status = "starting".into();
        download::publish_and_store(app, state);
    }
    Ok(())
}

fn install_downloaded(root: &Path, state: &mut DlcState, app_id: u32, title: &str, downloaded: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    walk_files(downloaded, downloaded, &mut files)?;
    if files.is_empty() {
        return Err(format!("DLC {app_id} downloaded no files."));
    }
    let mut install = DlcInstall { title: title.to_owned(), files: BTreeMap::new() };
    let backup_root = root.join(BACKUP_DIR).join(app_id.to_string());
    for (source, relative) in files {
        let Some(relative) = safe_relative(&relative) else { continue };
        let destination = root.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        let backed_up = destination.is_file();
        if backed_up {
            let backup = backup_root.join(&relative);
            if let Some(parent) = backup.parent() {
                fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
            }
            fs::copy(&destination, &backup).map_err(|error| format!("{}: {error}", backup.display()))?;
        }
        fs::copy(&source, &destination).map_err(|error| format!("{}: {error}", destination.display()))?;
        let sha256 = file_hash(&destination).ok_or_else(|| format!("{}: could not hash", destination.display()))?;
        install.files.insert(relative.to_string_lossy().replace('\\', "/"), DlcFile { sha256, backed_up });
    }
    state.installed.insert(app_id, install);
    Ok(())
}

fn parse_title(folder: &Path, app_id: u32) -> String {
    folder.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.split(" - ").skip(1).collect::<Vec<_>>().join(" - "))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("DLC {app_id}"))
}

fn dlc_row(state: &DlcState, app_id: u32, _title: &str) -> DepotProgress {
    DepotProgress {
        depot_id: app_id.to_string(),
        completed: if state.installed.contains_key(&app_id) { 1 } else { 0 },
        total: 1,
        percent: if state.installed.contains_key(&app_id) { 100.0 } else { 0.0 },
        status: if state.installed.contains_key(&app_id) { "done".into() } else { "pending".into() },
    }
}

fn sizes_cache_path() -> PathBuf {
    steamtron_core::paths::layout().cache_dir().join("dlc-sizes.json")
}

fn load_size_cache() -> BTreeMap<String, u64> {
    fs::read(sizes_cache_path())
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

fn save_size_cache(cache: &BTreeMap<String, u64>) {
    let path = sizes_cache_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_vec_pretty(cache) {
        let _ = fs::write(path, data);
    }
}

fn find_block<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)?;
    let open = text[start..].find('{')? + start;
    let mut depth = 0usize;
    for (offset, character) in text[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Default)]
struct PicsDlc {
    sizes: BTreeMap<u32, u64>,
    depots: BTreeMap<u32, Vec<String>>,
}

fn parse_pics_dlc(text: &str) -> PicsDlc {
    let mut result = PicsDlc::default();
    let Some(depots) = find_block(text, "depots") else { return result };
    let mut index = 0usize;
    let bytes = depots.as_bytes();
    while index < bytes.len() {
        if depots.as_bytes()[index] != b'"' { index += 1; continue; }
        let Some(key_end) = depots[index + 1..].find('"').map(|value| index + 1 + value) else { break };
        let key = &depots[index + 1..key_end];
        let Some(open) = depots[key_end + 1..].find('{').map(|value| key_end + 1 + value) else { index = key_end + 1; continue };
        let mut depth = 0usize;
        let mut close = None;
        for (offset, character) in depots[open..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 { close = Some(open + offset); break; }
                }
                _ => {}
            }
        }
        let Some(close) = close else { break };
        if key.chars().all(|character| character.is_ascii_digit()) {
            let block = &depots[open + 1..close];
            if let Some(dlc) = regex::Regex::new(r#""dlcappid"\s+"(\d+)""#).unwrap().captures(block).and_then(|captures| captures[1].parse::<u32>().ok()) {
                let total = regex::Regex::new(r#""size"\s+"(\d+)""#).unwrap()
                    .captures_iter(block)
                    .filter_map(|captures| captures[1].parse::<u64>().ok())
                    .sum::<u64>();
                *result.sizes.entry(dlc).or_insert(0) += total;
                result.depots.entry(dlc).or_default().push(key.to_owned());
            }
        }
        index = close + 1;
    }
    result
}

fn steamcmd_dlc(base_appid: u32) -> PicsDlc {
    let layout = steamtron_core::paths::layout();
    let candidates = [
        layout.tools().join("steamcmd/steamcmd.sh"),
        layout.tools().join("steamcmd/steamcmd.exe"),
    ];
    let Some(steamcmd) = candidates.into_iter().find(|path| path.is_file()) else { return PicsDlc::default() };
    let output = hidden_command(steamcmd)
        .args(["+login", "anonymous", "+app_info_print", &base_appid.to_string(), "+quit"])
        .output();
    let Ok(output) = output else { return PicsDlc::default() };
    parse_pics_dlc(&String::from_utf8_lossy(&output.stdout))
}

/// Sum the depot sizes reported by Steam PICS / SMD for each DLC App ID.
pub fn dlc_sizes(game_id: &str, app_ids: &[u32]) -> BTreeMap<u32, u64> {
    let mut cache = load_size_cache();
    let mut result = BTreeMap::new();
    let mut changed = false;
    let mut sidecar = None;
    let game = Library::load().ok().and_then(|library| library.game(game_id).cloned());
    if let Some(game) = &game {
        sidecar = Some(steamtron_library::cache::load_for_game(game));
    }
    let sidecar_sizes: BTreeMap<u32, u64> = sidecar.as_ref()
        .and_then(|cache| cache.dlc.as_ref())
        .and_then(|value| value.get("entries"))
        .and_then(|value| value.as_array())
        .map(|items| items.iter().filter_map(|item| Some((item.get("app_id")?.as_u64()? as u32, item.get("size")?.as_u64()?))).collect())
        .unwrap_or_default();
    for app_id in app_ids {
        let key = app_id.to_string();
        if let Some(size) = cache.get(&key).copied().or_else(|| sidecar_sizes.get(app_id).copied()) {
            result.insert(*app_id, size);
            continue;
        }
    }
    let missing = app_ids.iter().any(|id| !result.contains_key(id));
    let base_appid = game.as_ref().and_then(|game| game.metadata_app_id);
    let pics = if missing { base_appid.map(steamcmd_dlc).map(|pics| pics.sizes).unwrap_or_default() } else { BTreeMap::new() };
    let smd = if missing { download::resolve_smd().ok() } else { None };
    for app_id in app_ids {
        if result.contains_key(app_id) { continue; }
        let key = app_id.to_string();
        if let Some(size) = pics.get(app_id).copied() {
            cache.insert(key, size);
            result.insert(*app_id, size);
            changed = true;
            continue;
        }
        let Some(smd) = smd.as_ref() else { continue };
        let output = hidden_command(smd)
            .args(["search", &key, "--manifests", "0", "--json"])
            .output();
        let size = match output {
            Ok(output) if output.status.success() => {
                serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()
                    .and_then(|value| value.get("manifests").and_then(|value| value.as_array()).map(|items| {
                        items.iter().filter_map(|item| item.get("size_bytes").and_then(|value| value.as_u64())).sum::<u64>()
                    }))
                    .unwrap_or(0)
            }
            _ => 0,
        };
        cache.insert(key, size);
        result.insert(*app_id, size);
        changed = true;
    }
    if changed { save_size_cache(&cache); }
    if let (Some(game), Some(mut sidecar)) = (game.as_ref(), sidecar.take()) {
        let dlc = sidecar.dlc.get_or_insert_with(|| serde_json::json!({"entries": []}));
        if dlc.get("entries").and_then(|value| value.as_array()).is_none() {
            dlc["entries"] = serde_json::json!([]);
        }
        for (app_id, size) in &result {
            let entries = dlc["entries"].as_array_mut().expect("entries array");
            if let Some(entry) = entries.iter_mut().find(|entry| entry.get("app_id").and_then(|value| value.as_u64()) == Some(*app_id as u64)) {
                entry["size"] = serde_json::json!(size);
            } else {
                entries.push(serde_json::json!({"app_id": app_id, "name": format!("DLC {app_id}"), "released": "", "size": size}));
            }
        }
        let _ = steamtron_library::cache::save_for_game(game, &sidecar);
    }
    result
}

pub fn start(app: AppHandle, game_id: String) -> Result<DownloadState, String> {
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library.game(&game_id).ok_or_else(|| format!("unknown game {game_id}"))?.clone();
    let root = game.installation.root.clone();
    let mut state = load_state(&root);
    let pending: Vec<u32> = state.selected.iter().copied().filter(|id| !state.installed.contains_key(id)).collect();
    if pending.is_empty() {
        return Err("No DLC is queued for download.".into());
    }
    if download::status(&game_id).is_some_and(|state| !matches!(state.status.as_str(), "done" | "error" | "cancelled")) {
        return Err("This game is already downloading.".into());
    }
    let smd = download::resolve_smd()?;
    let cache = steamtron_core::paths::layout().cache_dir().join("smd");
    download::configure_smd(&cache.join("data"))?;
    let staging = cache.join("dlc").join(&game_id);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|error| format!("{}: {error}", staging.display()))?;
    let pics = game.metadata_app_id.map(steamcmd_dlc).unwrap_or_default();
    let base_bundle = game.metadata_app_id
        .and_then(|id| steamtron_library::persistent_bundle(&id.to_string()));
    let total = pending.len();
    let total_bytes: u64 = pending.iter().filter_map(|id| pics.sizes.get(id).copied()).sum();
    let mut dl_state = DownloadState {
        game_id: game_id.clone(),
        status: "starting".into(),
        percent: 0.0,
        completed: 0,
        total: total_bytes,
        speed: 0.0,
        average_speed: 0.0,
        depots_total: total as u32,
        depots_done: 0,
        depots_current: 0,
        stalled_seconds: 0,
        elapsed_seconds: 0.0,
        depots: pending.iter().map(|id| {
            let mut row = dlc_row(&state, *id, "");
            row.total = pics.sizes.get(id).copied().unwrap_or(1);
            row
        }).collect(),
        last_event_at: 0.0,
        message: "Downloading DLC…".into(),
        log: String::new(),
    };
    download::publish_and_store(&app, &dl_state);
    for (index, app_id) in pending.iter().enumerate() {
        let title = format!("DLC {app_id}");
        dl_state.status = "starting".into();
        dl_state.message = format!("Downloading {title} ({} of {total})…", index + 1);
        dl_state.percent = index as f64 / total as f64 * 100.0;
        dl_state.depots_current = index as u32;
        dl_state.depots_done = index as u32;
        download::publish_and_store(&app, &dl_state);
        if let Some(row) = dl_state.depots.iter_mut().find(|row| row.depot_id == app_id.to_string()) {
            row.status = "downloading".into();
        }
        let depots = pics.depots.get(app_id).filter(|depots| !depots.is_empty()).cloned();
        let base = index as f64 / total as f64 * 100.0;
        let span = 100.0 / total as f64;
        let use_ddm = depots.is_some() && base_bundle.is_some();
        if let (Some(depots), Some(bundle)) = (depots.as_ref(), base_bundle.as_ref()) {
            download_with_ddm(&app, &mut dl_state, game.metadata_app_id.unwrap_or(0), *app_id, depots, bundle, &staging, base, span)?;
        } else {
            let mut command = hidden_command(&smd);
            command.arg("--data-dir").arg(cache.join("data"))
                .arg("download").arg(app_id.to_string())
                .args(["--like-steam", "--platform", "windows", "--language", "english", "-o"])
                .arg(&staging)
                .arg("--json");
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
            let mut child = command
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("could not start the DLC download: {error}"))?;
            download::set_active_pid(&game_id, child.id());
            dl_state.status = "downloading".into();
            download::publish_and_store(&app, &dl_state);
            let reader = BufReader::new(child.stdout.take().ok_or("DLC downloader produced no output")?);
            for line in reader.lines().map_while(Result::ok) {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                if value.get("type").and_then(|v| v.as_str()) == Some("depot_complete") {
                    if let (Some(current), Some(count)) = (value.get("current").and_then(|v| v.as_u64()), value.get("total").and_then(|v| v.as_u64())) {
                        if count > 0 {
                            dl_state.percent = base + span * current as f64 / count as f64;
                            dl_state.completed = if dl_state.total > 0 { (dl_state.percent / 100.0 * dl_state.total as f64) as u64 } else { 0 };
                            if let Some(row) = dl_state.depots.iter_mut().find(|row| row.depot_id == app_id.to_string()) {
                                row.percent = dl_state.percent;
                                row.status = "downloading".into();
                                row.completed = if row.total > 0 { (row.percent / 100.0 * row.total as f64) as u64 } else { 0 };
                            }
                            dl_state.depots_current = current as u32;
                            dl_state.depots_done = current as u32;
                            download::publish_and_store(&app, &dl_state);
                        }
                    }
                }
            }
            let status = child.wait().map_err(|error| error.to_string())?;
            download::clear_active_pid(&game_id);
            dl_state.status = "starting".into();
            download::publish_and_store(&app, &dl_state);
            if !status.success() {
                dl_state.status = "error".into();
                dl_state.message = format!("{title} download failed.");
                download::publish_and_store(&app, &dl_state);
                let _ = app.emit("download-complete", serde_json::json!({"gameId":game_id,"success":false,"cancelled":false,"message":dl_state.message}));
                return Err(dl_state.message);
            }
        }
        let folder = if use_ddm {
            staging.clone()
        } else {
            fs::read_dir(&staging).ok().into_iter().flatten().flatten()
                .map(|entry| entry.path()).find(|path| path.is_dir())
                .ok_or_else(|| format!("{title} download folder was not produced."))?
        };
        let title = if use_ddm { title.clone() } else { parse_title(&folder, *app_id) };
        install_downloaded(&root, &mut state, *app_id, &title, &folder)?;
        save_state(&root, &state)?;
        write_smd_dlc_choice(&root, &state);
        if let Some(row) = dl_state.depots.iter_mut().find(|row| row.depot_id == app_id.to_string()) {
            row.percent = 100.0;
            row.status = "done".into();
            row.completed = row.total;
        }
        dl_state.depots_done = index as u32 + 1;
        dl_state.completed = dl_state.depots_done as u64;
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).ok();
    }
    let _ = fs::remove_dir_all(&staging);
    download::clear_active_pid(&game_id);
    if let Some(appid) = game.metadata_app_id {
        steamtron_library::cache::clear_updates_for_appid(appid);
    }
    dl_state.status = "done".into();
    dl_state.percent = 100.0;
    dl_state.completed = total_bytes;
    dl_state.depots_done = total as u32;
    dl_state.depots_current = total as u32;
    dl_state.message = "DLC download complete.".into();
    download::publish_and_store(&app, &dl_state);
    let _ = app.emit("download-complete", serde_json::json!({"gameId":game_id,"success":true,"cancelled":false,"message":dl_state.message}));
    Ok(dl_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninstall_restores_a_file_that_the_dlc_overwrote() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("game");
        let downloaded = temp.path().join("download/DLC");
        fs::create_dir_all(&downloaded).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("shared.dll"), b"base").unwrap();
        fs::write(downloaded.join("shared.dll"), b"dlc").unwrap();
        let mut state = DlcState::default();
        install_downloaded(&root, &mut state, 42, "DLC", &downloaded).unwrap();
        assert_eq!(fs::read(root.join("shared.dll")).unwrap(), b"dlc");
        remove_installed(&root, 42, &mut state);
        assert_eq!(fs::read(root.join("shared.dll")).unwrap(), b"base");
        assert!(!state.installed.contains_key(&42));
    }

    #[test]
    fn installs_and_records_downloaded_dlc_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("game");
        let downloaded = temp.path().join("download/Game DLC");
        fs::create_dir_all(&downloaded).unwrap();
        fs::write(downloaded.join("dlc.pak"), b"dlc").unwrap();
        fs::create_dir_all(&root).unwrap();
        let mut state = DlcState::default();
        install_downloaded(&root, &mut state, 42, "Game DLC", &downloaded).unwrap();
        assert!(root.join("dlc.pak").is_file());
        assert!(state.installed.contains_key(&42));
        assert!(state.installed[&42].files.contains_key("dlc.pak"));
    }
}
