//! Steam depot downloads through the `smd` CLI.
//!
//! UmuTron ships `smd` (DepotDownloaderMod) and invokes it with the registered
//! Lua manifest, writing content directly into the managed game folder. We use
//! the same tool and normalize the app line so smd installs one platform.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadState {
    pub game_id: String,
    pub status: String,
    pub percent: f64,
    pub completed: u64,
    pub total: u64,
    pub speed: f64,
    pub message: String,
    pub log: String,
}

fn states() -> &'static Mutex<HashMap<String, DownloadState>> {
    static STATES: OnceLock<Mutex<HashMap<String, DownloadState>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn active() -> &'static Mutex<HashMap<String, u32>> {
    static ACTIVE: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn publish(app: &AppHandle, state: &DownloadState) {
    let _ = app.emit("download-progress", state.clone());
}

pub(crate) fn resolve_smd() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(value) = std::env::var_os("STEAMTRON_SMD") {
        candidates.push(PathBuf::from(value));
    }
    candidates.push(steamtron_core::paths::layout().tools().join("smd"));
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.push(directory.join("smd"));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "smd was not found. Install UmuTron or set STEAMTRON_SMD.".to_string())
}

pub(crate) fn smd_lua(app_id: u32, lua_path: &Path, cache: &Path) -> Result<PathBuf, String> {
    let content = std::fs::read_to_string(lua_path)
        .map_err(|error| format!("{}: {error}", lua_path.display()))?;
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
    std::fs::create_dir_all(cache).map_err(|error| error.to_string())?;
    let target = cache.join(format!("{app_id}.smd.lua"));
    std::fs::write(&target, output).map_err(|error| error.to_string())?;
    Ok(target)
}

pub(crate) fn detect_launch_target(root: &Path) -> Option<(PathBuf, String)> {
    let mut best: Option<(u64, PathBuf)> = None;
    let mut stack = vec![root.to_path_buf()];
    let blocked = ["_commonredist", "directx", "vcredist", "support", "tools", "redist"];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if !blocked.iter().any(|block| name.contains(block)) {
                    stack.push(path);
                }
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.ends_with(".exe") {
                continue;
            }
            if ["unins", "setup", "dxsetup", "vcredist", "crashhandler", "unitycrash", "launcher_installer"]
                .iter()
                .any(|skip| name.contains(skip))
            {
                continue;
            }
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            if best.as_ref().map(|(best_size, _)| size > *best_size).unwrap_or(true) {
                best = Some((size, path));
            }
        }
    }
    best.map(|(_, path)| {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Game")
            .to_owned();
        (path, name)
    })
}

fn finalize(app: &AppHandle, game_id: &str, root: &Path, download_dir: Option<PathBuf>, success: bool) {
    use steamtron_core::library::Library;

    let mut state = {
        let Ok(mut states) = states().lock() else { return };
        let Some(state) = states.get_mut(game_id) else { return };
        state.status = if success { "done".into() } else { "error".into() };
        if success {
            state.percent = 100.0;
            state.completed = state.total;
        }
        state.clone()
    };
    if success {
        if let Some(download_dir) = download_dir {
            if download_dir.is_dir() && download_dir != root {
                // smd writes "<AppID> - <Name>" inside the output folder; adopt
                // its contents into the managed game root.
                std::fs::create_dir_all(root).ok();
                let empty = std::fs::read_dir(root)
                    .map(|mut entries| entries.next().is_none())
                    .unwrap_or(true);
                if empty {
                    let _ = std::fs::remove_dir(root);
                    let _ = std::fs::rename(&download_dir, root);
                } else if let Ok(entries) = std::fs::read_dir(&download_dir) {
                    for entry in entries.flatten() {
                        let target = root.join(entry.file_name());
                        if !target.exists() {
                            let _ = std::fs::rename(entry.path(), target);
                        }
                    }
                    let _ = std::fs::remove_dir(&download_dir);
                }
            }
        }
        if let Ok(mut library) = Library::load() {
            if let Some(mut game) = library.game(game_id).cloned() {
                game.installation.confirmed = true;
                if game.launch_targets.is_empty() {
                    if let Some((executable, name)) = detect_launch_target(root) {
                        game.launch_targets = vec![steamtron_core::library::LaunchTarget {
                            id: "t1".into(),
                            name,
                            executable,
                            arguments: Vec::new(),
                            primary: true,
                        }];
                    }
                }
                steamtron_core::library::mark_setup_completed(&mut game);
                let _ = library.upsert_game(game);
                let _ = library.save();
            }
        }
        state.message = "Download complete".into();
    } else if state.message.is_empty() {
        state.message = "Download failed. See the diagnostic log.".into();
    }
    if let Ok(mut states) = states().lock() {
        states.insert(game_id.to_owned(), state.clone());
    }
    if let Ok(mut active) = active().lock() {
        active.remove(game_id);
    }
    let _ = app.emit("download-complete", serde_json::json!({
        "gameId": game_id,
        "success": success,
        "message": state.message,
    }));
}

pub fn start(app: AppHandle, game_id: String) -> Result<DownloadState, String> {
    use steamtron_core::library::Library;

    {
        let active = active().lock().map_err(|_| "download state lock failed".to_string())?;
        if active.contains_key(&game_id) {
            return Err("This game is already downloading.".into());
        }
    }
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    if game.installation.mode == steamtron_core::library::InstallMode::Linked {
        return Err(
            "Linked games are read-only references. Remove the link and download the game to manage its files."
                .into(),
        );
    }
    let app_id = game
        .metadata_app_id
        .ok_or_else(|| "this game has no Steam App ID".to_string())?;
    let root = game.installation.root.clone();
    let lua_dir = steamtron_core::steam::steam_root()
        .map_err(|error| error.to_string())?
        .join("config/stplug-in");
    let lua = lua_dir.join(format!("{app_id}.lua"));
    if !lua.is_file() {
        return Err("The Lua manifest is missing. Re-add the game first.".into());
    }
    let cache = steamtron_core::paths::layout().cache_dir().join("smd");
    let smd_lua = smd_lua(app_id, &lua, &cache)?;
    let smd = resolve_smd()?;
    std::fs::create_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
    let logs = steamtron_core::paths::layout().diagnostics_for(&game_id);
    std::fs::create_dir_all(&logs).map_err(|error| format!("{}: {error}", logs.display()))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let log_path = logs.join(format!("download-{stamp}.log"));

    let mut command = Command::new(&smd);
    command
        .arg("--data-dir")
        .arg(cache.join("data"))
        .arg("download")
        .arg(&smd_lua)
        .arg("-o")
        .arg(&root)
        .args(["--like-steam", "--platform", "windows", "--language", "english", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start smd: {error}"))?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or("smd stdout unavailable")?;
    let mut stderr = child.stderr.take().ok_or("smd stderr unavailable")?;
    let log_file = std::fs::File::create(&log_path).map_err(|error| error.to_string())?;
    let stderr_log = log_file.try_clone().map_err(|error| error.to_string())?;
    let return_game_id = game_id.clone();
    let return_log = log_path.display().to_string();
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut err_out = stderr_log;
        std::thread::spawn(move || {
            let mut reader = BufReader::new(&mut stderr);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let _ = write!(err_out, "{line}");
                line.clear();
            }
        });

        let state = DownloadState {
            game_id: game_id.clone(),
            status: "downloading".into(),
            percent: 0.0,
            completed: 0,
            total: 0,
            speed: 0.0,
            message: "Preparing depots…".into(),
            log: log_path.display().to_string(),
        };
        if let Ok(mut states) = states().lock() {
            states.insert(game_id.clone(), state.clone());
        }
        if let Ok(mut active) = active().lock() {
            active.insert(game_id.clone(), pid);
        }
        publish(&app, &state);

        let mut output = std::fs::OpenOptions::new()
            .append(true)
            .open(&log_path)
            .ok();
        let mut depot_bytes: HashMap<String, u64> = HashMap::new();
        let mut depot_totals: HashMap<String, u64> = HashMap::new();
        let mut download_dir: Option<PathBuf> = None;
        let mut plan_total: u64 = 0;
        let mut total: u64 = 0;
        let mut last_emit = Instant::now();
        let mut last_bytes = 0u64;
        let mut last_time = Instant::now();
        let mut speed = 0.0f64;
        let mut stage_message = "Preparing depots…".to_owned();
        let started = Instant::now();
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            if let Some(file) = output.as_mut() {
                let _ = writeln!(file, "{line}");
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            match value.get("type").and_then(|kind| kind.as_str()) {
                Some("plan") => {
                    if let Some(depots) = value.get("depots").and_then(|d| d.as_array()) {
                        plan_total = depots
                            .iter()
                            .map(|depot| depot.get("sizeBytes").and_then(|size| size.as_u64()).unwrap_or(0))
                            .sum();
                    }
                    stage_message = "Preparing depot downloads…".into();
                }
                Some("started") => {
                    if let Some(dir) = value.get("downloadDir").and_then(|dir| dir.as_str()) {
                        download_dir = Some(PathBuf::from(dir));
                    }
                    stage_message = "Starting downloader…".into();
                }
                Some("status") => {
                    stage_message = match value.get("step").and_then(|step| step.as_str()) {
                        Some("disk_space") => "Checking disk space…",
                        Some("downloading_manifests") => "Fetching depot manifests…",
                        Some("generating_keys") => "Generating depot keys…",
                        Some("keys_generated") => "Preparing downloader…",
                        Some("starting_downloader") => "Starting downloader…",
                        Some("running_downloader") => "Contacting depot sources…",
                        Some("installing_files") => "Installing files…",
                        Some("cancelled_cleanup") => "Cancelling…",
                        _ => "Preparing download…",
                    }
                    .to_owned();
                }
                Some("manifest_source") => {
                    if let Some(message) = value.get("message").and_then(|message| message.as_str()) {
                        stage_message = message.to_owned();
                    }
                }
                Some("error") => {
                    if let Ok(mut states) = states().lock() {
                        if let Some(state) = states.get_mut(&game_id) {
                            state.message = value
                                .get("message")
                                .and_then(|message| message.as_str())
                                .unwrap_or("download error")
                                .to_owned();
                        }
                    }
                }
                _ => {}
            }
            if let Some(depot) = value.get("depotId").and_then(|depot| depot.as_str()) {
                if let Some(bytes) = value.get("networkBytes").and_then(|bytes| bytes.as_u64()) {
                    depot_bytes.insert(depot.to_owned(), bytes);
                } else if let Some(bytes) = value.get("completedBytes").and_then(|bytes| bytes.as_u64()) {
                    depot_bytes.insert(depot.to_owned(), bytes);
                }
                if let Some(depot_total) = value.get("totalBytes").and_then(|bytes| bytes.as_u64()) {
                    if depot_total > 0 {
                        depot_totals.insert(depot.to_owned(), depot_total);
                    }
                }
                total = if plan_total > 0 {
                    plan_total
                } else {
                    depot_totals.values().sum()
                };
            }
            let completed: u64 = depot_bytes.values().sum();
            let percent = if total > 0 {
                (completed as f64 / total as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            let now = Instant::now();
            if now.duration_since(last_time) >= Duration::from_millis(500) {
                speed = (completed.saturating_sub(last_bytes)) as f64
                    / now.duration_since(last_time).as_secs_f64().max(0.001);
                last_bytes = completed;
                last_time = now;
            }
            if now.duration_since(last_emit) >= Duration::from_millis(400) {
                last_emit = now;
                if let Ok(mut states) = states().lock() {
                    if let Some(state) = states.get_mut(&game_id) {
                        state.percent = percent;
                        state.completed = completed;
                        state.total = total;
                        state.speed = speed;
                        state.status = "downloading".into();
                        state.message = if completed == 0 {
                            stage_message.clone()
                        } else {
                            format!("Downloading · {}", elapsed_text(started.elapsed()))
                        };
                        publish(&app, state);
                    }
                }
            }
        }
        let success = child.wait().map(|status| status.success()).unwrap_or(false);
        finalize(&app, &game_id, &root, download_dir, success);
    });

    Ok(DownloadState {
        game_id: return_game_id,
        status: "queued".into(),
        percent: 0.0,
        completed: 0,
        total: 0,
        speed: 0.0,
        message: "Starting smd…".into(),
        log: return_log,
    })
}

fn elapsed_text(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

pub fn cancel(game_id: &str) -> Result<(), String> {
    let pid = active()
        .lock()
        .map_err(|_| "download state lock failed".to_string())?
        .get(game_id)
        .copied()
        .ok_or_else(|| "this game is not downloading".to_string())?;
    #[cfg(unix)]
    {
        let _ = Command::new("kill").args(["-TERM", &format!("-{pid}")]).status();
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status();
    }
    if let Ok(mut states) = states().lock() {
        if let Some(state) = states.get_mut(game_id) {
            state.status = "cancelled".into();
            state.message = "Download cancelled".into();
        }
    }
    Ok(())
}

pub fn status(game_id: &str) -> Option<DownloadState> {
    states().lock().ok()?.get(game_id).cloned()
}
