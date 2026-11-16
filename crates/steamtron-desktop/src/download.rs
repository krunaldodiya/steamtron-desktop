//! Steam depot downloads through the `smd` CLI.
//!
//! UmuTron ships `smd` (DepotDownloaderMod) and invokes it with the registered
//! Lua manifest, writing content directly into the managed game folder. We use
//! the same tool and normalize the app line so smd installs one platform.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepotProgress {
    pub depot_id: String,
    pub completed: u64,
    pub total: u64,
    pub percent: f64,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadState {
    pub game_id: String,
    pub status: String,
    pub percent: f64,
    pub completed: u64,
    pub total: u64,
    pub speed: f64,
    pub average_speed: f64,
    pub depots_total: u32,
    pub depots_done: u32,
    pub depots_current: u32,
    pub stalled_seconds: u64,
    pub elapsed_seconds: f64,
    pub depots: Vec<DepotProgress>,
    #[serde(skip)]
    pub last_event_at: f64,
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

const ACTIVE_FILE: &str = "active-download.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveDownload {
    game_id: String,
    pid: u32,
    log: PathBuf,
    root: PathBuf,
}

fn active_path(game_id: &str) -> PathBuf {
    steamtron_core::paths::layout().diagnostics_for(game_id).join(ACTIVE_FILE)
}

pub(crate) fn publish(app: &AppHandle, state: &DownloadState) {
    let _ = app.emit("download-progress", state.clone());
}

const SMD_VERSION: &str = "1.5.0";

#[cfg(target_os = "linux")]
const SMD_URL: &str =
    "https://github.com/mcbabel/Steam-Manifest-Downloader/releases/download/v1.5.0/Steam-Manifest-Downloader-Terminal_1.5.0_linux-x64";
#[cfg(target_os = "linux")]
const SMD_SHA256: &str = "e8819e62b65a9c83d6f71d6369006c3f5ccbaa61d479b5636d97314c0f3021c2";

#[cfg(target_os = "windows")]
const SMD_URL: &str =
    "https://github.com/mcbabel/Steam-Manifest-Downloader/releases/download/v1.5.0/Steam-Manifest-Downloader-Terminal_1.5.0_windows-x64.exe";
#[cfg(target_os = "windows")]
const SMD_SHA256: &str = "7cb56e33f18d0eff8a4dec5edffd8d4f16c41fcfce2605ba783133a0e4cfa138";

fn smd_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "smd.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "smd"
    }
}

fn provision_smd() -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};

    let directory = steamtron_core::paths::layout().tools();
    std::fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let destination = directory.join(smd_name());
    if destination.is_file() {
        return Ok(destination);
    }
    let temporary = directory.join(format!(".{}.{}.download", smd_name(), std::process::id()));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|error| format!("could not create the SMD download client: {error}"))?;
    let mut response = client
        .get(SMD_URL)
        .header("User-Agent", "steamtron-desktop")
        .send()
        .map_err(|error| format!("could not download SMD {SMD_VERSION}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("SMD download returned HTTP {}", response.status()));
    }
    let mut file = std::fs::File::create(&temporary)
        .map_err(|error| format!("{}: {error}", temporary.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|error| format!("SMD download failed: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .map_err(|error| format!("{}: {error}", temporary.display()))?;
    }
    file.sync_all().map_err(|error| format!("{}: {error}", temporary.display()))?;
    drop(file);
    let digest = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    if digest != SMD_SHA256 {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "SMD {SMD_VERSION} failed its integrity check (expected {SMD_SHA256}, got {digest})"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("{}: {error}", temporary.display()))?;
    }
    if destination.is_file() {
        let _ = std::fs::remove_file(&temporary);
        return Ok(destination);
    }
    std::fs::rename(&temporary, &destination)
        .map_err(|error| format!("{}: {error}", destination.display()))?;
    Ok(destination)
}

pub(crate) fn resolve_smd() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(value) = std::env::var_os("STEAMTRON_SMD") {
        candidates.push(PathBuf::from(value));
    }
    candidates.push(steamtron_core::paths::layout().tools().join(smd_name()));
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.push(directory.join(smd_name()));
        }
    }
    if let Some(found) = candidates.into_iter().find(|candidate| candidate.is_file()) {
        return Ok(found);
    }
    provision_smd().map_err(|error| {
        format!("The game downloader (SMD {SMD_VERSION}) is not available and could not be prepared: {error}")
    })
}

pub(crate) fn configure_smd(data_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
    let path = data_dir.join("settings.json");
    let mut settings = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    // SMD's native Rust engine can stall at "chunk task queue built" after the
    // Steam manifest-code fallback. DepotDownloaderMod (the engine SMD ships)
    // completes the same anonymous download quickly and is the stable path.
    settings.insert("use_native_downloader".into(), serde_json::Value::Bool(false));
    let payload = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.steamtron.tmp");
    std::fs::write(&temporary, payload).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn smd_lua_text(content: &str, app_id: u32) -> String {
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

pub(crate) fn smd_lua(app_id: u32, lua_path: &Path, cache: &Path) -> Result<PathBuf, String> {
    let content = std::fs::read_to_string(lua_path)
        .map_err(|error| format!("{}: {error}", lua_path.display()))?;
    std::fs::create_dir_all(cache).map_err(|error| error.to_string())?;
    let target = cache.join(format!("{app_id}.smd.lua"));
    std::fs::write(&target, smd_lua_text(&content, app_id)).map_err(|error| error.to_string())?;
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

fn process_paused(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| stat.rsplit(')').next()?.split_whitespace().next()?.chars().next())
            .is_some_and(|state| state == 'T' || state == 't')
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        false
    }
}

/// Tail SMD's persistent JSON log. The same parser runs during a live download
/// and when reconnecting after SteamTron restarted, so a downloader owned by a
/// previous process can still update the library and UI.
fn monitor(
    app: &AppHandle,
    game_id: &str,
    root: &Path,
    log_path: &Path,
    pid: u32,
    mut child: Option<std::process::Child>,
    mut state: DownloadState,
) {
    use std::io::{BufRead, BufReader};

    let file = match std::fs::File::open(log_path) {
        Ok(file) => file,
        Err(error) => {
            state.status = "error".into();
            state.message = format!("The download log is unavailable: {error}");
            if let Ok(mut states) = states().lock() {
                states.insert(game_id.to_owned(), state.clone());
            }
            finalize(app, game_id, root, None, false, false);
            return;
        }
    };
    if state.status == "queued" {
        state.status = "downloading".into();
        state.message = "Preparing depots…".into();
    }
    if let Ok(mut states) = states().lock() {
        states.insert(game_id.to_owned(), state.clone());
    }
    publish(app, &state);

    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut pending = String::new();
    let mut depot_bytes: HashMap<String, u64> = HashMap::new();
    let mut depot_totals: HashMap<String, u64> = HashMap::new();
    let mut depot_order: Vec<String> = Vec::new();
    let mut depot_finished: Vec<String> = Vec::new();
    let mut current_depot: Option<String> = None;
    let mut download_dir: Option<PathBuf> = None;
    let mut plan_total: u64 = 0;
    let mut total: u64 = 0;
    let mut last_emit = Instant::now();
    let mut last_event = Instant::now();
    let mut depots_total = 0u32;
    let mut depots_done = 0u32;
    let mut depots_current = 0u32;
    let mut last_bytes = 0u64;
    let mut last_time = Instant::now();
    let mut speed = 0.0f64;
    let mut stage_message = "Preparing depots…".to_owned();
    let mut completion_ok: Option<bool> = None;
    let mut failure_message: Option<String> = None;
    let mut reported_percent = 0.0f64;
    let started = Instant::now();
    let mut idle = Instant::now();
    let mut exit_status = None;
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                let alive = match child.as_mut() {
                    Some(child) => match child.try_wait() {
                        Ok(Some(status)) => {
                            exit_status = Some(status);
                            false
                        }
                        Ok(None) => true,
                        Err(_) => false,
                    },
                    None => process_alive(pid),
                };
                if !alive && idle.elapsed() >= Duration::from_millis(400) {
                    break;
                }
                if alive {
                    idle = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Ok(_) => idle = Instant::now(),
            Err(_) => break,
        }
        if !line.ends_with('\n') {
            pending.push_str(&line);
            if pending.len() > 16 * 1024 * 1024 {
                pending.clear();
            }
            continue;
        }
        let full_line = if pending.is_empty() {
            std::mem::take(&mut line)
        } else {
            pending.push_str(&line);
            let value = std::mem::take(&mut pending);
            line.clear();
            value
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(full_line.trim()) else { continue };
        last_event = Instant::now();
        match value.get("type").and_then(|kind| kind.as_str()) {
            Some("plan") => {
                if let Some(depots) = value.get("depots").and_then(|d| d.as_array()) {
                    plan_total = depots
                        .iter()
                        .map(|depot| depot.get("sizeBytes").and_then(|size| size.as_u64()).unwrap_or(0))
                        .sum();
                    depots_total = depots.len() as u32;
                    depot_order = depots
                        .iter()
                        .filter_map(|depot| depot.get("depotId").and_then(|id| id.as_str()).map(str::to_owned))
                        .collect();
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
                if let Some(total) = value.get("total").and_then(|total| total.as_u64()) {
                    if total > 0 { depots_total = total as u32; }
                }
                if let Some(current) = value.get("current").and_then(|current| current.as_u64()) {
                    if current > 0 { depots_current = current as u32; }
                }
                if let Some(depot) = value.get("depotId").and_then(|depot| depot.as_str()) {
                    current_depot = Some(depot.to_owned());
                    if !depot_order.iter().any(|id| id == depot) {
                        depot_order.push(depot.to_owned());
                    }
                }
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
            Some("output") => {
                if let Some(line) = value.get("output").and_then(|output| output.as_str()) {
                    if let Some(position) = line.find('%') {
                        if let Ok(percent) = line[..position].trim().parse::<f64>() {
                            reported_percent = percent.clamp(0.0, 100.0);
                            stage_message = format!("Downloading · {reported_percent:.0}%");
                        }
                    }
                    if let Some(depot) = value.get("depotId").and_then(|depot| depot.as_str()) {
                        if let Some(position) = line.find("Downloaded ") {
                            if let Some(bytes) = line[position + 11..]
                                .split_whitespace()
                                .next()
                                .and_then(|value| value.parse::<u64>().ok())
                            {
                                depot_bytes.insert(depot.to_owned(), bytes);
                                depot_totals.insert(depot.to_owned(), bytes);
                                total = depot_totals.values().sum();
                            }
                        }
                    }
                }
            }
            Some("depot_complete") => {
                depots_done = depots_done.saturating_add(1);
                depots_current = depots_done;
                if let Some(depot) = value.get("depotId").and_then(|depot| depot.as_str()) {
                    if !depot_finished.iter().any(|id| id == depot) {
                        depot_finished.push(depot.to_owned());
                    }
                    if let Some(bytes) = depot_bytes.get(depot).copied() {
                        depot_totals.entry(depot.to_owned()).or_insert(bytes);
                    }
                }
                stage_message = "Finalizing depot…".into();
            }
            Some("error") => {
                failure_message = value
                    .get("message")
                    .and_then(|message| message.as_str())
                    .map(str::to_owned);
                if let Ok(mut states) = states().lock() {
                    if let Some(state) = states.get_mut(game_id) {
                        state.message = failure_message
                            .clone()
                            .unwrap_or_else(|| "download error".into());
                    }
                }
            }
            Some("complete") => {
                let diag = value.get("diag");
                let outcome = diag
                    .and_then(|diag| diag.get("outcome"))
                    .and_then(|outcome| outcome.as_str())
                    .unwrap_or("");
                let params = value.get("params");
                let ok = params.and_then(|params| params.get("ok")).and_then(|ok| ok.as_u64());
                let total = params.and_then(|params| params.get("total")).and_then(|total| total.as_u64());
                let results_ok = value
                    .get("results")
                    .and_then(|results| results.as_array())
                    .map(|results| {
                        results.iter().all(|result| {
                            result.get("error").map_or(true, serde_json::Value::is_null)
                        })
                    })
                    .unwrap_or(true);
                completion_ok = Some(
                    outcome != "failed"
                        && ok.zip(total).map(|(ok, total)| ok >= total).unwrap_or(true)
                        && results_ok,
                );
                if let Some(message) = value.get("message").and_then(|message| message.as_str()) {
                    stage_message = message.to_owned();
                }
            }
            _ => {}
        }
        if let Some(depot) = value.get("depotId").and_then(|depot| depot.as_str()) {
            if !depot_order.iter().any(|id| id == depot) {
                depot_order.push(depot.to_owned());
            }
            current_depot = Some(depot.to_owned());
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
            total = if plan_total > 0 { plan_total } else { depot_totals.values().sum() };
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
            let depots = depot_order
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    let done = depot_finished.iter().any(|finished| finished == id);
                    let depot_completed = *depot_bytes.get(id).unwrap_or(&0);
                    let depot_total = *depot_totals.get(id).unwrap_or(&0);
                    let status = if done || depots_current as usize > index + 1 {
                        "done"
                    } else if current_depot.as_deref() == Some(id) || depots_current as usize == index + 1 {
                        "downloading"
                    } else {
                        "pending"
                    };
                    let depot_percent = if depot_total > 0 {
                        (depot_completed as f64 / depot_total as f64 * 100.0).min(100.0)
                    } else if status == "done" {
                        100.0
                    } else if status == "downloading" {
                        reported_percent
                    } else {
                        0.0
                    };
                    DepotProgress {
                        depot_id: id.clone(),
                        completed: depot_completed,
                        total: depot_total,
                        percent: depot_percent,
                        status: status.into(),
                    }
                })
                .collect::<Vec<_>>();
            if let Ok(mut states) = states().lock() {
                if let Some(state) = states.get_mut(game_id) {
                    state.percent = if total > 0 { percent } else { reported_percent };
                    state.completed = completed;
                    state.total = total;
                    state.depots = depots;
                    if matches!(state.status.as_str(), "paused" | "cancelled") {
                        state.speed = 0.0;
                    } else {
                        state.speed = speed;
                        state.average_speed = completed as f64
                            / started.elapsed().as_secs_f64().max(0.001);
                        state.depots_total = depots_total;
                        state.depots_done = depots_done;
                        state.depots_current = depots_current;
                        state.stalled_seconds = last_event.elapsed().as_secs();
                        state.last_event_at = unix_now();
                        state.status = "downloading".into();
                        state.message = if completed == 0 {
                            if reported_percent > 0.0 {
                                format!("Downloading · {reported_percent:.0}%")
                            } else {
                                stage_message.clone()
                            }
                        } else {
                            format!("Downloading · {}", elapsed_text(started.elapsed()))
                        };
                    }
                    publish(app, state);
                }
            }
        }
    }
    if exit_status.is_none() {
        if let Some(child) = child.as_mut() {
            exit_status = child.wait().ok();
        }
    }
    let exit_success = exit_status
        .map(|status| status.success())
        .unwrap_or(completion_ok == Some(true));
    let cancelled = state.status == "cancelled";
    let success = !cancelled && exit_success && completion_ok.unwrap_or(true) && failure_message.is_none();
    let cleanup = cancelled || failure_message.is_some() || completion_ok == Some(false);
    if !success && !cleanup {
        state.message = "Download interrupted. Click Download to resume.".into();
    }
    finalize(app, game_id, root, download_dir, success, cleanup);
}

fn finalize(app: &AppHandle, game_id: &str, root: &Path, download_dir: Option<PathBuf>, success: bool, cleanup: bool) {
    use steamtron_core::library::Library;

    let mut state = {
        let Ok(mut states) = states().lock() else { return };
        let Some(state) = states.get_mut(game_id) else { return };
        let cancelled = state.status == "cancelled";
        state.status = if cancelled {
            "cancelled".into()
        } else if success {
            "done".into()
        } else {
            "error".into()
        };
        if success {
            state.percent = 100.0;
            state.completed = state.total;
            state.speed = 0.0;
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
    } else {
        if state.status == "cancelled" {
            state.message = "Download cancelled".into();
        }
        // A cancelled or failed download normally leaves smd's preallocated,
        // possibly sparse files. Interrupted downloads keep them so a retry can
        // resume, so only clean up when requested.
        if cleanup && root.is_dir() {
            let _ = std::fs::remove_dir_all(root);
        }
        if state.message.is_empty() {
            state.message = "Download failed. See the diagnostic log.".into();
        }
    }
    if let Ok(mut states) = states().lock() {
        states.insert(game_id.to_owned(), state.clone());
    }
    if let Ok(mut active) = active().lock() {
        active.remove(game_id);
    }
    let _ = std::fs::remove_file(active_path(game_id));
    let _ = app.emit("download-complete", serde_json::json!({
        "gameId": game_id,
        "success": success,
        "cancelled": state.status == "cancelled",
        "message": state.message,
    }));
}

fn bundle_lua_path(game: &steamtron_core::library::Game, cache: &Path) -> Result<PathBuf, String> {
    let app_id = game
        .metadata_app_id
        .ok_or_else(|| "this game has no Steam App ID".to_string())?;
    let bundle = steamtron_library::download_bundle(&app_id.to_string()).map_err(|error| {
        format!("The Lua manifest is missing and could not be read from the bundle store: {error}")
    })?;
    let directory = cache.join("bundles");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{app_id}.lua"));
    std::fs::write(&path, bundle.lua.as_bytes()).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Reconnect to downloader processes that outlived a previous SteamTron run.
/// The sidecar records the PID and log; the monitor replays the log to rebuild
/// progress and finalizes a download that completed while the app was closed.
pub fn restore(app: &AppHandle) {
    let Ok(entries) = std::fs::read_dir(steamtron_core::paths::layout().diagnostics()) else { return };
    for entry in entries.flatten() {
        let path = entry.path().join(ACTIVE_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(meta) = serde_json::from_str::<ActiveDownload>(&text) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        {
            let active = active().lock().map(|active| active.contains_key(&meta.game_id)).unwrap_or(true);
            if active { continue; }
        }
        if !meta.log.is_file() {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let last_event_at = std::fs::metadata(&meta.log)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_secs_f64())
            .unwrap_or_else(unix_now);
        let paused = process_paused(meta.pid);
        let state = DownloadState {
            game_id: meta.game_id.clone(),
            status: if paused { "paused" } else { "downloading" }.into(),
            percent: 0.0,
            completed: 0,
            total: 0,
            speed: 0.0,
            average_speed: 0.0,
            depots_total: 0,
            depots_done: 0,
            depots_current: 0,
            stalled_seconds: (unix_now() - last_event_at).max(0.0) as u64,
            elapsed_seconds: 0.0,
            depots: Vec::new(),
            last_event_at,
            message: if paused { "Download paused" } else { "Resuming download…" }.into(),
            log: meta.log.display().to_string(),
        };
        if let Ok(mut states) = states().lock() {
            states.insert(meta.game_id.clone(), state.clone());
        }
        if let Ok(mut active) = active().lock() {
            active.insert(meta.game_id.clone(), meta.pid);
        }
        publish(app, &state);
        let app = app.clone();
        std::thread::spawn(move || {
            monitor(&app, &meta.game_id, &meta.root, &meta.log, meta.pid, None, state);
        });
    }
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
    let cache = steamtron_core::paths::layout().cache_dir().join("smd");
    let local_lua = steamtron_core::library::managed_lua_path(game)
        .filter(|path| path.is_file());
    let lua = if let Some(path) = local_lua {
        path
    } else if let Ok(steam_root) = steamtron_core::steam::steam_root() {
        let steam_lua = steam_root.join("config/stplug-in").join(format!("{app_id}.lua"));
        if steam_lua.is_file() {
            steam_lua
        } else {
            bundle_lua_path(game, &cache)?
        }
    } else {
        bundle_lua_path(game, &cache)?
    };
    if !lua.is_file() {
        return Err("The Lua manifest is missing and could not be read from the bundle store.".into());
    }
    configure_smd(&cache.join("data"))?;
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
    let stderr_path = logs.join(format!("download-{stamp}.stderr.log"));

    // SMD writes its JSON events to a real file instead of a pipe. The file
    // survives SteamTron exiting, and the next launch tails it to reconnect to
    // the still-running downloader. stderr goes to a separate file so it cannot
    // interleave with and corrupt JSON lines.
    let stdout_log = std::fs::File::create(&log_path).map_err(|error| error.to_string())?;
    let stderr_log = std::fs::File::create(&stderr_path).map_err(|error| error.to_string())?;
    let mut command = Command::new(&smd);
    command
        .arg("--data-dir")
        .arg(cache.join("data"))
        .arg("download")
        .arg(&smd_lua)
        .arg("-o")
        .arg(&root)
        // The verbose flag sends backend diagnostics to stderr instead of the
        // shared log file. SMD 1.5.0's manifest-code fallback can stall when its
        // diagnostics are written to the data-dir log; stderr keeps JSON stdout
        // clean and the fallback completes.
        .args(["--like-steam", "--platform", "windows", "--language", "english", "--json", "-v"])
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("could not start smd: {error}"))?;
    let pid = child.id();
    let active_meta = ActiveDownload {
        game_id: game_id.clone(),
        pid,
        log: log_path.clone(),
        root: root.clone(),
    };
    if let Ok(payload) = serde_json::to_vec_pretty(&active_meta) {
        std::fs::write(active_path(&game_id), payload).map_err(|error| error.to_string())?;
    }
    let return_game_id = game_id.clone();
    let return_log = log_path.display().to_string();
    let queued = DownloadState {
        game_id: game_id.clone(),
        status: "queued".into(),
        percent: 0.0,
        completed: 0,
        total: 0,
        speed: 0.0,
        average_speed: 0.0,
        depots_total: 0,
        depots_done: 0,
        depots_current: 0,
        stalled_seconds: 0,
        elapsed_seconds: 0.0,
        depots: Vec::new(),
        last_event_at: unix_now(),
        message: "Starting smd…".into(),
        log: return_log.clone(),
    };
    if let Ok(mut states) = states().lock() {
        states.insert(game_id.clone(), queued.clone());
    }
    if let Ok(mut active) = active().lock() {
        active.insert(game_id.clone(), pid);
    }
    publish(&app, &queued);
    let monitor_state = queued.clone();
    std::thread::spawn(move || {
        monitor(&app, &game_id, &root, &log_path, pid, Some(child), monitor_state);
    });

    Ok(DownloadState {
        game_id: return_game_id,
        status: "queued".into(),
        percent: 0.0,
        completed: 0,
        total: 0,
        speed: 0.0,
        average_speed: 0.0,
        depots_total: 0,
        depots_done: 0,
        depots_current: 0,
        stalled_seconds: 0,
        elapsed_seconds: 0.0,
        depots: Vec::new(),
        last_event_at: unix_now(),
        message: "Starting smd…".into(),
        log: return_log,
    })
}

pub fn pause(game_id: &str) -> Result<(), String> {
    let pid = active()
        .lock()
        .map_err(|_| "download state lock failed".to_string())?
        .get(game_id)
        .copied()
        .ok_or_else(|| "this game is not downloading".to_string())?;
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-STOP", &format!("-{pid}")])
            .status()
            .map_err(|error| format!("could not pause the downloader: {error}"))?;
        if !status.success() {
            return Err("the downloader process is no longer running".into());
        }
    }
    #[cfg(not(unix))]
    {
        return Err("Pausing downloads is not supported on this platform.".into());
    }
    #[allow(unreachable_code)]
    {
        if let Ok(mut states) = states().lock() {
            if let Some(state) = states.get_mut(game_id) {
                state.status = "paused".into();
                state.message = "Download paused".into();
                state.speed = 0.0;
            }
        }
        Ok(())
    }
}

pub fn resume(game_id: &str) -> Result<(), String> {
    let pid = active()
        .lock()
        .map_err(|_| "download state lock failed".to_string())?
        .get(game_id)
        .copied()
        .ok_or_else(|| "this game is not downloading".to_string())?;
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-CONT", &format!("-{pid}")])
            .status()
            .map_err(|error| format!("could not resume the downloader: {error}"))?;
        if !status.success() {
            return Err("the downloader process is no longer running".into());
        }
    }
    #[cfg(not(unix))]
    {
        return Err("Pausing downloads is not supported on this platform.".into());
    }
    #[allow(unreachable_code)]
    {
        if let Ok(mut states) = states().lock() {
            if let Some(state) = states.get_mut(game_id) {
                state.status = "downloading".into();
                state.message = "Resuming download…".into();
            }
        }
        Ok(())
    }
}

fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0)
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
        // A paused process ignores SIGTERM until it is continued.
        let _ = Command::new("kill").args(["-CONT", &format!("-{pid}")]).status();
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

fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &format!("-{pid}")])
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

pub fn status(game_id: &str) -> Option<DownloadState> {
    let pid = {
        let mut active = active().lock().ok()?;
        let pid = active.get(game_id).copied();
        if pid.is_some_and(|pid| !process_alive(pid)) {
            active.remove(game_id);
        }
        pid
    };
    let running = pid.is_some_and(process_alive);
    let mut states = states().lock().ok()?;
    let state = states.get_mut(game_id)?;
    if !running && !matches!(state.status.as_str(), "done" | "error" | "cancelled") {
        let generic = state.message.is_empty()
            || state.message.starts_with("Downloading ·")
            || [
                "Preparing depots…",
                "Preparing depot downloads…",
                "Starting downloader…",
                "Contacting depot sources…",
                "Fetching depot manifests…",
                "Generating depot keys…",
                "Preparing downloader…",
                "Checking disk space…",
                "Resuming download…",
            ]
            .contains(&state.message.as_str());
        state.status = "error".into();
        if generic {
            state.message = "Download stopped unexpectedly. Start it again.".into();
        }
        state.speed = 0.0;
    }
    if state.status == "downloading" && state.last_event_at > 0.0 {
        state.stalled_seconds = (unix_now() - state.last_event_at).max(0.0) as u64;
    }
    Some(state.clone())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    fn seed(game_id: &str, pid: u32, status: &str) {
        active().lock().unwrap().insert(game_id.into(), pid);
        states().lock().unwrap().insert(
            game_id.into(),
            DownloadState {
                game_id: game_id.into(),
                status: status.into(),
                percent: 0.0,
                completed: 0,
                total: 0,
                speed: 0.0,
                average_speed: 0.0,
                depots_total: 0,
                depots_done: 0,
                depots_current: 0,
                stalled_seconds: 0,
                elapsed_seconds: 0.0,
                depots: Vec::new(),
                last_event_at: unix_now(),
                message: String::new(),
                log: String::new(),
            },
        );
    }

    fn cleanup(game_id: &str) {
        active().lock().unwrap().remove(game_id);
        states().lock().unwrap().remove(game_id);
    }

    fn process_state(pid: u32) -> char {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| stat.rsplit(')').next()?.split_whitespace().next()?.chars().next())
            .unwrap_or('?')
    }

    #[test]
    fn pause_and_resume_stop_and_continue_the_process_group() {
        let game_id = "test-pause-resume";
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = child.id();
        seed(game_id, pid, "downloading");
        pause(game_id).unwrap();
        assert_eq!(status(game_id).unwrap().status, "paused");
        assert_eq!(process_state(pid), 'T');
        resume(game_id).unwrap();
        assert_eq!(status(game_id).unwrap().status, "downloading");
        assert_ne!(process_state(pid), 'T');
        let _ = child.kill();
        let _ = child.wait();
        cleanup(game_id);
    }

    #[test]
    fn cancel_terminates_a_paused_download() {
        let game_id = "test-cancel-paused";
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = child.id();
        seed(game_id, pid, "downloading");
        pause(game_id).unwrap();
        cancel(game_id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(child.try_wait().ok().flatten().is_some(), "paused downloader did not exit");
        assert_eq!(status(game_id).unwrap().status, "cancelled");
        cleanup(game_id);
    }

    #[test]
    fn status_reports_a_live_download_with_no_recent_output_as_stalled() {
        let game_id = "test-stalled";
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        seed(game_id, child.id(), "downloading");
        if let Some(state) = states().lock().unwrap().get_mut(game_id) {
            state.last_event_at = unix_now() - 30.0;
        }
        let state = status(game_id).unwrap();
        assert_eq!(state.status, "downloading");
        assert!(state.stalled_seconds >= 29);
        let _ = child.kill();
        let _ = child.wait();
        cleanup(game_id);
    }

    #[test]
    fn status_marks_a_download_with_a_dead_pid_as_failed() {
        let game_id = "test-dead-pid";
        seed(game_id, u32::MAX, "downloading");
        assert_eq!(status(game_id).unwrap().status, "error");
        cleanup(game_id);
    }

    #[test]
    fn status_marks_a_download_without_a_process_as_failed() {
        let game_id = "test-stale-status";
        states().lock().unwrap().insert(
            game_id.into(),
            DownloadState {
                game_id: game_id.into(),
                status: "downloading".into(),
                percent: 5.0,
                completed: 1,
                total: 20,
                speed: 1.0,
                average_speed: 1.0,
                depots_total: 0,
                depots_done: 0,
                depots_current: 0,
                stalled_seconds: 0,
                elapsed_seconds: 0.0,
                depots: Vec::new(),
                last_event_at: unix_now(),
                message: "Preparing depot downloads…".into(),
                log: String::new(),
            },
        );
        let state = status(game_id).unwrap();
        assert_eq!(state.status, "error");
        assert!(state.message.contains("no longer") || state.message.contains("stopped"));
        cleanup(game_id);
    }
}
