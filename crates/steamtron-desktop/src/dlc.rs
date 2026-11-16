//! DLC selection, per-file install records, and SMD downloads.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use steamtron_core::library::Library;

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
    let total = pending.len();
    let mut dl_state = DownloadState {
        game_id: game_id.clone(),
        status: "downloading".into(),
        percent: 0.0,
        completed: 0,
        total: total as u64,
        speed: 0.0,
        average_speed: 0.0,
        depots_total: total as u32,
        depots_done: 0,
        depots_current: 0,
        stalled_seconds: 0,
        elapsed_seconds: 0.0,
        depots: pending.iter().map(|id| dlc_row(&state, *id, "")).collect(),
        last_event_at: 0.0,
        message: "Downloading DLC…".into(),
        log: String::new(),
    };
    download::publish(&app, &dl_state);
    for (index, app_id) in pending.iter().enumerate() {
        let title = format!("DLC {app_id}");
        dl_state.message = format!("Downloading {title} ({} of {total})…", index + 1);
        dl_state.percent = index as f64 / total as f64 * 100.0;
        dl_state.depots_current = index as u32;
        dl_state.depots_done = index as u32;
        download::publish(&app, &dl_state);
        let mut child = Command::new(&smd)
            .arg("--data-dir").arg(cache.join("data"))
            .arg("download").arg(app_id.to_string())
            .args(["--like-steam", "--platform", "windows", "--language", "english", "-o"])
            .arg(&staging)
            .arg("--json")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("could not start the DLC download: {error}"))?;
        let reader = BufReader::new(child.stdout.take().ok_or("DLC downloader produced no output")?);
        let mut base = index as f64 / total as f64 * 100.0;
        let span = 100.0 / total as f64;
        for line in reader.lines().map_while(Result::ok) {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            if value.get("type").and_then(|v| v.as_str()) == Some("depot_complete") {
                if let (Some(current), Some(count)) = (value.get("current").and_then(|v| v.as_u64()), value.get("total").and_then(|v| v.as_u64())) {
                    if count > 0 {
                        dl_state.percent = base + span * current as f64 / count as f64;
                        dl_state.completed = (dl_state.percent * total as f64 / 100.0) as u64;
                        dl_state.depots_current = current as u32;
                        dl_state.depots_done = current as u32;
                        download::publish(&app, &dl_state);
                    }
                }
            }
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        if !status.success() {
            dl_state.status = "error".into();
            dl_state.message = format!("{title} download failed.");
            download::publish(&app, &dl_state);
            let _ = app.emit("download-complete", serde_json::json!({"gameId":game_id,"success":false,"cancelled":false,"message":dl_state.message}));
            return Err(dl_state.message);
        }
        let folder = fs::read_dir(&staging).ok().into_iter().flatten().flatten()
            .map(|entry| entry.path()).find(|path| path.is_dir())
            .ok_or_else(|| format!("{title} download folder was not produced."))?;
        let title = parse_title(&folder, *app_id);
        install_downloaded(&root, &mut state, *app_id, &title, &folder)?;
        save_state(&root, &state)?;
        write_smd_dlc_choice(&root, &state);
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).ok();
    }
    let _ = fs::remove_dir_all(&staging);
    dl_state.status = "done".into();
    dl_state.percent = 100.0;
    dl_state.completed = total as u64;
    dl_state.depots_done = total as u32;
    dl_state.depots_current = total as u32;
    dl_state.message = "DLC download complete.".into();
    download::publish(&app, &dl_state);
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
