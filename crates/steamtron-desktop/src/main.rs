#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

mod artwork;
mod download;
mod fix_tools;
mod proton_sources;

use serde::Serialize;
use tauri::Manager;
#[cfg(target_os = "linux")]
use tauri::Emitter;
mod security;
use steamtron_core::config::Config;
use steamtron_core::launch::{clear_managed_launch_option, has_launch_option_value, launch_option_value_with_target, set_launch_option};
use steamtron_core::fixes::{export_name, export_zip, install_zip};
use steamtron_core::steam;
use steamtron_register::{platform_backend, RegistrationBackend};
use steamtron_doctor::{run as run_doctor, launch_unlock_installer};
use steamtron_auth::{login as auth_login, logout as auth_logout, status as auth_status, AuthState};
use steamtron_library::updates::{available_updates, clear_pin, pin_update, Updates};
use steamtron_library::{add_bundle, backup, bundle_from_file, download_bundle, normalize_appid, remove_game as remove_library_game, repair_missing_appmanifests, search as steam_search, title_for_appid, AddedGame, RemovedGame, RepairedGame, SearchResult};

#[derive(Serialize)]
struct GameView {
    appid: String,
    name: String,
    cover: String,
    hero: String,
    build_id: String,
    installed: bool,
    version: Option<String>,
    pinned: Option<String>,
    latest: Option<String>,
    common: String,
    manifest: String,
    managed: bool,
}

#[derive(Serialize)]
struct GameArtwork {
    cover: String,
    hero: String,
}

#[derive(Serialize)]
struct FixView {
    appid: String,
    fixes: String,
    enabled: bool,
    launch_option_set: bool,
    launch_option_error: Option<String>,
    launch_target: Option<String>,
    launch_targets: Vec<steamtron_core::config::LaunchTarget>,
    launch_only: bool,
}

fn fix_view(entry: &steamtron_core::config::GameEntry) -> FixView {
    FixView {
        appid: entry.appid.clone(),
        fixes: entry.fixes.display().to_string(),
        enabled: entry.enabled,
        launch_option_set: launch_option_matches_entry(entry),
        launch_option_error: None,
        launch_target: entry
            .launch_targets
            .first()
            .map(|target| target.executable.display().to_string())
            .or_else(|| entry.launch_target.as_ref().map(|path| path.display().to_string())),
        launch_targets: entry.launch_targets.clone(),
        launch_only: entry.launch_only,
    }
}

#[cfg(unix)]
fn overlay_pids(appid: &str) -> Vec<u32> {
    let needle = format!("/{appid}-");
    let mut result = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return result;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let cwd = std::fs::read_link(entry.path().join("cwd"))
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let cmdline = std::fs::read(entry.path().join("cmdline"))
            .map(|bytes| String::from_utf8_lossy(&bytes).replace('\0', " "))
            .unwrap_or_default();
        let environ = std::fs::read(entry.path().join("environ"))
            .map(|bytes| String::from_utf8_lossy(&bytes).replace('\0', "\n"))
            .unwrap_or_default();
        let belongs_to_overlay = (cwd.contains("steamtron-") && cwd.contains(&needle))
            || (cmdline.contains("steamtron-") && cmdline.contains(&needle) && cmdline.contains("merged"))
            || (cmdline.contains("--steamtron-overlay") && cmdline.contains(&format!("--appid {appid}")))
            || (environ.contains(&format!("SteamAppId={appid}"))
                || environ.contains(&format!("SteamGameId={appid}")));
        if belongs_to_overlay {
            result.push(pid);
        }
    }
    result
}

#[cfg(unix)]
fn stop_overlay_processes(appid: &str) -> Result<(), String> {
    use std::process::Command;
    use std::time::{Duration, Instant};

    let pids = overlay_pids(appid);
    for pid in &pids {
        let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while !overlay_pids(appid).is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    for pid in overlay_pids(appid) {
        let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).status();
    }
    // Drop the FUSE mount if its process was killed rather than asked to exit.
    let mount = std::env::temp_dir().join("steamtron-");
    if let Ok(mountinfo) = std::fs::read_to_string("/proc/self/mountinfo") {
        for line in mountinfo.lines() {
            let Some(fields) = line.split(" - ").next() else { continue };
            let Some(path) = fields.split_whitespace().nth(4) else { continue };
            let mount_text = mount.to_string_lossy();
            if path.starts_with(mount_text.as_ref()) && path.contains(&format!("/{appid}-")) && path.ends_with("/merged") {
                let _ = Command::new("fusermount3").args(["-u", "-z", path]).status();
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn game_process_running(appid: &str) -> bool {
    !overlay_pids(appid).is_empty()
}

#[cfg(windows)]
fn game_process_running(_appid: &str) -> bool {
    false
}

static RUNNING_GAMES: OnceLock<Mutex<HashMap<String, std::process::Child>>> = OnceLock::new();

fn running_games() -> &'static Mutex<HashMap<String, std::process::Child>> {
    RUNNING_GAMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn managed_runs() -> &'static Mutex<HashMap<String, u32>> {
    static RUNS: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn artwork_for_appid(appid: &str) -> (String, String) {
    static CACHE: OnceLock<Mutex<HashMap<String, (String, String)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(value) = cache.lock().ok().and_then(|cache| cache.get(appid).cloned()) {
        return value;
    }
    let artwork = steam_search(appid)
        .ok()
        .and_then(|mut results| results.drain(..).next())
        .map(|result| (result.image, result.hero))
        .unwrap_or_default();
    if let Ok(mut cache) = cache.lock() {
        cache.insert(appid.to_owned(), artwork.clone());
    }
    artwork
}

fn require_auth() -> Result<(), String> {
    if auth_status().authenticated {
        Ok(())
    } else {
        Err("Sign in to SteamTron before using the desktop app.".into())
    }
}

fn steamtron_executable() -> Result<std::path::PathBuf, String> {
    let exe = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| "Could not resolve the SteamTron executable path.".to_string())?;
    Ok(std::path::PathBuf::from(
        exe.display()
            .to_string()
            .strip_suffix(" (deleted)")
            .unwrap_or(&exe.display().to_string()),
    ))
}

fn expected_launch_option(
    entry: &steamtron_core::config::GameEntry,
    game: &steam::SteamGame,
) -> Result<String, String> {
    let exe = steamtron_executable()?;
    let fixes = (!entry.fixes.as_os_str().is_empty()).then_some(entry.fixes.as_path());
    let target = entry.launch_targets.first();
    Ok(launch_option_value_with_target(
        &exe,
        &entry.appid,
        &game.common,
        fixes,
        target.map(|target| target.id.as_str()),
    ))
}

fn launch_option_matches_entry(entry: &steamtron_core::config::GameEntry) -> bool {
    steam::games()
        .ok()
        .and_then(|games| games.into_iter().find(|game| game.appid == entry.appid))
        .and_then(|game| expected_launch_option(entry, &game).ok())
        .is_some_and(|expected| has_launch_option_value(&entry.appid, &expected))
}

fn configure_launch_option(appid: &str, enabled: bool) -> Result<bool, String> {
    if !enabled {
        clear_managed_launch_option(appid).map_err(|error| error.to_string())?;
        return Ok(false);
    }
    let mut config = Config::load().map_err(|error| error.to_string())?;
    let Some(mut entry) = config.entry(appid).cloned() else {
        return Err("No fixes configured for this game.".into());
    };
    if entry.launch_only && entry.fixes.as_os_str().is_empty() {
        let fixes = steamtron_core::fixes::fixes_path(appid)
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&fixes)
            .map_err(|error| format!("Could not create launch fixes directory: {error}"))?;
        config.set_fixes_path(appid, fixes.clone()).map_err(|error| error.to_string())?;
        entry.fixes = fixes;
    }
    let game = steam::games()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|game| game.appid == appid)
        .ok_or_else(|| "This game is not installed in a Steam library.".to_string())?;
    let value = expected_launch_option(&entry, &game)?;
    set_launch_option(appid, Some(&value)).map_err(|error| error.to_string())?;
    Ok(true)
}

#[cfg(target_os = "linux")]
fn require_steam_stopped() -> Result<(), String> {
    if steam_process_running() {
        Err("Steam is running. Exit Steam completely, then retry this action.".into())
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
fn require_steam_stopped() -> Result<(), String> {
    if steam_process_running() {
        Err("Steam is running. Exit Steam completely, then retry this action.".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
fn profile_status() -> AuthState {
    auth_status()
}

#[tauri::command]
fn profile_login() -> Result<AuthState, String> {
    auth_login().map_err(|error| error.to_string())
}

#[tauri::command]
fn profile_logout() -> AuthState {
    auth_logout()
}

#[tauri::command]
fn doctor_report() -> Result<steamtron_doctor::DoctorReport, String> {
    require_auth()?;
    Ok(run_doctor())
}

#[tauri::command]
async fn repair_steam_library() -> Result<String, String> {
    require_auth()?;
    tauri::async_runtime::spawn_blocking(|| {
        if steam::steam_root().is_err() {
            return Err("Steam is not installed. Install Steam and start it once, then retry. SteamTron does not install Steam.".into());
        }
        let installed = || {
            platform_backend()
                .ok()
                .and_then(|backend| backend.status().ok())
                .map(|status| status.ready)
                .unwrap_or(false)
        };
        let marker = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".local/share/steamtron/installer.exit");
        let _ = std::fs::remove_file(&marker);
        require_steam_stopped()?;
        let repair_result = (|| -> Result<String, String> {
            if !installed() {
                launch_unlock_installer()?;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
                loop {
                    if installed() {
                        break;
                    }
                    if let Ok(code) = std::fs::read_to_string(&marker) {
                        let code = code.trim();
                        if !code.is_empty() && code != "0" {
                            return Err(format!(
                                "SLSsteam installer failed with exit code {code}. Check ~/.local/share/steamtron/installer.log."
                            ));
                        }
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err("SLSsteam Moon was not installed within 60 seconds. Check ~/.local/share/steamtron/installer.log for the installer error.".into());
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            let repaired = repair_missing_appmanifests().unwrap_or_default();
            Ok(format!(
                "SLSsteam Moon is installed. Recreated {} appmanifest{}. You can now start Steam manually.",
                repaired.len(),
                if repaired.len() == 1 { "" } else { "s" }
            ))
        })();
        repair_result
    })
    .await
    .map_err(|error| format!("Repair task failed: {error}"))?
}

fn unlock_backend() -> Result<impl RegistrationBackend, String> {
    let backend = platform_backend().map_err(|error| error.to_string())?;
    let status = backend.status().map_err(|error| error.to_string())?;
    if !status.ready {
        return Err("Steam library support is not ready. Open Settings → Doctor.".into());
    }
    Ok(backend)
}

#[tauri::command]
async fn search_games(query: String) -> Result<Vec<SearchResult>, String> {
    require_auth()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut results = steam_search(&query).map_err(|error| error.to_string())?;
        let installed: std::collections::BTreeSet<String> = steam::installed_appids()
            .map_err(|error| error.to_string())?
            .into_iter()
            .chain(
                steam::managed_appids()
                    .map_err(|error| error.to_string())?
                    .into_iter(),
            )
            .collect();
        for result in &mut results {
            result.installed = installed.contains(&result.appid);
        }
        Ok(results)
    })
    .await
    .map_err(|error| format!("Search task failed: {error}"))?
}

#[tauri::command]
fn remove_game(appid: String) -> Result<RemovedGame, String> {
    require_auth()?;
    let backend = platform_backend().map_err(|error| error.to_string())?;
    remove_library_game(&appid, &backend).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_versions(appid: String) -> Result<Updates, String> {
    require_auth()?;
    available_updates(&appid).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_pin(appid: String, date: String) -> Result<Updates, String> {
    require_auth()?;
    pin_update(&appid, &date).map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_update_pin(appid: String) -> Result<Updates, String> {
    require_auth()?;
    clear_pin(&appid).map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
#[tauri::command]
fn install_desktop_entry() -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;

    require_auth()?;
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from).map(|home| home.join(".local/share")))
        .ok_or_else(|| "Could not resolve the user data directory.".to_string())?;
    let applications = data_home.join("applications");
    let icons = data_home.join("icons/hicolor/256x256/apps");
    std::fs::create_dir_all(&applications).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&icons).map_err(|error| error.to_string())?;
    let icon = icons.join("steamtron.png");
    std::fs::write(&icon, include_bytes!("../icons/256x256.png")).map_err(|error| error.to_string())?;
    let exe = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| "Could not resolve the SteamTron executable.".to_string())?;
    let quote = |value: &str| format!("\"{}\"", value.replace('\\', "\\\\").replace('\"', "\\\""));
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=SteamTron\nComment=Steam library, fixes, and updates manager\nExec={}\nIcon={}\nTerminal=false\nCategories=Utility;\nStartupNotify=true\nStartupWMClass=steamtron-desktop\n",
        quote(&exe.display().to_string()),
        icon.display()
    );
    let path = applications.join("steamtron.desktop");
    let temporary = applications.join("steamtron.desktop.steamtron.tmp");
    std::fs::write(&temporary, desktop).map_err(|error| error.to_string())?;
    std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o644)).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    let _ = std::process::Command::new("update-desktop-database").arg(&applications).status();
    Ok(path.display().to_string())
}

#[cfg(not(target_os = "linux"))]
#[tauri::command]
fn install_desktop_entry() -> Result<String, String> {
    Err("Desktop entries are only implemented on Linux.".into())
}

#[tauri::command]
fn repair_launch_options() -> Result<Vec<String>, String> {
    require_auth()?;
    if steam_process_running() {
        return Ok(Vec::new());
    }
    let config = Config::load().map_err(|error| error.to_string())?;
    let mut repaired = Vec::new();
    let mut errors = Vec::new();
    for entry in config.entries().filter(|entry| (entry.enabled || entry.launch_only) && !entry.launch_option_disabled) {
        if launch_option_matches_entry(entry) {
            continue;
        }
        match configure_launch_option(&entry.appid, true) {
            Ok(true) => repaired.push(entry.appid.clone()),
            Ok(false) => {}
            Err(error) => errors.push(format!("{}: {error}", entry.appid)),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(repaired)
}

#[tauri::command]
fn repair_library_entries() -> Result<Vec<RepairedGame>, String> {
    require_auth()?;
    repair_missing_appmanifests().map_err(|error| error.to_string())
}

#[tauri::command]
fn steam_running() -> bool {
    steam_process_running()
}

#[tauri::command]
fn ensure_steam_running() -> Result<(), String> {
    require_auth()?;
    if steam_process_running() {
        Ok(())
    } else {
        Err("Steam is not running. Start Steam, then try Play again.".into())
    }
}

#[tauri::command]
async fn restart_steam() -> Result<String, String> {
    require_auth()?;
    tauri::async_runtime::spawn_blocking(restart_steam_impl)
        .await
        .map_err(|error| format!("Steam restart task failed: {error}"))?
}

#[cfg(target_os = "linux")]
fn steam_process_running() -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        if cmdline
            .split(|byte| *byte == 0)
            .any(|arg| arg == b"steam" || arg.ends_with(b"/steam"))
        {
            return true;
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn steam_process_running() -> bool {
    use std::process::Command;
    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq steam.exe", "/NH"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_ascii_lowercase().contains("steam.exe"))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn stop_steam() -> Result<(), String> {
    use std::process::Command;
    use std::time::{Duration, Instant};

    if !steam_process_running() {
        return Ok(());
    }
    let output = Command::new("taskkill")
        .args(["/IM", "steam.exe", "/F"])
        .output()
        .map_err(|error| format!("Could not stop Steam: {error}"))?;
    if !output.status.success() && steam_process_running() {
        return Err(format!(
            "Steam shutdown failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    while steam_process_running() {
        if Instant::now() >= deadline {
            return Err("Steam did not exit within 60 seconds.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn start_steam() -> Result<(), String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let root = steam::steam_root().map_err(|error| error.to_string())?;
    let executable = root.join("steam.exe");
    Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start Steam: {error}"))?;
    let startup_deadline = Instant::now() + Duration::from_secs(20);
    while !steam_process_running() {
        if Instant::now() >= startup_deadline {
            return Err("Steam did not start within 20 seconds.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn restart_steam_impl() -> Result<String, String> {
    stop_steam()?;
    start_steam()?;
    Ok("Steam restarted.".into())
}

#[cfg(target_os = "linux")]
fn stop_steam() -> Result<(), String> {
    use std::process::Command;
    use std::time::{Duration, Instant};

    if !steam_process_running() {
        return Ok(());
    }
    let output = Command::new("steam")
        .arg("-shutdown")
        .output()
        .map_err(|error| format!("Steam executable not found: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Steam shutdown failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    while steam_process_running() {
        if Instant::now() >= deadline {
            return Err("Steam did not exit within 60 seconds.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn start_steam() -> Result<(), String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    Command::new("setsid")
        .arg("steam")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start Steam: {error}"))?;
    let startup_deadline = Instant::now() + Duration::from_secs(10);
    while !steam_process_running() {
        if Instant::now() >= startup_deadline {
            return Err("Steam did not start within 10 seconds.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    if !steamtron_doctor::steam_injected() {
        return Err("Steam started without SLSsteam injection. Start Steam through the SLSsteam wrapper.".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn restart_steam_impl() -> Result<String, String> {
    stop_steam()?;
    start_steam()?;
    Ok("Steam restarted.".into())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn restart_steam_impl() -> Result<String, String> {
    Err("Steam restart is not implemented on this platform.".into())
}

#[tauri::command]
async fn add_game(appid_or_url: String, title: String) -> Result<AddedGame, String> {
    require_auth()?;
    let appid = normalize_appid(&appid_or_url)
        .ok_or_else(|| "Enter a numeric App ID or Steam store URL.".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let backend = unlock_backend()?;
        let bundle = download_bundle(&appid).map_err(|error| error.to_string())?;
        add_bundle(&appid, &title, bundle, &backend).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Add task failed: {error}"))?
}

#[tauri::command]
fn import_game_file(appid_or_url: String, title: String) -> Result<Option<AddedGame>, String> {
    require_auth()?;
    let appid = normalize_appid(&appid_or_url).ok_or_else(|| "Enter a numeric App ID or Steam store URL.".to_string())?;
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Lua manifest or ZIP", &["lua", "zip"])
        .pick_file()
    else {
        return Ok(None);
    };
    let backend = unlock_backend()?;
    let bundle = bundle_from_file(&path).map_err(|error| error.to_string())?;
    add_bundle(&appid, &title, bundle, &backend)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn backend_status() -> Result<steamtron_register::BackendStatus, String> {
    require_auth()?;
    platform_backend()
        .and_then(|backend| backend.status())
        .map_err(|error| error.to_string())
}

fn notification_summary(operation: &str, success: bool) -> (&'static str, &'static str) {
    match (operation, success) {
        ("backup", true) => ("Backup complete", "complete"),
        ("backup", false) => ("Backup failed", "failed"),
        ("restore", true) => ("Restore complete", "complete"),
        ("restore", false) => ("Restore failed", "failed"),
        ("verify", true) => ("Verification complete", "complete"),
        ("verify", false) => ("Verification failed", "failed"),
        ("download", true) => ("Download complete", "complete"),
        ("download", false) => ("Download failed", "failed"),
        ("library-export", true) => ("Export complete", "complete"),
        ("library-export", false) => ("Export failed", "failed"),
        ("library-import", true) => ("Import complete", "complete"),
        ("library-import", false) => ("Import failed", "failed"),
        (_, true) => ("Task complete", "complete"),
        (_, false) => ("Task failed", "failed"),
    }
}

fn truncate_notification(text: &str, max: usize) -> String {
    let mut value: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        value.push('…');
    }
    value
}

#[tauri::command]
fn notify_task_result(app: tauri::AppHandle, operation: String, game_id: String, success: bool, message: String) {
    let focused = app
        .get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);
    if focused {
        return;
    }
    let game_title = if game_id.is_empty() {
        String::new()
    } else {
        steamtron_core::library::Library::load()
            .ok()
            .and_then(|library| library.game(&game_id).map(|game| game.title.clone()))
            .unwrap_or_default()
    };
    let (summary, _) = notification_summary(&operation, success);
    let detail = if message.trim().is_empty() {
        summary.to_owned()
    } else {
        truncate_notification(message.trim(), 220)
    };
    let body = if game_title.is_empty() {
        detail
    } else {
        format!("{game_title}: {detail}")
    };
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .appname("SteamTron")
            .summary(summary)
            .body(&body)
            .urgency(notify_rust::Urgency::Critical)
            .timeout(notify_rust::Timeout::Milliseconds(15000))
            .show();
    });
}

#[tauri::command]
fn pick_import_paths(mode: Option<String>) -> Vec<String> {
    if mode.as_deref() == Some("folder") {
        rfd::FileDialog::new()
            .set_title("Choose a folder of SteamTron backups")
            .pick_folder()
            .into_iter()
            .map(|path| path.display().to_string())
            .collect()
    } else {
        rfd::FileDialog::new()
            .set_title("Choose SteamTron backup ZIP files")
            .add_filter("SteamTron backup", &["zip"])
            .pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|path| path.display().to_string())
            .collect()
    }
}

#[tauri::command]
async fn import_game_metadata(app: tauri::AppHandle, paths: Vec<String>) -> Result<serde_json::Value, String> {
    require_auth()?;
    let paths: Vec<std::path::PathBuf> = paths.into_iter().map(std::path::PathBuf::from).collect();
    let control = register_operation("library-import", "");
    let emit_app = app.clone();
    let emit_control = control.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        backup::import_game_metadata_with_progress(&paths, &mut |done, total, message| {
            operation_check(&emit_control)?;
            let percent = if total > 0 { done as f64 / total as f64 * 100.0 } else { 0.0 };
            emit_operation_at(&emit_app, "library-import", "", "import", "running", message, percent);
            Ok(())
        })
        .map_err(|error| error.to_string())
    })
    .await;
    unregister_operation("library-import", "");
    let summary = summary.map_err(|error| format!("Import task failed: {error}"))??;
    emit_operation_at(&app, "library-import", "", "done", "done", "Import complete", 100.0);
    serde_json::to_value(summary).map_err(|error| error.to_string())
}

#[tauri::command]
async fn export_game_backups(app: tauri::AppHandle, destination: Option<String>) -> Result<Option<serde_json::Value>, String> {
    require_auth()?;
    let folder = if let Some(destination) = destination {
        std::path::PathBuf::from(destination)
    } else {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose a folder for per-game backups")
            .pick_folder()
        else {
            return Ok(None);
        };
        folder
    };
    let control = register_operation("library-export", "");
    let emit_app = app.clone();
    let emit_control = control.clone();
    let emit_folder = folder.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        backup::export_game_backups(&emit_folder, &mut |done, total, message| {
            operation_check(&emit_control)?;
            let percent = if total > 0 { done as f64 / total as f64 * 100.0 } else { 0.0 };
            emit_operation_at(&emit_app, "library-export", "", "export", "running", message, percent);
            Ok(())
        })
        .map_err(|error| error.to_string())
    })
    .await;
    unregister_operation("library-export", "");
    let summary = summary.map_err(|error| format!("export task failed: {error}"))??;
    emit_operation_at(&app, "library-export", "", "done", "done", "Export complete", 100.0);
    serde_json::to_value(summary)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn game_artwork(appids: Vec<String>) -> Result<HashMap<String, GameArtwork>, String> {
    require_auth()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut artwork = HashMap::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = appids
                .into_iter()
                .map(|appid| {
                    scope.spawn({
                        let appid = appid.clone();
                        move || (appid.clone(), artwork_for_appid(&appid))
                    })
                })
                .collect();
            for handle in handles {
                if let Ok((appid, (cover, hero))) = handle.join() {
                    artwork.insert(appid, GameArtwork { cover, hero });
                }
            }
        });
        Ok(artwork)
    })
    .await
    .map_err(|error| format!("Artwork task failed: {error}"))?
}

#[tauri::command]
async fn list_games() -> Result<Vec<GameView>, String> {
    require_auth()?;
    tauri::async_runtime::spawn_blocking(list_games_impl)
        .await
        .map_err(|error| format!("Library scan failed: {error}"))?
}

fn list_games_impl() -> Result<Vec<GameView>, String> {
    let managed = steam::managed_appids().map_err(|error| error.to_string())?;
    let games = steam::games().map_err(|error| error.to_string())?;
    let mut views: Vec<GameView> = games
        .into_iter()
        .map(|game| {
            let updates = available_updates(&game.appid).ok();
            let version = if game.build_id == "0" {
                updates
                    .as_ref()
                    .map(|updates| updates.latest.clone())
                    .filter(|latest| !latest.is_empty())
            } else {
                None
            };
            let pinned = updates
                .as_ref()
                .map(|updates| updates.pinned.clone())
                .filter(|pinned| !pinned.is_empty());
            let latest = updates
                .as_ref()
                .map(|updates| updates.latest.clone())
                .filter(|latest| !latest.is_empty());
            GameView {
                managed: managed.contains(&game.appid),
                cover: String::new(),
                hero: String::new(),
                appid: game.appid,
                name: game.name,
                build_id: game.build_id.clone(),
                installed: game.state_flags & 4 != 0 && game.size_on_disk > 0 && game.build_id != "0",
                version,
                pinned,
                latest,
                common: game.common.display().to_string(),
                manifest: game.manifest.display().to_string(),
            }
        })
        .collect();
    let libraries = steam::libraries().unwrap_or_default();
    let library = libraries.first();
    for appid in managed {
        if views.iter().any(|view| view.appid == appid) {
            continue;
        }
        let title = title_for_appid(&appid).unwrap_or_else(|| format!("Steam App {appid}"));
        let updates = available_updates(&appid).ok();
        let version = updates
            .as_ref()
            .map(|updates| updates.latest.clone())
            .filter(|latest| !latest.is_empty());
        let pinned = updates
            .as_ref()
            .map(|updates| updates.pinned.clone())
            .filter(|pinned| !pinned.is_empty());
        views.push(GameView {
            managed: true,
            cover: String::new(),
            hero: String::new(),
            appid: appid.clone(),
            name: title.clone(),
            build_id: "0".to_owned(),
            installed: false,
            version: version.clone(),
            pinned,
            latest: version,
            common: library
                .map(|path| path.join("common").join(&title).display().to_string())
                .unwrap_or_default(),
            manifest: String::new(),
        });
    }
    views.sort_by(|left, right| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()));
    Ok(views)
}

#[tauri::command]
fn list_fixes() -> Result<Vec<FixView>, String> {
    require_auth()?;
    let config = Config::load().map_err(|error| error.to_string())?;
    Ok(config
        .entries()
        .map(fix_view)
        .collect())
}

fn single_fix_executable(root: &std::path::Path) -> Option<std::path::PathBuf> {
    fn walk(root: &std::path::Path, current: &std::path::Path, found: &mut Vec<std::path::PathBuf>, depth: usize) {
        if depth > 6 || found.len() > 1 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(current) else { return; };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, found, depth + 1);
            } else if path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("exe")) {
                if let Ok(relative) = path.strip_prefix(root) {
                    found.push(relative.to_path_buf());
                }
            }
            if found.len() > 1 { return; }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found, 0);
    (found.len() == 1).then(|| found.remove(0))
}

#[tauri::command]
fn auto_launch_targets() -> Result<Vec<String>, String> {
    require_auth()?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    let games = steam::games().map_err(|error| error.to_string())?;
    let entries: Vec<_> = config.entries().cloned().collect();
    let mut added = Vec::new();
    for entry in entries {
        if !entry.enabled || entry.fixes.as_os_str().is_empty() || !entry.fixes.is_dir() || !entry.launch_targets.is_empty() {
            continue;
        }
        let Some(relative) = single_fix_executable(&entry.fixes) else { continue; };
        let Some(game) = games.iter().find(|game| game.appid == entry.appid) else { continue; };
        let target = game.common.join(relative);
        if !target.is_file() {
            continue;
        }
        if config
            .add_launch_target(&entry.appid, target, String::new(), Vec::new())
            .is_ok()
        {
            let _ = configure_launch_option(&entry.appid, true);
            added.push(entry.appid);
        }
    }
    Ok(added)
}

#[tauri::command]
fn reconcile_managed_installs() -> Result<usize, String> {
    use steamtron_core::library::Library;

    let mut library = Library::load().map_err(|error| error.to_string())?;
    let games: Vec<_> = library.games().to_vec();
    let mut changed = 0usize;
    for mut game in games {
        let root = game.installation.root.clone();
        if !root.is_dir() {
            continue;
        }
        // A download that outlived the app can leave smd's
        // "<AppID> - <Game>" folder inside the managed root. Promote its
        // contents before detecting a launcher.
        if let Some(appid) = game.metadata_app_id {
            let prefix = format!("{appid} - ");
            let entries: Vec<_> = std::fs::read_dir(&root)
                .map(|entries| entries.flatten().collect())
                .unwrap_or_default();
            let has_files = entries.iter().any(|entry| entry.path().is_file());
            if !has_files {
                for entry in &entries {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !path.is_dir() || !name.starts_with(&prefix) {
                        continue;
                    }
                    if let Ok(children) = std::fs::read_dir(&path) {
                        for child in children.flatten() {
                            let target = root.join(child.file_name());
                            if !target.exists() {
                                let _ = std::fs::rename(child.path(), target);
                            }
                        }
                    }
                    let _ = std::fs::remove_dir(&path);
                    changed += 1;
                    break;
                }
            }
        }
        let malformed = game.launch_targets.iter().find(|target| target.primary).or_else(|| game.launch_targets.first()).is_some_and(|target| {
            let first = std::fs::read(&target.executable).ok().map(|bytes| bytes.get(..2).map(|head| head == b"MZ").unwrap_or(false));
            first == Some(false)
                && std::fs::read_dir(&root).map(|entries| entries.flatten().any(|entry| entry.file_name().to_string_lossy().starts_with(".smd-depot-"))).unwrap_or(false)
        });
        if malformed {
            let _ = std::fs::remove_dir_all(&root);
            game.launch_targets.clear();
            game.installation.confirmed = false;
            changed += 1;
            let _ = library.upsert_game(game);
            continue;
        }
        let installed = root.is_dir()
            && std::fs::read_dir(&root)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(false);
        if installed {
            if !game.installation.confirmed {
                game.installation.confirmed = true;
                changed += 1;
            }
            if game.launch_targets.is_empty() {
                if let Some((executable, name)) = download::detect_launch_target(&root) {
                    game.launch_targets = vec![steamtron_core::library::LaunchTarget {
                        id: "t1".into(),
                        name,
                        executable,
                        arguments: Vec::new(),
                        primary: true,
                    }];
                    changed += 1;
                }
            }
            steamtron_core::library::mark_setup_completed(&mut game);
        }
        if changed > 0 {
            let _ = library.upsert_game(game);
        }
    }
    if changed > 0 {
        library.save().map_err(|error| error.to_string())?;
    }
    Ok(changed)
}

#[tauri::command]
fn auto_pin_installed() -> Result<Vec<String>, String> {
    require_auth()?;
    let managed = steam::managed_appids().map_err(|error| error.to_string())?;
    let mut pinned = Vec::new();
    for appid in managed {
        if steamtron_library::updates::has_existing_pin(&appid) {
            let _ = steamtron_library::updates::ensure_pin_lock(&appid);
            continue;
        }
        match steamtron_library::updates::pin_installed(&appid) {
            Ok(Some(updates)) if !updates.installed.is_empty() => pinned.push(appid),
            Ok(_) => {}
            Err(_) => {}
        }
    }
    Ok(pinned)
}

#[tauri::command]
fn windows_security_supported() -> bool {
    security::supported()
}

#[tauri::command]
fn windows_security_status(appid: String) -> Result<security::SecurityStatus, String> {
    require_auth()?;
    security::status(&appid)
}

#[tauri::command]
fn windows_security_apply(
    appid: String,
    include_game: Option<bool>,
) -> Result<security::SecurityStatus, String> {
    require_auth()?;
    security::apply(&appid, include_game.unwrap_or(true))
}

#[tauri::command]
fn windows_security_remove(appid: String) -> Result<security::SecurityStatus, String> {
    require_auth()?;
    security::remove(&appid)
}

#[tauri::command]
fn windows_security_repair(appid: String) -> Result<security::SecurityStatus, String> {
    require_auth()?;
    security::repair(&appid)
}

fn installed_game(appid: &str) -> Result<steam::SteamGame, String> {
    steam::games()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|game| game.appid == appid)
        .ok_or_else(|| "This game is not installed in a Steam library.".to_string())
}

fn validate_launch_executable(game: &steam::SteamGame, value: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(value)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !path.is_file() {
        return Err("Launch target is not a file.".into());
    }
    if !path.starts_with(&game.common) {
        return Err("Launch target must be inside the game directory.".into());
    }
    Ok(path)
}

#[tauri::command]
fn pick_launch_executable(appid: String) -> Result<Option<String>, String> {
    require_auth()?;
    let game = installed_game(&appid)?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose launch executable")
        .set_directory(&game.common)
        .add_filter("Windows executable", &["exe"])
        .pick_file()
    else {
        return Ok(None);
    };
    let path = validate_launch_executable(&game, &path.display().to_string())?;
    Ok(Some(path.display().to_string()))
}

fn ensure_launch_only_fixes(
    config: &mut Config,
    appid: &str,
) -> Result<(), String> {
    let Some(entry) = config.entry(appid) else { return Ok(()); };
    if !entry.launch_only || !entry.fixes.as_os_str().is_empty() {
        return Ok(());
    }
    let fixes = steamtron_core::fixes::fixes_path(appid)
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&fixes).map_err(|error| format!("Could not create launch fixes directory: {error}"))?;
    config.set_fixes_path(appid, fixes).map_err(|error| error.to_string())
}

#[tauri::command]
fn add_launch_target(
    appid: String,
    executable: String,
    name: Option<String>,
    arguments: Option<Vec<String>>,
) -> Result<FixView, String> {
    require_auth()?;
    let game = installed_game(&appid)?;
    let path = validate_launch_executable(&game, &executable)?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    config
        .add_launch_target(
            &appid,
            path,
            name.unwrap_or_default(),
            arguments.unwrap_or_default(),
        )
        .map_err(|error| error.to_string())?;
    ensure_launch_only_fixes(&mut config, &appid)?;
    let entry = config
        .entry(&appid)
        .ok_or_else(|| "Fixes entry disappeared.".to_string())?;
    let launch = configure_launch_option(&appid, true);
    let mut view = fix_view(entry);
    view.launch_option_error = launch.err();
    Ok(view)
}

#[tauri::command]
fn update_launch_target(
    appid: String,
    id: String,
    name: Option<String>,
    arguments: Option<Vec<String>>,
) -> Result<FixView, String> {
    require_auth()?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    config
        .update_launch_target(
            &appid,
            &id,
            name.unwrap_or_default(),
            arguments.unwrap_or_default(),
            None,
        )
        .map_err(|error| error.to_string())?;
    ensure_launch_only_fixes(&mut config, &appid)?;
    let entry = config
        .entry(&appid)
        .ok_or_else(|| "Fixes entry disappeared.".to_string())?;
    let launch = configure_launch_option(&appid, true);
    let mut view = fix_view(entry);
    view.launch_option_error = launch.err();
    Ok(view)
}

#[tauri::command]
fn remove_launch_target(appid: String, id: String) -> Result<FixView, String> {
    require_auth()?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    config
        .remove_launch_target(&appid, &id)
        .map_err(|error| error.to_string())?;
    ensure_launch_only_fixes(&mut config, &appid)?;
    let entry = config
        .entry(&appid)
        .ok_or_else(|| "Fixes entry disappeared.".to_string())?;
    let launch = if entry.launch_only && entry.launch_targets.is_empty() {
        clear_managed_launch_option(&appid).map(|_| false).map_err(|error| error.to_string())
    } else {
        configure_launch_option(&appid, true)
    };
    let mut view = fix_view(entry);
    view.launch_option_error = launch.err();
    Ok(view)
}

fn play_target_without_overlay(
    appid: &str,
    game: &steam::SteamGame,
    target: &std::path::Path,
    arguments: &[String],
) -> Result<(), String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let (proton, compat) = proton_launcher(appid)
            .ok_or("could not resolve the Steam Proton runtime")?;
        let mut command = std::process::Command::new(proton);
        command
            .arg("waitforexitandrun")
            .arg(target)
            .args(arguments)
            .env("STEAM_COMPAT_DATA_PATH", compat)
            .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam::steam_root().map_err(|error| error.to_string())?)
            .env("SteamAppId", appid)
            .env("SteamGameId", appid)
            .current_dir(&game.common)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let child = command
            .spawn()
            .map_err(|error| format!("Could not start the game: {error}"))?;
        running_games()
            .lock()
            .map_err(|_| "Game state lock failed.".to_string())?
            .insert(appid.to_owned(), child);
        return Ok(());
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let child = std::process::Command::new(target)
            .args(arguments)
            .current_dir(&game.common)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("Could not start the game: {error}"))?;
        running_games()
            .lock()
            .map_err(|_| "Game state lock failed.".to_string())?
            .insert(appid.to_owned(), child);
        Ok(())
    }
}

#[tauri::command]
fn play_game(appid: String, target_id: Option<String>) -> Result<(), String> {
    require_auth()?;
    let config = Config::load().map_err(|error| error.to_string())?;
    let entry = config
        .entry(&appid)
        .ok_or_else(|| "Install fixes and choose a launch target first.".to_string())?;
    // Games without fixes cannot use the overlay path. Let Steam run its
    // native launcher and launch configuration instead of guessing an exe,
    // especially for launchers such as The Witcher 3's REDprelauncher.exe.
    if entry.launch_only {
        return launch_steam_app(&appid, false);
    }
    let selected = target_id
        .as_deref()
        .and_then(|id| entry.launch_targets.iter().find(|target| target.id == id))
        .or_else(|| entry.launch_targets.iter().find(|target| target.primary))
        .or_else(|| entry.launch_targets.first());
    let target = selected
        .map(|target| target.executable.clone())
        .or_else(|| entry.launch_target.clone());
    let game = steam::games()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|game| game.appid == appid)
        .ok_or_else(|| "This game is not installed in a Steam library.".to_string())?;
    let Some(target) = target else {
        if entry.launch_only || !entry.enabled {
            return play_steam_game(appid);
        }
        return launch_steam_app(&appid, false);
    };
    if !target.starts_with(&game.common) {
        return Err("Launch target is outside the game directory.".into());
    }
    if entry.launch_only || !entry.enabled {
        let arguments = selected
            .map(|target| target.arguments.clone())
            .unwrap_or_default();
        if game_process_running(&appid) {
            return Err("This game is already running through SteamTron.".into());
        }
        return play_target_without_overlay(&appid, &game, &target, &arguments);
    }
    let exe = steamtron_executable()?;
    let mut args = vec![
        "--steamtron-overlay".to_owned(),
        "--appid".to_owned(),
        appid.clone(),
        "--common".to_owned(),
        game.common.display().to_string(),
        "--fixes".to_owned(),
        entry.fixes.display().to_string(),
    ];
    let selected_id = selected.map(|target| target.id.clone());
    if let Some(id) = selected_id.as_deref().or(target_id.as_deref()) {
        args.push("--target-id".to_owned());
        args.push(id.to_owned());
    }
    #[cfg(target_os = "linux")]
    args.push("--proton".to_owned());
    args.push("--".to_owned());
    args.push(target.display().to_string());

    if game_process_running(&appid) {
        return Err("This game is already running through SteamTron.".into());
    }
    let mut games = running_games().lock().map_err(|_| "Game state lock failed.")?;
    if games.contains_key(&appid) {
        return Err("This game is already running through SteamTron.".into());
    }
    let mut command = if cfg!(unix) {
        let mut command = std::process::Command::new("setsid");
        command.arg(exe).args(args);
        command
    } else {
        let mut command = std::process::Command::new(exe);
        command.args(args);
        command
    };
    let child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start the game: {error}"))?;
    games.insert(appid, child);
    Ok(())
}

fn launch_steam_app(appid: &str, clear_stale_launch_option: bool) -> Result<(), String> {
    let _game = installed_game(appid)?;
    // A previous SteamTron/UmuLibrary wrapper may still be attached to this
    // app. Steam would run that wrapper instead of the plain game and fail if
    // its old fixes directory no longer exists. A fixes-enabled game without a
    // launch target must keep its managed launch option.
    if clear_stale_launch_option {
        clear_managed_launch_option(appid).map_err(|error| error.to_string())?;
    }
    let steam_root = steam::steam_root().map_err(|error| error.to_string())?;
    let mut steam = if steam_root.join("steam.sh").is_file() {
        steam_root.join("steam.sh")
    } else if steam_root.join("steam").is_file() {
        steam_root.join("steam")
    } else {
        std::path::PathBuf::from("steam")
    };
    if !steam.is_absolute() {
        steam = std::path::PathBuf::from("steam");
    }
    let mut command = if cfg!(unix) {
        let mut command = std::process::Command::new("setsid");
        command.arg(steam).args(["-applaunch", appid]);
        command
    } else {
        let mut command = std::process::Command::new(steam);
        command.args(["-applaunch", appid]);
        command
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start the game through Steam: {error}"))?;
    Ok(())
}

#[tauri::command]
fn play_steam_game(appid: String) -> Result<(), String> {
    require_auth()?;
    launch_steam_app(&appid, true)
}

#[tauri::command]
async fn stop_game(appid: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || stop_game_blocking(appid))
        .await
        .map_err(|error| format!("stop task failed: {error}"))?
}

fn stop_game_blocking(appid: String) -> Result<(), String> {
    require_auth()?;
    let mut games = running_games().lock().map_err(|_| "Game state lock failed.")?;
    let tracked = games.remove(&appid);
    if let Some(mut child) = tracked {
        let _ = child.kill();
    }
    drop(games);
    #[cfg(unix)]
    {
        stop_overlay_processes(&appid)?;
    }
    #[cfg(windows)]
    {
        // The Windows wrapper is tracked by its direct child process; the
        // tracked process was killed above. No Proton process tree is involved.
    }
    Ok(())
}

#[tauri::command]
fn game_running(appid: String) -> Result<bool, String> {
    if game_process_running(&appid) {
        return Ok(true);
    }
    let mut games = running_games().lock().map_err(|_| "Game state lock failed.")?;
    let Some(child) = games.get_mut(&appid) else {
        return Ok(false);
    };
    match child.try_wait().map_err(|error| error.to_string())? {
        Some(_) => {
            games.remove(&appid);
            Ok(false)
        }
        None => Ok(true),
    }
}

#[tauri::command]
fn install_fixes(appid: String) -> Result<Option<FixView>, String> {
    require_auth()?;
    require_steam_stopped()?;
    security::require_exception(&appid)?;
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Fixes ZIP", &["zip"])
        .pick_file()
    else {
        return Ok(None);
    };
    let target = install_zip(&appid, &path).map_err(|error| error.to_string())?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    let entry = config
        .upsert(&appid, target, true)
        .map_err(|error| error.to_string())?;
    config
        .set_launch_option_disabled(&entry.appid, false)
        .map_err(|error| error.to_string())?;
    let launch = configure_launch_option(&entry.appid, true);
    let mut view = fix_view(&entry);
    view.launch_option_error = launch.err();
    Ok(Some(view))
}

#[tauri::command]
fn use_fixes_folder(appid: String) -> Result<Option<FixView>, String> {
    require_auth()?;
    require_steam_stopped()?;
    security::require_exception(&appid)?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose fixes folder")
        .pick_folder()
    else {
        return Ok(None);
    };
    let mut config = Config::load().map_err(|error| error.to_string())?;
    let entry = config
        .upsert(&appid, &path, true)
        .map_err(|error| error.to_string())?;
    config
        .set_launch_option_disabled(&entry.appid, false)
        .map_err(|error| error.to_string())?;
    let launch = configure_launch_option(&entry.appid, true);
    let mut view = fix_view(&entry);
    view.launch_option_error = launch.err();
    Ok(Some(view))
}

#[tauri::command]
fn open_fixes_folder(appid: String) -> Result<(), String> {
    require_auth()?;
    let config = Config::load().map_err(|error| error.to_string())?;
    let entry = config
        .entry(&appid)
        .ok_or_else(|| "No fixes configured for this game.".to_string())?;
    if !entry.fixes.is_dir() {
        return Err(format!(
            "Fixes directory does not exist: {}",
            entry.fixes.display()
        ));
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&entry.fixes)
            .spawn()
            .map_err(|error| format!("Could not open the fixes folder: {error}"))?;
        return Ok(());
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("Opening the fixes folder is only implemented on Linux.".into())
    }
}

#[tauri::command]
fn export_fixes(appid: String, title: Option<String>) -> Result<Option<String>, String> {
    require_auth()?;
    require_steam_stopped()?;
    let config = Config::load().map_err(|error| error.to_string())?;
    let Some(entry) = config.entry(&appid) else {
        return Err("No fixes configured for this game.".into());
    };
    let game_title = title
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            steam::games()
                .ok()
                .and_then(|games| games.into_iter().find(|game| game.appid == appid).map(|game| game.name))
        })
        .unwrap_or_else(|| format!("Steam App {appid}"));
    let name = export_name(&game_title);
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(&name)
        .add_filter("ZIP", &["zip"])
        .save_file()
    else {
        return Ok(None);
    };
    export_zip(&entry.fixes, &path, &appid).map_err(|error| error.to_string())?;
    Ok(Some(path.display().to_string()))
}


#[tauri::command]
fn apply_launch_option(appid: String, enabled: bool) -> Result<String, String> {
    require_auth()?;
    require_steam_stopped()?;
    configure_launch_option(&appid, enabled)?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    config
        .set_launch_option_disabled(&appid, !enabled)
        .map_err(|error| error.to_string())?;
    Ok(if enabled {
        "Steam launch option set. Start Steam manually.".into()
    } else {
        "Steam launch option cleared. Start Steam manually.".into()
    })
}

#[tauri::command]
fn remove_fixes(appid: String) -> Result<(), String> {
    require_auth()?;
    require_steam_stopped()?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    let fixes = config.entry(&appid).map(|entry| entry.fixes.clone());
    config.remove(&appid).map_err(|error| error.to_string())?;
    clear_managed_launch_option(&appid).map_err(|error| error.to_string())?;

    // Removing the entry removes its fixes folder as well. This is deliberate:
    // the folder belongs to that fixes entry, including a folder selected
    // through "Change fixes folder".
    if let Some(fixes) = fixes {
        let fixes = fixes.canonicalize().unwrap_or(fixes);
        if fixes.is_dir() {
            std::fs::remove_dir_all(&fixes)
                .map_err(|error| format!("Could not remove fixes folder {}: {error}", fixes.display()))?;
        }
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn proton_launcher(appid: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    use std::fs;

    let root = steam::steam_root().ok()?;
    let libraries = steam::libraries().ok()?;
    let mut tool = String::new();
    if let Ok(config) = fs::read_to_string(root.join("config/config.vdf")) {
        if let Some(start) = config.find("\"CompatToolMapping\"") {
            let tail = &config[start..];
            let marker = format!("\"{appid}\"");
            if let Some(index) = tail.find(&marker) {
                let block = &tail[index..];
                if let Some(name) = block.find("\"name\"") {
                    let after = &block[name + 6..];
                    if let Some(open) = after.find('"') {
                        if let Some(close) = after[open + 1..].find('"') {
                            tool = after[open + 1..open + 1 + close].to_owned();
                        }
                    }
                }
            }
        }
    }
    let want = match tool.as_str() {
        "proton_experimental" => "experimental",
        "proton_9" => "proton 9",
        "proton_8" => "proton 8",
        "proton_7" => "proton 7",
        _ => "",
    };
    let mut candidates = Vec::new();
    for library in &libraries {
        let common = library.join("common");
        if let Ok(entries) = fs::read_dir(&common) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with("proton") && path.join("proton").is_file() {
                    candidates.push((name, path));
                }
            }
        }
    }
    candidates.sort_by_key(|(name, _)| {
        if !want.is_empty() && name.contains(want) { 0 } else if name.contains("experimental") { 1 } else { 2 }
    });
    let (_, proton_dir) = candidates.into_iter().next()?;
    let proton = proton_dir.join("proton");
    let compat = libraries
        .iter()
        .map(|library| library.join("compatdata").join(appid))
        .find(|path| path.exists())
        .or_else(|| libraries.first().map(|library| library.join("compatdata").join(appid)))?;
    Some((proton, compat))
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn try_overlay_wrapper() -> Option<i32> {
    use steamtron_overlay::{OverlayGame, PlatformOverlay};
    use std::process::Command;

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--steamtron-overlay") {
        return None;
    }
    let mut appid = None;
    let mut common = None;
    let mut fixes = None;
    let mut command = Vec::new();
    let mut use_proton = false;
    let mut target_id = None;
    let mut proton_path = None;
    let mut compat = None;
    let mut managed_cwd = None;
    let mut managed_steam_root = None;
    let mut managed = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--appid" => { appid = args.get(index + 1).cloned(); index += 2; }
            "--common" => { common = args.get(index + 1).cloned(); index += 2; }
            "--fixes" => { fixes = args.get(index + 1).cloned(); index += 2; }
            "--proton" => { use_proton = true; index += 1; }
            "--target-id" => { target_id = args.get(index + 1).cloned(); index += 2; }
            "--proton-path" => { proton_path = args.get(index + 1).cloned(); managed = true; index += 2; }
            "--compat" => { compat = args.get(index + 1).cloned(); index += 2; }
            "--cwd" => { managed_cwd = args.get(index + 1).cloned(); managed = true; index += 2; }
            "--steam-root" => { managed_steam_root = args.get(index + 1).cloned(); index += 2; }
            "--" => { command.extend_from_slice(&args[index + 1..]); break; }
            value => { command.push(value.to_owned()); index += 1; }
        }
    }
    let result = (|| -> Result<i32, String> {
        let appid = appid.ok_or("missing --appid")?;
        let common = common.ok_or("missing --common")?;
        let fixes = fixes.ok_or("missing --fixes")?;
        if command.is_empty() {
            return Err("missing game command after --".into());
        }
        let game = OverlayGame::new(common.clone(), fixes, Some(appid.clone()))
            .map_err(|error| error.to_string())?;
        let mut overlay = PlatformOverlay::new(game);
        let merged = overlay.mount().map_err(|error| error.to_string())?.to_path_buf();
        // Only PlayTron launches pass --target-id. Steam launches must keep
        // using the command Steam prepared; otherwise a configured launcher
        // target silently overrides %command% for Steam Play.
        let selected_target = if managed {
            None
        } else {
            Config::load().ok().and_then(|config| {
                let entry = config.entry(&appid)?;
                target_id
                    .as_deref()
                    .and_then(|id| entry.launch_targets.iter().find(|target| target.id == id))
                    .cloned()
            })
        };
        let mut source_command = command.clone();
        if let Some(target) = selected_target.as_ref() {
            // Steam wraps %command% (steam-launch-wrapper -> runtime -> Proton
            // -> game.exe). Replace only the game executable in that chain so
            // the Proton/runtime wrappers are preserved. A bare PlayTron
            // command is just the executable, which is handled by the same
            // replacement.
            let common_path = std::path::Path::new(&common);
            let target_path = target.executable.display().to_string();
            let replace = command.iter().rposition(|value| {
                let lower = value.to_ascii_lowercase();
                lower.ends_with(".exe")
                    && (std::path::Path::new(value).starts_with(common_path)
                        || value.contains("merged"))
            }).or_else(|| command.iter().rposition(|value| value.to_ascii_lowercase().ends_with(".exe")));
            if let Some(index) = replace {
                source_command[index] = target_path;
                source_command.splice(index + 1..index + 1, target.arguments.iter().cloned());
            } else {
                source_command = vec![target.executable.display().to_string()];
                source_command.extend(target.arguments.iter().cloned());
                source_command.extend(command);
            }
        }
        let rewritten: Vec<String> = source_command
            .iter()
            .map(|value| overlay.game().rewrite_arg(value, &merged))
            .collect();
        let mut process = if let Some(proton) = proton_path.as_deref() {
            let compat = compat
                .as_deref()
                .ok_or("missing --compat for a managed Proton launch")?;
            let client = match managed_steam_root.as_deref() {
                Some(path) => std::path::PathBuf::from(path),
                None => steam::steam_root().map_err(|e| e.to_string())?,
            };
            let mut process = Command::new(proton);
            process
                .arg("waitforexitandrun")
                .arg(&rewritten[0])
                .env("STEAM_COMPAT_DATA_PATH", compat)
                .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", client)
                .env("SteamAppId", &appid)
                .env("SteamGameId", &appid);
            process
        } else if use_proton {
            let (proton, compat) = proton_launcher(&appid)
                .ok_or("could not resolve the Steam Proton runtime")?;
            let mut process = Command::new(proton);
            process
                .arg("waitforexitandrun")
                .arg(&rewritten[0])
                .env("STEAM_COMPAT_DATA_PATH", compat)
                .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", steam::steam_root().map_err(|e| e.to_string())?)
                .env("SteamAppId", &appid)
                .env("SteamGameId", &appid);
            process
        } else {
            Command::new(&rewritten[0])
        };
        // Use the launcher's own directory. Steam's native launch usually
        // runs a nested executable from its own directory (e.g. Max Payne 3's
        // PlayMaxPayne3.exe); running it from the game root changes its
        // relative-path configuration and can terminate it immediately.
        let workdir = if let Some(cwd) = managed_cwd.as_deref() {
            std::path::PathBuf::from(overlay.game().rewrite_arg(cwd, &merged))
        } else {
            selected_target
                .as_ref()
                .map(|target| overlay.game().rewrite_arg(
                    &target.executable.display().to_string(),
                    &merged,
                ))
                .and_then(|path| std::path::Path::new(&path).parent().map(std::path::Path::to_path_buf))
                .unwrap_or_else(|| merged.clone())
        };
        process.args(&rewritten[1..]).current_dir(workdir);
        if let Some(appid) = overlay.game().appid() {
            process.env("SteamAppId", appid);
            process.env("SteamGameId", appid);
        }
        // A fix that ships a launcher loader DLL must win over Wine's
        // builtin/prefix copy. This is required for Uplay/Ubisoft Connect
        // fixes, which otherwise start the real Ubisoft launcher.
        let mut overrides: Vec<String> = ["uplay_r1_loader64", "uplay_r1_loader", "dbdata"]
            .into_iter()
            .filter(|name| overlay.game().fixes().join(format!("{name}.dll")).is_file())
            .map(|name| format!("{name}=n,b"))
            .collect();
        if !overrides.is_empty() {
            if let Some(existing) = std::env::var_os("WINEDLLOVERRIDES") {
                overrides.extend(
                    existing
                        .to_string_lossy()
                        .split(';')
                        .filter(|part| !part.trim().is_empty())
                        .map(str::to_owned),
                );
            }
            process.env("WINEDLLOVERRIDES", overrides.join(";"));
        }
        let status = process.status().map_err(|error| format!("could not launch game: {error}"))?;
        overlay.unmount().map_err(|error| error.to_string())?;
        Ok(status.code().unwrap_or(1))
    })();
    Some(match result {
        Ok(code) => code,
        Err(error) => { eprintln!("SteamTron overlay wrapper: {error}"); 1 }
    })
}



#[tauri::command]
fn managed_fixes_dir(game: &steamtron_core::library::Game) -> Option<std::path::PathBuf> {
    let drive = steamtron_core::library::drive_root_for(&game.installation.root)?;
    let layout = steamtron_core::library::game_layout(&drive, game).ok()?;
    Some(layout.fixes)
}

fn dir_has_entries(path: &std::path::Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

fn installed_proton_runner() -> Option<std::path::PathBuf> {
    let root = proton_sources::runner_root();
    let mut candidates: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    candidates.sort();
    for directory in candidates {
        let runner = directory.join("proton");
        if runner.is_file() {
            return Some(runner);
        }
        if directory.join("proton").is_dir() {
            return Some(directory.join("proton"));
        }
    }
    None
}

#[tauri::command]
fn play_managed(game_id: String) -> Result<serde_json::Value, String> {
    use std::fs::File;
    use std::process::Stdio;
    use steamtron_core::library::Library;
    use steamtron_core::runtime::{managed_launch_plan, primary_target};

    require_auth()?;
    let library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library
        .game(&game_id)
        .cloned()
        .ok_or_else(|| format!("Unknown game {game_id}"))?;
    steamtron_core::runtime::use_default_proton(&mut game, &library.settings().default_proton);
    let selected_proton = if !game.launch.proton.is_empty() {
        game.launch.proton.clone()
    } else {
        game.installation.proton.clone()
    };
    let proton_usable = {
        let path = std::path::Path::new(&selected_proton);
        !selected_proton.is_empty() && (path.is_file() || path.is_dir())
    };
    if !proton_usable {
        if let Some(fallback) = installed_proton_runner() {
            let fallback = fallback.display().to_string();
            game.launch.proton = fallback.clone();
            game.installation.proton = fallback;
        } else {
            return Err(
                "No Proton runner is installed. Open Settings → Proton Manager and install one before playing."
                    .into(),
            );
        }
    }
    let steam_root = steam::steam_root().ok();
    let client_root = if game.launch.requires_steam {
        let root = steam_root.clone().ok_or_else(|| {
            "This game requires Steam, but Steam is not installed. Install Steam or turn off the Steam requirement in Game Settings."
                .to_string()
        })?;
        game.launch.environment.remove("PROTON_DISABLE_LSTEAMCLIENT");
        if let Some(overrides) = game.launch.environment.get_mut("WINEDLLOVERRIDES") {
            let kept: Vec<&str> = overrides
                .split(';')
                .filter(|part| !part.split('=').next().is_some_and(|name| name.trim().eq_ignore_ascii_case("lsteamclient")))
                .filter(|part| !part.is_empty())
                .collect();
            if kept.is_empty() {
                game.launch.environment.remove("WINEDLLOVERRIDES");
            } else {
                *overrides = kept.join(";");
            }
        }
        if !steam_process_running() {
            return Err(
                "This game requires a running Steam client for Steamworks. Start Steam in the background, then try again."
                    .into(),
            );
        }
        // Steamworks games borrow Steam's compatdata prefix, but still launch
        // through Proton directly; Steam is never asked to start the game.
        if let Some(app_id) = game.metadata_app_id {
            if game.launch.prefix_mode != "steam" {
                game.launch.prefix_mode = "steam".into();
                game.installation.prefix = root
                    .join("steamapps/compatdata")
                    .join(app_id.to_string())
                    .join("pfx");
            }
        }
        root
    } else {
        steam_root.unwrap_or_else(|| steamtron_core::paths::layout().home())
    };

    let layout = steamtron_core::paths::layout();
    let logs = layout.diagnostics_for(&game.id);
    steamtron_core::paths::ensure_private_dir(&logs)
        .map_err(|error| format!("{}: {error}", logs.display()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let log = logs.join(format!("play-{stamp}.log"));
    let stdout = File::create(&log).map_err(|error| format!("{}: {error}", log.display()))?;
    let stderr = stdout
        .try_clone()
        .map_err(|error| format!("{}: {error}", log.display()))?;

    let (mut command, mode, program, arguments, overlay, fixes) = {
    let target = primary_target(&game)
        .cloned()
        .ok_or_else(|| "This game has no launch target yet.".to_string())?;
    let plan = managed_launch_plan(&game, &target, &client_root)
        .map_err(|error| error.to_string())?;
    let fixes = managed_fixes_dir(&game);
    let use_overlay =
        fixes.as_ref().is_some_and(|path| dir_has_entries(path));
    let command = if use_overlay {
        let fixes_path = fixes.as_ref().expect("checked above").clone();
        let appid = game
            .metadata_app_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| game.id.clone());
        let exe = steamtron_executable()?;
        let mut command = std::process::Command::new(&exe);
        command
            .arg("--steamtron-overlay")
            .args([
                "--appid",
                &appid,
                "--common",
                &game.installation.root.display().to_string(),
                "--fixes",
                &fixes_path.display().to_string(),
                "--proton-path",
                &plan.program.display().to_string(),
                "--compat",
                &game.installation.prefix.display().to_string(),
                "--cwd",
                &game.installation.root.display().to_string(),
                "--steam-root",
                &client_root.display().to_string(),
                "--",
                &plan.arguments[1],
            ])
            .args(&target.arguments);
        command
    } else {
        plan.command()
    };
    (
        command,
        "proton",
        plan.program.display().to_string(),
        plan.arguments,
        use_overlay,
        fixes,
    )
    };

    if managed_runs()
        .lock()
        .map_err(|_| "Game state lock failed.".to_string())?
        .contains_key(&game.id)
    {
        return Err("This game is already running.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.env("STEAMTRON_GAME_ID", &game.id);
    steamtron_core::prelaunch::run(&game, std::path::Path::new(&program), &stdout)?;
    let child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|error| format!("Could not start the game: {error}"))?;
    let pid = child.id();
    managed_runs()
        .lock()
        .map_err(|_| "Game state lock failed.".to_string())?
        .insert(game.id.clone(), pid);
    let tracked = game.id.clone();
    let observed_at = stamp as i64;
    std::thread::spawn(move || {
        let mut child = child;
        // Match the UmuTron milestone: two continuous seconds of the launched
        // process before it counts as a recent play.
        std::thread::sleep(std::time::Duration::from_secs(2));
        let alive = child.try_wait().ok().flatten().is_none();
        if alive {
            let _ = steamtron_core::play_history::record(&tracked, observed_at);
        }
        if let Ok(mut runs) = managed_runs().lock() {
            if runs.get(&tracked) == Some(&pid) {
                runs.remove(&tracked);
            }
        }
        let _ = child.wait();
    });
    Ok(serde_json::json!({
        "pid": pid,
        "log": log.display().to_string(),
        "mode": mode,
        "overlay": overlay,
        "fixes": fixes.map(|path| path.display().to_string()),
        "program": program,
        "arguments": arguments,
    }))
}

#[cfg(target_os = "linux")]
fn process_evidence() -> Vec<(u32, String, String, Vec<String>, String)> {
    use std::path::Path as StdPath;

    let self_pid = std::process::id();
    let mut result = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else { return result };
    for (index, entry) in entries.flatten().enumerate() {
        if index >= 8192 {
            break;
        }
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else { continue };
        if pid == self_pid {
            continue;
        }
        let directory = entry.path();
        let Ok(raw) = std::fs::read(directory.join("cmdline")) else { continue };
        if raw.len() > 64 * 1024 {
            continue;
        }
        let args: Vec<String> = raw
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect();
        let Some(first) = args.first() else { continue };
        let argv0 = first
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .unwrap_or(first)
            .trim_end_matches('.')
            .to_ascii_lowercase();
        let comm = std::fs::read_to_string(directory.join("comm"))
            .unwrap_or_default()
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();
        let mut prefixes = Vec::new();
        let mut steamtron_game = String::new();
        if let Ok(environment) = std::fs::read(directory.join("environ")) {
            for value in environment.split(|byte| *byte == 0) {
                let text = String::from_utf8_lossy(value);
                if let Some((name, value)) = text.split_once('=') {
                    if name == "WINEPREFIX" || name == "STEAM_COMPAT_DATA_PATH" {
                        let path = StdPath::new(value);
                        prefixes.push(path.display().to_string());
                    } else if name == "STEAMTRON_GAME_ID" {
                        steamtron_game = value.to_owned();
                    }
                }
            }
        }
        result.push((pid, argv0, comm, prefixes, steamtron_game));
    }
    result
}

#[cfg(target_os = "linux")]
fn game_process_pids(game: &steamtron_core::library::Game) -> Vec<u32> {
    let Some(target) = game
        .launch_targets
        .iter()
        .find(|target| target.primary)
        .or_else(|| game.launch_targets.first())
    else {
        return Vec::new();
    };
    let _ = target;
    let mut result = Vec::new();
    for (pid, _argv0, _comm, _prefixes, steamtron_game) in process_evidence() {
        // Only processes launched by SteamTron carry this marker. Games
        // started by another launcher (including UmuTron) must not flip our
        // Play button into Stop.
        if steamtron_game == game.id {
            result.push(pid);
        }
    }
    result
}

#[cfg(not(target_os = "linux"))]
fn game_process_pids(_game: &steamtron_core::library::Game) -> Vec<u32> {
    Vec::new()
}

#[tauri::command]
fn running_managed() -> Result<Vec<String>, String> {
    use steamtron_core::library::Library;

    let mut games: std::collections::BTreeSet<String> = managed_runs()
        .lock()
        .map_err(|_| "Game state lock failed.".to_string())?
        .keys()
        .cloned()
        .collect();
    #[cfg(target_os = "linux")]
    if let Ok(library) = Library::load() {
        for game in library.games() {
            if !game_process_pids(game).is_empty() {
                games.insert(game.id.clone());
            }
        }
    }
    Ok(games.into_iter().collect())
}

#[cfg(target_os = "linux")]
fn process_descendants(root: u32) -> Vec<u32> {
    let mut table = std::collections::HashMap::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else { continue };
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else { continue };
            let Some(close) = stat.rfind(')') else { continue };
            let fields: Vec<&str> = stat[close + 2..].split_whitespace().collect();
            if let Some(parent) = fields.get(1).and_then(|value| value.parse::<u32>().ok()) {
                table.insert(pid, parent);
            }
        }
    }
    let mut owned = vec![root];
    let mut index = 0;
    while index < owned.len() && owned.len() < 4096 {
        let parent = owned[index];
        index += 1;
        for (pid, ppid) in &table {
            if *ppid == parent && !owned.contains(pid) {
                owned.push(*pid);
            }
        }
    }
    owned
}

#[cfg(target_os = "linux")]
fn signal_pid(pid: u32, signal: i32) {
    unsafe {
        libc::kill(pid as i32, signal);
    }
}

#[cfg(target_os = "linux")]
fn pid_exists(pid: u32) -> bool {
    std::path::Path::new("/proc").join(pid.to_string()).exists()
}

#[tauri::command]
async fn stop_managed(game_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || stop_managed_blocking(game_id))
        .await
        .map_err(|error| format!("stop task failed: {error}"))?
}

fn stop_managed_blocking(game_id: String) -> Result<(), String> {
    use steamtron_core::library::Library;

    let tracked = managed_runs()
        .lock()
        .map_err(|_| "Game state lock failed.".to_string())?
        .get(&game_id)
        .copied();
    let game = Library::load()
        .ok()
        .and_then(|library| library.game(&game_id).cloned());
    #[cfg(target_os = "linux")]
    let mut pids: Vec<u32> = tracked.into_iter().collect();
    #[cfg(target_os = "linux")]
    if let Some(game) = game.as_ref() {
        pids.extend(game_process_pids(game));
    }
    #[cfg(target_os = "linux")]
    pids.sort_unstable();
    #[cfg(target_os = "linux")]
    pids.dedup();
    #[cfg(target_os = "linux")]
    if pids.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        for pid in &pids {
            for process in process_descendants(*pid) {
                signal_pid(process, libc::SIGTERM);
            }
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            let alive = pids.iter().any(|pid| pid_exists(*pid))
                || game.as_ref().is_some_and(|game| !game_process_pids(game).is_empty());
            if !alive {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        for pid in &pids {
            for process in process_descendants(*pid) {
                signal_pid(process, libc::SIGKILL);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        managed_runs()
            .lock()
            .map_err(|_| "Game state lock failed.".to_string())?
            .remove(&game_id);
        if game.as_ref().is_some_and(|game| !game_process_pids(game).is_empty()) {
            return Err("The game processes did not stop.".into());
        }
        return Ok(());
    }
    #[cfg(not(target_os = "linux"))]
    {
        let pid = tracked.ok_or_else(|| "This game is not running.".to_string())?;
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|error| format!("Could not stop the game: {error}"))?;
        if !status.success() {
            return Err(format!("taskkill exited with {status}"));
        }
        managed_runs()
            .lock()
            .map_err(|_| "Game state lock failed.".to_string())?
            .remove(&game_id);
        Ok(())
    }
}

#[tauri::command]
fn pick_managed_executable(
    game_id: String,
    start: Option<String>,
) -> Result<Option<String>, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("Unknown game {game_id}"))?;
    let directory = start
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| game.installation.root.clone());
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose game launcher")
        .set_directory(directory)
        .pick_file()
    else {
        return Ok(None);
    };
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
fn pick_managed_folder(game_id: String) -> Result<Option<String>, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("Unknown game {game_id}"))?;
    let directory = if game.installation.root.is_dir() {
        game.installation.root.clone()
    } else {
        std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default()
    };
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose installed game folder")
        .set_directory(directory)
        .pick_folder()
    else {
        return Ok(None);
    };
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
fn link_managed_game(
    request: steamtron_library::managed::LinkGameRequest,
) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let report = steamtron_library::managed::link_game(request)
        .map_err(|error| error.to_string())?;
    let library = Library::load().map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "report": report,
        "library": library_json(&library),
    }))
}

#[tauri::command]
fn set_managed_launch_target(
    request: steamtron_library::managed::SetTargetRequest,
) -> Result<steamtron_library::managed::TargetReport, String> {
    require_auth()?;
    steamtron_library::managed::set_launch_target(request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn managed_fixes_status(game_id: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    let view = steamtron_library::managed::fixes_view(&game_id)
        .map_err(|error| error.to_string())?;
    let tools = fix_tools::status(game);
    let bytes: u64 = view.files.iter().map(|file| file.size).sum();
    Ok(serde_json::json!({
        "path": view.path,
        "files": view.files.len(),
        "bytes": bytes,
        "enabled": game.fixes_enabled,
        "goldberg": tools["goldberg"],
        "steamless": tools["steamless"],
    }))
}


#[tauri::command]
fn export_managed_fixes(game_id: String) -> Result<Option<String>, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    let fixes = managed_fixes_dir(game)
        .ok_or_else(|| "this game is not inside a managed library".to_string())?;
    if !fixes.is_dir() {
        return Err("No fixes are installed for this game.".into());
    }
    let appid = game
        .metadata_app_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| game.id.clone());
    let Some(path) = rfd::FileDialog::new()
        .set_title("Export fixes ZIP")
        .set_file_name(steamtron_core::fixes::export_name(&game.title))
        .add_filter("Fixes ZIP", &["zip"])
        .save_file()
    else {
        return Ok(None);
    };
    steamtron_core::fixes::export_zip(&fixes, &path, &appid)
        .map_err(|error| error.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
async fn apply_managed_goldberg(
    app: tauri::AppHandle,
    game_id: String,
) -> Result<serde_json::Value, String> {
    require_auth()?;
    tauri::async_runtime::spawn_blocking(move || {
        let progress = |message: &str| {
            let _ = app.emit("fix-progress", message.to_string());
        };
        fix_tools::apply_goldberg(&game_id, &progress)
    })
    .await
    .map_err(|error| format!("GoldBerg task failed: {error}"))?
}

#[tauri::command]
fn remove_managed_goldberg(game_id: String) -> Result<serde_json::Value, String> {
    require_auth()?;
    fix_tools::remove_goldberg(&game_id)
}

#[tauri::command]
async fn apply_managed_steamless(
    app: tauri::AppHandle,
    game_id: String,
) -> Result<serde_json::Value, String> {
    require_auth()?;
    tauri::async_runtime::spawn_blocking(move || {
        let progress = |message: &str| {
            let _ = app.emit("fix-progress", message.to_string());
        };
        fix_tools::apply_steamless(&game_id, &progress)
    })
    .await
    .map_err(|error| format!("Steamless task failed: {error}"))?
}

#[tauri::command]
fn remove_managed_steamless(game_id: String) -> Result<serde_json::Value, String> {
    require_auth()?;
    fix_tools::remove_steamless(&game_id)
}

#[tauri::command]
fn managed_fixes(game_id: String) -> Result<steamtron_library::managed::FixesView, String> {
    steamtron_library::managed::fixes_view(&game_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn install_managed_fixes(
    game_id: String,
) -> Result<Option<steamtron_library::managed::FixesView>, String> {
    require_auth()?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose a fixes ZIP")
        .add_filter("Fixes ZIP", &["zip"])
        .pick_file()
    else {
        return Ok(None);
    };
    steamtron_library::managed::install_fixes(&game_id, &path)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_managed_fixes(game_id: String) -> Result<steamtron_library::managed::FixesRemoved, String> {
    require_auth()?;
    steamtron_library::managed::remove_fixes(&game_id).map_err(|error| error.to_string())
}


fn library_json(library: &steamtron_core::library::Library) -> serde_json::Value {
    let games: Vec<serde_json::Value> = library
        .games()
        .iter()
        .map(|game| {
            let mut value = serde_json::to_value(game).unwrap_or(serde_json::Value::Null);
            let root = &game.installation.root;
            let installed = root.is_dir()
                && std::fs::read_dir(root)
                    .map(|mut entries| entries.next().is_some())
                    .unwrap_or(false);
            value["installed"] = serde_json::json!(installed);
            value["prefix_exists"] = serde_json::json!(game.installation.prefix.is_dir());
            value
        })
        .collect();
    serde_json::json!({
        "path": library.path().display().to_string(),
        "settings": library.settings(),
        "drives": library.drives(),
        "games": games,
        "recently_played": steamtron_core::play_history::recent_ids(),
    })
}

fn diagnostics_totals(root: &std::path::Path) -> (u64, u64) {
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(meta) = entry.metadata() {
                files += 1;
                bytes += meta.len();
            }
        }
    }
    (files, bytes)
}

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[tauri::command]
fn update_library_settings(
    default_display_mode: Option<String>,
    default_prefix_mode: Option<String>,
    gamemode: Option<bool>,
) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    let mut library = Library::load().map_err(|error| error.to_string())?;
    if let Some(mode) = default_display_mode {
        if mode != "desktop" && mode != "fullscreen" {
            return Err("display mode must be desktop or fullscreen".into());
        }
        library.settings_mut().default_display_mode = mode;
    }
    if let Some(mode) = default_prefix_mode {
        if mode != "umu" && mode != "steam" {
            return Err("prefix source must be umu or steam".into());
        }
        library.settings_mut().default_prefix_mode = mode;
    }
    if let Some(gamemode) = gamemode {
        library.settings_mut().gamemode = gamemode;
    }
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!(library.settings()))
}

fn unescape_mount(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let octal = &value[index + 1..index + 4];
            if let Ok(character) = u8::from_str_radix(octal, 8) {
                output.push(character as char);
                index += 4;
                continue;
            }
        }
        output.push(bytes[index] as char);
        index += 1;
    }
    output
}

#[cfg(target_os = "linux")]
fn disk_space(path: &std::path::Path) -> (u64, u64) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return (0, 0);
    };
    let mut statistics: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut statistics) } != 0 {
        return (0, 0);
    }
    let block = statistics.f_frsize as u64;
    (
        statistics.f_blocks as u64 * block,
        statistics.f_bavail as u64 * block,
    )
}

#[cfg(target_os = "linux")]
fn path_writable(path: &std::path::Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

#[tauri::command]
fn available_library_drives() -> Result<Vec<serde_json::Value>, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    let mut result = Vec::new();
    #[cfg(target_os = "linux")]
    {
        let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
        let pseudo = [
            "proc", "sysfs", "devtmpfs", "devpts", "tmpfs", "cgroup", "cgroup2", "pstore",
            "bpf", "tracefs", "debugfs", "securityfs", "configfs", "fusectl", "mqueue",
            "hugetlbfs", "autofs", "ramfs", "squashfs", "nsfs", "overlay", "binfmt_misc",
            "efivarfs", "rpc_pipefs", "fuse.gvfsd-fuse", "fuse.portal",
        ];
        let mut seen = std::collections::BTreeSet::new();
        for line in mounts.lines() {
            let mut fields = line.split_whitespace();
            let device = fields.next().unwrap_or("");
            let mountpoint = unescape_mount(fields.next().unwrap_or(""));
            let fstype = fields.next().unwrap_or("");
            if mountpoint.is_empty() || !mountpoint.starts_with('/') {
                continue;
            }
            if pseudo.contains(&fstype) || mountpoint.starts_with("/sys") || mountpoint.starts_with("/proc") {
                continue;
            }
            let candidate_mount = mountpoint == "/"
                || mountpoint.starts_with("/mnt/")
                || mountpoint.starts_with("/media/")
                || mountpoint.starts_with("/run/media/");
            if !candidate_mount {
                continue;
            }
            if device != "none" && !device.starts_with('/') {
                continue;
            }
            let path = std::path::Path::new(&mountpoint);
            if !path.is_dir() {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if !seen.insert(canonical.clone()) {
                continue;
            }
            let (total, free) = disk_space(&canonical);
            if total == 0 {
                continue;
            }
            let registered = library.drives().iter().any(|drive| drive.root == canonical);
            let label = if mountpoint == "/" {
                "System Root (/)".to_owned()
            } else {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&mountpoint)
                    .to_owned()
            };
            result.push(serde_json::json!({
                "path": canonical.display().to_string(),
                "label": label,
                "device": device,
                "fstype": fstype,
                "total": total,
                "free": free,
                "writable": path_writable(&canonical),
                "registered": registered,
                "default": library.drives().iter().any(|drive| drive.root == canonical && drive.default),
            }));
        }
    }
    result.sort_by(|left, right| {
        let left_root = left["path"] == "/";
        let right_root = right["path"] == "/";
        left_root.cmp(&right_root).then_with(|| {
            left["label"].as_str().unwrap_or("").cmp(right["label"].as_str().unwrap_or(""))
        })
    });
    Ok(result)
}

#[tauri::command]
fn add_library_drive(path: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let root = std::path::Path::new(&path);
    if !root.is_absolute() || !root.is_dir() {
        return Err("Choose a mounted drive or folder.".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    if !path_writable(&root) {
        return Err("This drive is not writable.".into());
    }
    let label = if root == std::path::Path::new("/") {
        "System Root".to_owned()
    } else {
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Drive")
            .to_owned()
    };
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let default = library.drives().is_empty();
    library
        .add_drive(&root, label, default)
        .map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(library_json(&library))
}

#[tauri::command]
fn remove_library_drive(id: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let mut library = Library::load().map_err(|error| error.to_string())?;
    library.remove_drive(&id).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(library_json(&library))
}

#[tauri::command]
fn set_default_library_drive(id: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let mut library = Library::load().map_err(|error| error.to_string())?;
    library
        .set_default_drive(&id)
        .map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(library_json(&library))
}

#[tauri::command]
fn diagnostics_summary() -> Result<serde_json::Value, String> {
    let root = steamtron_core::paths::layout().diagnostics();
    let games = std::fs::read_dir(&root)
        .map(|entries| entries.flatten().filter(|entry| entry.path().is_dir()).count())
        .unwrap_or(0);
    let (files, bytes) = diagnostics_totals(&root);
    Ok(serde_json::json!({
        "path": root.display().to_string(),
        "games": games,
        "files": files,
        "bytes": bytes,
    }))
}

#[tauri::command]
fn clear_diagnostics() -> Result<serde_json::Value, String> {
    let root = steamtron_core::paths::layout().diagnostics();
    let (files, bytes) = diagnostics_totals(&root);
    if root.exists() {
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("{}: {error}", root.display()))?;
    }
    std::fs::create_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
    Ok(serde_json::json!({ "removed_files": files, "removed_bytes": bytes }))
}

fn runner_entry(path: &std::path::Path, source: &str) -> serde_json::Value {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| *name != "proton")
        .map(str::to_owned)
        .or_else(|| {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| path.display().to_string());
    serde_json::json!({
        "path": path.display().to_string(),
        "name": name,
        "source": source,
        "exists": path.exists(),
    })
}

fn detect_proton_runners() -> Vec<serde_json::Value> {
    let mut seen = std::collections::BTreeSet::new();
    let mut runners = Vec::new();
    let mut push = |path: std::path::PathBuf, source: &str| {
        if seen.insert(path.clone()) {
            runners.push(runner_entry(&path, source));
        }
    };
    if let Ok(libraries) = steam::libraries() {
        for library in libraries {
            let common = library.join("common");
            if let Ok(entries) = std::fs::read_dir(&common) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                    if !name.contains("proton") {
                        continue;
                    }
                    let runner = path.join("proton");
                    if runner.is_file() {
                        push(runner, "steam");
                    }
                }
            }
        }
    }
    runners
}

fn proton_runners_json() -> serde_json::Value {
    use steamtron_core::library::Library;
    let settings = Library::load()
        .map(|library| library.settings().clone())
        .unwrap_or_default();
    let mut detected = detect_proton_runners();
    let runners_root = steamtron_core::paths::layout().runners();
    for runner in &settings.proton_runners {
        if !detected.iter().any(|entry| entry["path"] == runner.as_str()) {
            let source = if std::path::Path::new(runner).starts_with(&runners_root) {
                "downloaded"
            } else {
                "custom"
            };
            detected.push(runner_entry(std::path::Path::new(runner), source));
        }
    }
    serde_json::json!({
        "runners": detected,
        "default": settings.default_proton,
    })
}

#[tauri::command]
fn proton_runners() -> serde_json::Value {
    proton_runners_json()
}
#[tauri::command]
async fn proton_source_releases(
    family: String,
    force: Option<bool>,
) -> Result<Vec<proton_sources::SourceRelease>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        proton_sources::list_cached(&family, force.unwrap_or(false))
    })
    .await
    .map_err(|error| format!("Proton source task failed: {error}"))?
}

fn spawn_proton_install(family: &str, tag: &str, name: &str, resume: bool) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not locate SteamTron: {error}"))?;
    let mut command = std::process::Command::new(executable);
    command
        .args(["--proton-install", family, tag, name])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if resume {
        command.arg("--resume");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .spawn()
        .map_err(|error| format!("Could not start the Proton installer: {error}"))?;
    Ok(())
}

#[tauri::command]
fn install_proton_release(
    family: String,
    tag: String,
    name: String,
) -> Result<serde_json::Value, String> {
    require_auth()?;
    if proton_sources::read_states().iter().any(|state| {
        state.family == family
            && state.tag == tag
            && state.name == name
            && matches!(state.status.as_str(), "starting" | "downloading" | "extracting" | "registering")
    }) {
        return Ok(serde_json::json!({ "started": false }));
    }
    spawn_proton_install(&family, &tag, &name, false)?;
    Ok(serde_json::json!({ "started": true }))
}

#[tauri::command]
fn resume_proton_install(
    family: String,
    tag: String,
    name: String,
) -> Result<serde_json::Value, String> {
    require_auth()?;
    spawn_proton_install(&family, &tag, &name, true)?;
    Ok(serde_json::json!({ "started": true }))
}

#[tauri::command]
fn pause_proton_install(family: String, tag: String, name: String) -> Result<(), String> {
    require_auth()?;
    proton_sources::pause(&family, &tag, &name)
}

#[tauri::command]
fn cancel_proton_install(family: String, tag: String, name: String) -> Result<(), String> {
    require_auth()?;
    proton_sources::cancel(&family, &tag, &name)
}


#[tauri::command]
fn proton_install_states() -> Vec<proton_sources::InstallState> {
    proton_sources::read_states()
}

#[tauri::command]
fn uninstall_proton_release(path: String) -> Result<serde_json::Value, String> {
    require_auth()?;
    proton_sources::uninstall(&path)?;
    Ok(serde_json::json!({ "uninstalled": true }))
}

#[tauri::command]
fn set_default_proton_runner(path: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let runner = std::path::Path::new(&path);
    if !runner.exists() {
        return Err(format!("runner does not exist: {path}"));
    }
    let mut library = Library::load().map_err(|error| error.to_string())?;
    library.settings_mut().default_proton = path;
    library.save().map_err(|error| error.to_string())?;
    Ok(proton_runners_json())
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let target = std::path::Path::new(&path);
    if !target.is_dir() {
        return Err(format!("folder does not exist: {path}"));
    }
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer");
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    return Err("opening folders is not supported on this platform".into());
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        command
            .arg(target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("could not open {}: {error}", target.display()))?;
        Ok(())
    }
}


#[tauri::command]
async fn app_details(appid: u32) -> Result<serde_json::Value, String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<u32, serde_json::Value>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(value) = cache.lock().ok().and_then(|cache| cache.get(&appid).cloned()) {
        return Ok(value);
    }
    let url = format!("https://store.steampowered.com/api/appdetails?appids={appid}&l=english");
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Steam store returned HTTP {}", response.status()));
    }
    let payload: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
    let data = payload
        .get(appid.to_string())
        .and_then(|entry| entry.get("data"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let release_date = data
        .get("release_date")
        .and_then(|value| value.get("date"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let genres = data
        .get("genres")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|genre| genre.get("description").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let description = data
        .get("short_description")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let name = data
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let join_names = |key: &str| {
        data.get(key)
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    let value = serde_json::json!({
        "name": name,
        "release_date": release_date,
        "genres": genres,
        "description": description,
        "developers": join_names("developers"),
        "publishers": join_names("publishers"),
    });
    if let Ok(mut cache) = cache.lock() {
        cache.insert(appid, value.clone());
    }
    Ok(value)
}


#[tauri::command]
async fn managed_artwork(game_id: String) -> Result<serde_json::Value, String> {
    let paths = artwork::ensure(&game_id).await?;
    Ok(serde_json::json!({
        "cover": paths.cover.map(|path| path.display().to_string()),
        "hero": paths.hero.map(|path| path.display().to_string()),
    }))
}



fn store_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .build()
            .expect("store HTTP client")
    })
}

fn catalog_cache_path(key: &str) -> std::path::PathBuf {
    steamtron_core::paths::layout().cache_dir().join("catalog").join(key)
}

fn catalog_cache_key(params: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    params.hash(&mut hasher);
    format!("{:016x}.json", hasher.finish())
}

fn read_catalog_cache(path: std::path::PathBuf) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_catalog_cache(path: std::path::PathBuf, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(value) {
        let _ = std::fs::write(path, text);
    }
}

async fn store_get(path: &str, query: &[(&str, String)]) -> Result<serde_json::Value, String> {
    let token = steamtron_auth::access_token();
    if token.is_empty() {
        return Err("Sign in to browse the Store.".into());
    }
    let base = steamtron_auth::api_base();
    let request = |client: &reqwest::Client| {
        client
            .get(format!("{base}{path}"))
            .query(query)
            .bearer_auth(&token)
            .header("Accept", "application/json")
            .header("User-Agent", "steamtron-desktop")
            .send()
    };
    let response = match request(store_client()).await {
        Ok(response) => response,
        Err(_) => {
            // A pooled connection can go stale while the window sits idle. Retry
            // once with a fresh client so Store search does not fail permanently
            // until the app restarts.
            std::thread::sleep(std::time::Duration::from_millis(200));
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .connect_timeout(std::time::Duration::from_secs(8))
                .build()
                .map_err(|error| format!("Could not create the Store client: {error}"))?;
            request(&client)
                .await
                .map_err(|error| format!("Could not reach the Store. Check your connection and retry. ({error})"))?
        }
    };
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("The Store rejected this session. Sign in again.".into());
    }
    if !status.is_success() {
        return Err(format!("Store returned HTTP {status}"));
    }
    response.json().await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn store_genres() -> Result<serde_json::Value, String> {
    let value = store_get("/v1/genres", &[]).await?;
    write_catalog_cache(catalog_cache_path("genres.json"), &value);
    Ok(value)
}

#[tauri::command]
fn store_cached_genres() -> Option<serde_json::Value> {
    read_catalog_cache(catalog_cache_path("genres.json"))
}

#[tauri::command]
async fn store_catalog(
    query: String,
    genre: Option<String>,
    sort: Option<String>,
    page: u32,
) -> Result<serde_json::Value, String> {
    let page = page.max(1);
    let genre = genre.unwrap_or_default();
    let sort = sort.unwrap_or_default();
    let mut params = vec![
        ("q", query.clone()),
        ("genre", genre.clone()),
        ("page", page.to_string()),
    ];
    if !sort.is_empty() {
        params.push(("sort", sort.clone()));
    }
    // Must match store_cached_catalog exactly: query|genre|sort|page.
    let key = catalog_cache_key(&format!("{query}|{genre}|{sort}|{page}"));
    let value = store_get("/v1/games", &params).await?;
    write_catalog_cache(catalog_cache_path(&key), &value);
    Ok(value)
}

#[tauri::command]
fn store_cached_catalog(
    query: String,
    genre: Option<String>,
    sort: Option<String>,
    page: u32,
) -> Option<serde_json::Value> {
    let key = catalog_cache_key(&format!(
        "{query}|{}|{}|{}",
        genre.unwrap_or_default(),
        sort.unwrap_or_default(),
        page.max(1)
    ));
    read_catalog_cache(catalog_cache_path(&key))
}

#[tauri::command]
async fn managed_dlc(game_id: String) -> Result<Vec<serde_json::Value>, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    let Some(app_id) = game.metadata_app_id else {
        return Ok(Vec::new());
    };
    let lua = steamtron_core::library::managed_lua_path(game)
        .filter(|path| path.is_file())
        .or_else(|| {
            steam::steam_root()
                .ok()
                .map(|root| root.join("config/stplug-in").join(format!("{app_id}.lua")))
                .filter(|path| path.is_file())
        })
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let response = store_client()
        .get("https://store.steampowered.com/api/appdetails")
        .query(&[("appids", app_id.to_string()), ("l", "english".into()), ("cc", "US".into())])
        .header("Accept", "application/json")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let payload: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
    let data = payload
        .get(app_id.to_string())
        .and_then(|entry| entry.get("data"));
    let ids: Vec<u32> = data
        .and_then(|data| data.get("dlc"))
        .and_then(|dlc| dlc.as_array())
        .map(|items| items.iter().filter_map(|id| id.as_u64().map(|id| id as u32)).collect())
        .unwrap_or_default();
    let mut result = Vec::new();
    for id in ids {
        let detail = match store_client()
            .get("https://store.steampowered.com/api/appdetails")
            .query(&[("appids", id.to_string()), ("l", "english".into()), ("cc", "US".into())])
            .header("Accept", "application/json")
            .header("User-Agent", "steamtron-desktop")
            .send()
            .await
        {
            Ok(response) => response.json::<serde_json::Value>().await.ok(),
            Err(_) => None,
        };
        let entry = detail.as_ref().and_then(|value| value.get(id.to_string())).and_then(|entry| entry.get("data"));
        let name = entry
            .and_then(|data| data.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned();
        let released = entry
            .and_then(|data| data.get("release_date"))
            .and_then(|value| value.get("date"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned();
        let marked = lua.contains(&format!("AppID: {id}"))
            || lua.lines().any(|line| line.trim_start().starts_with(&format!("addappid({id}")));
        result.push(serde_json::json!({
            "app_id": id,
            "name": if name.is_empty() { format!("DLC {id}") } else { name },
            "released": released,
            "available": marked,
            "installed": false,
        }));
    }
    Ok(result)
}

#[tauri::command]
async fn store_game(id: u64) -> Result<serde_json::Value, String> {
    store_get(&format!("/v1/games/{id}"), &[]).await
}


fn main_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    use tauri::Manager;
    app.get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())
}

#[tauri::command]
fn fullscreen_state(app: tauri::AppHandle) -> Result<bool, String> {
    main_window(&app)?
        .is_fullscreen()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn toggle_fullscreen(app: tauri::AppHandle) -> Result<bool, String> {
    let window = main_window(&app)?;
    let current = window.is_fullscreen().map_err(|error| error.to_string())?;
    window
        .set_fullscreen(!current)
        .map_err(|error| error.to_string())?;
    Ok(!current)
}


#[tauri::command]
fn set_ui_scale(app: tauri::AppHandle, scale: f64) -> Result<(), String> {
    let scale = scale.clamp(0.5, 2.0);
    main_window(&app)?
        .set_zoom(scale)
        .map_err(|error| error.to_string())
}

fn spawn_system(command: &str, args: &[&str]) -> Result<(), String> {
    std::process::Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not run {command}: {error}"))
}

#[tauri::command]
fn reboot_system() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    return spawn_system("systemctl", &["reboot"]);
    #[cfg(target_os = "windows")]
    return spawn_system("shutdown", &["/r", "/t", "0"]);
    #[allow(unreachable_code)]
    Err("reboot is not supported on this platform".into())
}

#[tauri::command]
fn shutdown_system() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    return spawn_system("systemctl", &["poweroff"]);
    #[cfg(target_os = "windows")]
    return spawn_system("shutdown", &["/s", "/t", "0"]);
    #[allow(unreachable_code)]
    Err("shutdown is not supported on this platform".into())
}


fn directory_size(root: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else { continue };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

fn copy_tree(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())?.flatten() {
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            std::fs::copy(&path, &target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn emit_operation(app: &tauri::AppHandle, operation: &str, game_id: &str, stage: &str, status: &str, message: &str) {
    let _ = app.emit("operation-progress", serde_json::json!({
        "operation": operation,
        "gameId": game_id,
        "stage": stage,
        "status": status,
        "message": message,
    }));
}

fn emit_operation_at(app: &tauri::AppHandle, operation: &str, game_id: &str, stage: &str, status: &str, message: &str, percent: f64) {
    let _ = app.emit("operation-progress", serde_json::json!({
        "operation": operation,
        "gameId": game_id,
        "stage": stage,
        "status": status,
        "message": message,
        "percent": percent.clamp(0.0, 100.0),
    }));
}

#[derive(Clone)]
struct OperationControl {
    cancelled: Arc<AtomicBool>,
    pid: Arc<Mutex<Option<u32>>>,
}

fn operation_controls() -> &'static Mutex<HashMap<String, OperationControl>> {
    static CONTROLS: OnceLock<Mutex<HashMap<String, OperationControl>>> = OnceLock::new();
    CONTROLS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn operation_key(operation: &str, game_id: &str) -> String {
    format!("{operation}:{game_id}")
}

fn register_operation(operation: &str, game_id: &str) -> OperationControl {
    let control = OperationControl {
        cancelled: Arc::new(AtomicBool::new(false)),
        pid: Arc::new(Mutex::new(None)),
    };
    if let Ok(mut controls) = operation_controls().lock() {
        controls.insert(operation_key(operation, game_id), control.clone());
    }
    control
}

fn unregister_operation(operation: &str, game_id: &str) {
    if let Ok(mut controls) = operation_controls().lock() {
        controls.remove(&operation_key(operation, game_id));
    }
}

fn operation_check(control: &OperationControl) -> Result<(), String> {
    if control.cancelled.load(Ordering::SeqCst) {
        Err("Operation cancelled".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
fn cancel_operation(app: tauri::AppHandle, operation: String, game_id: String) -> Result<(), String> {
    let control = operation_controls()
        .lock()
        .map_err(|_| "operation state lock failed".to_string())?
        .get(&operation_key(&operation, &game_id))
        .cloned()
        .ok_or_else(|| "this operation is not running".to_string())?;
    control.cancelled.store(true, Ordering::SeqCst);
    if let Some(pid) = *control.pid.lock().map_err(|_| "operation PID lock failed".to_string())? {
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("kill").args(["-CONT", &format!("-{pid}")]).status();
            let _ = std::process::Command::new("kill").args(["-TERM", &format!("-{pid}")]).status();
        }
    }
    emit_operation(&app, &operation, &game_id, "cancelled", "cancelled", "Cancelled");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_operation_stats(
    app: &tauri::AppHandle,
    operation: &str,
    game_id: &str,
    stage: &str,
    message: &str,
    percent: f64,
    completed: u64,
    total: u64,
    started: std::time::Instant,
) {
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let speed = completed as f64 / elapsed;
    let _ = app.emit("operation-progress", serde_json::json!({
        "operation": operation,
        "gameId": game_id,
        "stage": stage,
        "status": "running",
        "message": message,
        "percent": percent.clamp(0.0, 100.0),
        "completed": completed,
        "total": total,
        "speed": speed,
        "elapsed": elapsed,
    }));
}

fn collect_backup_entries(
    root: &Path,
    directory: &Path,
    directories: &mut Vec<PathBuf>,
    files: &mut Vec<(PathBuf, u64)>,
    total: &mut u64,
) -> Result<(), String> {
    let entries = std::fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|_| "backup path escaped the game folder".to_string())?.to_path_buf();
        if metadata.is_dir() {
            directories.push(relative);
            collect_backup_entries(root, &path, directories, files, total)?;
        } else if metadata.is_file() {
            *total = total.saturating_add(metadata.len());
            files.push((relative, metadata.len()));
        }
    }
    Ok(())
}

struct HashingWriter<W: std::io::Write> {
    inner: W,
    hasher: sha2::Sha256,
}

impl<W: std::io::Write> std::io::Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let count = self.inner.write(buf)?;
        use sha2::Digest;
        self.hasher.update(&buf[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

struct ProgressReader<'a> {
    inner: std::fs::File,
    on_read: &'a mut dyn FnMut(u64),
    cancelled: &'a std::sync::atomic::AtomicBool,
}

impl std::io::Read for ProgressReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Operation cancelled"));
        }
        let count = std::io::Read::read(&mut self.inner, buffer)?;
        if count > 0 {
            (self.on_read)(count as u64);
        }
        Ok(count)
    }
}

#[cfg(unix)]
fn available_space(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some(stat.f_bavail.saturating_mul(stat.f_frsize))
}

#[cfg(not(unix))]
fn available_space(_path: &Path) -> Option<u64> {
    None
}

fn backup_size(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = value as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn check_backup_space(directory: &Path, required: u64) -> Result<(), String> {
    let Some(available) = available_space(directory) else { return Ok(()) };
    if required <= available {
        return Ok(());
    }
    Err(format!(
        "Not enough free space in {}: this backup needs about {}, but only {} is available. The existing backup was not changed. Free space or choose another destination.",
        directory.display(),
        backup_size(required),
        backup_size(available)
    ))
}

fn replace_backup_directory(package: &Path, final_path: &Path) -> Result<(), String> {
    let name = final_path.file_name().and_then(|value| value.to_str()).unwrap_or("backup");
    let previous = final_path.with_file_name(format!(".{name}.previous-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&previous);
    let had_existing = final_path.exists();
    if had_existing {
        std::fs::rename(final_path, &previous).map_err(|error| error.to_string())?;
    }
    match std::fs::rename(package, final_path) {
        Ok(()) => {
            if had_existing {
                let _ = std::fs::remove_dir_all(&previous);
            }
            Ok(())
        }
        Err(error) => {
            if had_existing {
                let _ = std::fs::rename(&previous, final_path);
            }
            Err(error.to_string())
        }
    }
}

fn backup_safe_folder(title: &str) -> String {
    let mut value: String = title
        .chars()
        .map(|character| if matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0') { '_' } else { character })
        .collect();
    value = value.trim().trim_matches('.').to_owned();
    if value.is_empty() { "Game".into() } else { value.chars().take(120).collect() }
}

#[allow(clippy::too_many_arguments)]
fn archive_component(
    source: &Path,
    component_path: &Path,
    top_prefix: &str,
    directories: &[PathBuf],
    files: &[(PathBuf, u64)],
    total_bytes: u64,
    written: &mut u64,
    overall_total: u64,
    emit: &mut dyn FnMut(f64, u64, u64, &str),
    control: &OperationControl,
) -> Result<(String, u64), String> {
    use sha2::Digest;
    let file = std::fs::File::create(component_path).map_err(|error| error.to_string())?;
    let writer = HashingWriter { inner: file, hasher: sha2::Sha256::new() };
    let mut archive = tar::Builder::new(writer);
    let root_header = {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        header
    };
    archive.append_data(&mut root_header.clone(), top_prefix, std::io::empty()).map_err(|error| error.to_string())?;
    for relative in directories {
        operation_check(control)?;
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        let path = format!("{top_prefix}/{}", relative.to_string_lossy().replace('\\', "/"));
        archive.append_data(&mut header, path, std::io::empty()).map_err(|error| error.to_string())?;
    }
    let mut last_emit = std::time::Instant::now() - std::time::Duration::from_millis(200);
    for (relative, size) in files {
        operation_check(control)?;
        let path = source.join(relative);
        let input = std::fs::File::open(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let archive_path = format!("{top_prefix}/{}", relative.to_string_lossy().replace('\\', "/"));
        let activity = format!("Archiving {}", relative.to_string_lossy().replace('\\', "/"));
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(*size);
        header.set_mode(0o644);
        header.set_cksum();
        let mut progress = |count: u64| {
            *written = written.saturating_add(count);
            if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                let percent = if overall_total > 0 { *written as f64 / overall_total as f64 * 100.0 } else { 100.0 };
                emit(percent, *written, overall_total, &activity);
                last_emit = std::time::Instant::now();
            }
        };
        let mut reader = ProgressReader { inner: input, on_read: &mut progress, cancelled: &control.cancelled };
        archive.append_data(&mut header, archive_path, &mut reader)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    if overall_total > 0 {
        let percent = *written as f64 / overall_total as f64 * 100.0;
        emit(percent, *written, overall_total, "Archive complete");
    }
    let writer = archive.into_inner().map_err(|error| error.to_string())?;
    writer.inner.sync_data().map_err(|error| error.to_string())?;
    let digest = writer.hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    Ok((digest, total_bytes))
}

fn collect_component(root: &Path) -> Result<(Vec<PathBuf>, Vec<(PathBuf, u64)>, u64), String> {
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut total = 0u64;
    collect_backup_entries(root, root, &mut directories, &mut files, &mut total)?;
    Ok((directories, files, total))
}

fn backup_manifest_entries(
    app_id: Option<u32>,
    game_id: &str,
    lua: Option<&Path>,
    persistent: Option<&steamtron_library::Bundle>,
    root: &Path,
    manifest_source: &Path,
) -> Vec<(String, Vec<u8>)> {
    let lua_text = lua
        .and_then(|path| std::fs::read_to_string(path).ok())
        .or_else(|| persistent.map(|bundle| bundle.lua.clone()));
    let Some(lua_text) = lua_text else { return Vec::new() };
    let app = app_id.map(|id| id.to_string()).unwrap_or_else(|| game_id.to_owned());
    let mut entries = std::collections::BTreeMap::new();
    entries.insert(format!("{app}.lua"), lua_text.as_bytes().to_vec());
    if let Some(id) = app_id {
        entries.insert(format!("{id}.smd.lua"), download::smd_lua_text(&lua_text, id).into_bytes());
    }
    let (depots, _) = parse_lua_metadata(&lua_text);
    for (depot, manifest) in depots {
        let name = format!("{depot}_{manifest}.manifest");
        let data = [root.join(&name), manifest_source.join(&name)]
            .into_iter()
            .find_map(|path| std::fs::read(path).ok())
            .or_else(|| persistent.and_then(|bundle| bundle.manifests.get(&name).cloned()));
        if let Some(data) = data {
            entries.insert(name, data);
        }
    }
    entries.into_iter().collect()
}

#[tauri::command]
async fn backup_managed_game(app: tauri::AppHandle, game_id: String, destination: Option<String>, replace: bool) -> Result<Option<serde_json::Value>, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library.game(&game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    if !game.installation.root.is_dir() {
        return Err("The installation folder does not exist.".into());
    }
    let parent = if let Some(destination) = destination {
        let parent = std::path::PathBuf::from(destination);
        if !parent.is_dir() {
            return Err("The selected backup destination is no longer available.".into());
        }
        parent
    } else {
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Choose a backup destination folder")
            .pick_folder()
        else {
            return Ok(None);
        };
        parent
    };
    if parent.starts_with(&game.installation.root) {
        return Err("Choose a backup location outside the game folder.".into());
    }
    let folder = game.installation.root.file_name().and_then(|name| name.to_str()).unwrap_or("Game").to_owned();
    let final_path = parent.join(backup_safe_folder(&game.title));
    if final_path.exists() && !replace {
        return Ok(Some(serde_json::json!({
            "exists": true,
            "parent": parent.display().to_string(),
            "target": final_path.display().to_string(),
        })));
    }
    let staging = parent.join(format!(".steamtron-backup-{}", std::process::id()));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let package = staging.join(backup_safe_folder(&game.title));
    std::fs::create_dir_all(&package).map_err(|error| error.to_string())?;

    let root = game.installation.root.clone();
    let fixes = managed_fixes_dir(&game);
    let lua = steamtron_core::library::managed_lua_path(&game);
    let app_id = game.metadata_app_id;
    let persistent = app_id.and_then(|id| steamtron_library::persistent_bundle(&id.to_string()));
    let title = game.title.clone();
    let game_id_emit = game_id.clone();
    let emit_app = app.clone();
    let control = register_operation("backup", &game_id);
    let staging_cleanup = staging.clone();
    emit_operation_at(&app, "backup", &game_id, "prepare", "running", "Collecting game files…", 0.0);

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<std::path::PathBuf, String> {
        let (content_dirs, content_files, content_total) = collect_component(&root)?;
        let (fixes_dirs, fixes_files, fixes_total) = if let Some(fixes) = fixes.as_ref().filter(|path| path.is_dir()) {
            collect_component(fixes)?
        } else {
            (Vec::new(), Vec::new(), 0)
        };
        let manifest_source = app_id
            .map(|id| steamtron_core::paths::layout().manifests_for(&id.to_string()))
            .unwrap_or_else(|| steamtron_core::paths::layout().manifests());
        let entries = backup_manifest_entries(
            app_id,
            &game.id,
            lua.as_deref(),
            persistent.as_ref(),
            &root,
            &manifest_source,
        );
        let cache = package.join(".manifest-cache");
        let mut manifest_files = Vec::new();
        if !entries.is_empty() {
            std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        }
        for (name, data) in entries {
            let path = cache.join(name);
            std::fs::write(&path, &data).map_err(|error| error.to_string())?;
            manifest_files.push((path, data.len() as u64));
        }
        let manifests_total: u64 = manifest_files.iter().map(|(_, size)| *size).sum();
        let overall_total = content_total + fixes_total + manifests_total;
        let archive_overhead = ((content_files.len() + fixes_files.len() + manifest_files.len()) as u64)
            .saturating_mul(1024)
            .saturating_add(32 * 1024 * 1024);
        let required_space = overall_total.saturating_add(archive_overhead);
        let destination = final_path.parent().unwrap_or_else(|| Path::new("."));
        check_backup_space(destination, required_space)?;
        let mut written = 0u64;
        let started = std::time::Instant::now();

        let mut content_emit = |percent, completed, total, message: &str| emit_operation_stats(&emit_app, "backup", &game_id_emit, "content", message, percent, completed, total, started);
        let (content_sha, _) = archive_component(
            &root,
            &package.join("content.tar"),
            &format!("common/{folder}"),
            &content_dirs,
            &content_files,
            content_total,
            &mut written,
            overall_total,
            &mut content_emit,
            &control,
        )?;
        operation_check(&control)?;
        let (fixes_sha, _) = if fixes_total > 0 {
            let mut fixes_emit = |percent, completed, total, message: &str| emit_operation_stats(&emit_app, "backup", &game_id_emit, "fixes", message, percent, completed, total, started);
            archive_component(
                fixes.as_ref().expect("fixes checked above"),
                &package.join("fixes.tar"),
                &format!("fixes/{folder}"),
                &fixes_dirs,
                &fixes_files,
                fixes_total,
                &mut written,
                overall_total,
                &mut fixes_emit,
                &control,
            )?
        } else {
            ("".into(), 0)
        };
        operation_check(&control)?;
        let manifest_tar = package.join("manifest-bundle.tar");
        let manifest_sha = if !manifest_files.is_empty() {
            use sha2::Digest;
            let file = std::fs::File::create(&manifest_tar).map_err(|error| error.to_string())?;
            let writer = HashingWriter { inner: file, hasher: sha2::Sha256::new() };
            let mut archive = tar::Builder::new(writer);
            let top = format!("manifests/{folder}");
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(0o755);
            header.set_cksum();
            archive.append_data(&mut header, &top, std::io::empty()).map_err(|error| error.to_string())?;
            let mut last_emit = std::time::Instant::now();
            for (source, size) in &manifest_files {
                operation_check(&control)?;
                let name = source.file_name().and_then(|name| name.to_str()).unwrap_or("manifest");
                let mut input = std::fs::File::open(source).map_err(|error| error.to_string())?;
                archive.append_file(format!("{top}/{name}"), &mut input).map_err(|error| error.to_string())?;
                written = written.saturating_add(*size);
                if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                    let percent = if overall_total > 0 { written as f64 / overall_total as f64 * 100.0 } else { 100.0 };
                    emit_operation_stats(&emit_app, "backup", &game_id_emit, "manifests", &format!("Archiving {name}"), percent, written, overall_total, started);
                    last_emit = std::time::Instant::now();
                }
            }
            let writer = archive.into_inner().map_err(|error| error.to_string())?;
            writer.inner.sync_data().map_err(|error| error.to_string())?;
            writer.hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        } else {
            String::new()
        };
        operation_check(&control)?;
        let primary = game.launch_targets.iter().find(|target| target.primary).or_else(|| game.launch_targets.first());
        let launcher_relative = primary.and_then(|target| target.executable.strip_prefix(&root).ok()).map(|path| path.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        let targets = game.launch_targets.iter().map(|target| {
            serde_json::json!({
                "id": target.id,
                "name": target.name,
                "executable": target.executable.strip_prefix(&root).map(|path| path.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| target.executable.to_string_lossy().into_owned()),
                "arguments": target.arguments,
                "primary": target.primary,
            })
        }).collect::<Vec<_>>();
        let mut components = serde_json::Map::new();
        components.insert("content".into(), serde_json::json!({"name":"content.tar","sha256":content_sha}));
        if !fixes_sha.is_empty() {
            components.insert("fixes".into(), serde_json::json!({"name":"fixes.tar","sha256":fixes_sha}));
        }
        if !manifest_sha.is_empty() {
            components.insert("manifest_bundle".into(), serde_json::json!({"name":"manifest-bundle.tar","sha256":manifest_sha}));
        }
        let metadata = serde_json::json!({
            "backup_format": 2,
            "format": 1,
            "game_id": game.id,
            "title": title,
            "steam_app_id": app_id,
            "content_directory": format!("common/{folder}"),
            "fixes_directory": if fixes_sha.is_empty() { String::new() } else { format!("fixes/{folder}") },
            "compatdata_directory": format!("compatdata/{}", game.installation.prefix.file_name().and_then(|name| name.to_str()).unwrap_or(&folder)),
            "content_archive": {"name":"content.tar","sha256":content_sha},
            "components": components,
            "launcher_relative_path": launcher_relative,
            "launcher_targets": targets,
            "launch_settings": {
                "environment": game.launch.environment,
                "prefix_mode": game.launch.prefix_mode,
                "proton": game.launch.proton,
                "requires_steam": game.launch.requires_steam,
            },
            "proton": game.launch.proton,
            "fixes_enabled": game.fixes_enabled,
        });
        let _ = std::fs::remove_dir_all(package.join(".manifest-cache"));
        emit_operation_at(&emit_app, "backup", &game_id_emit, "metadata", "running", "Writing appmanifest…", 100.0);
        let manifest_name = format!("appmanifest_{}.json", game.metadata_app_id.map(|value| value.to_string()).unwrap_or_else(|| game.id.clone()));
        std::fs::write(package.join(manifest_name), serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        emit_operation_at(&emit_app, "backup", &game_id_emit, "finalize", "running", "Finalizing backup…", 100.0);
        replace_backup_directory(&package, &final_path)?;
        let _ = std::fs::remove_dir(&staging);
        Ok(final_path)
    })
    .await;
    unregister_operation("backup", &game_id);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging_cleanup);
    }
    let final_path = result
        .map_err(|error| format!("backup task failed: {error}"))??;
    emit_operation_at(&app, "backup", &game_id, "done", "done", "Backup complete", 100.0);
    Ok(Some(serde_json::json!({ "path": final_path.display().to_string() })))
}

#[derive(serde::Deserialize)]
struct BackupMetadata {
    #[serde(default)]
    title: String,
    #[serde(default)]
    appid: Option<u32>,
    #[serde(default)]
    game_id: String,
}

struct HashingReader {
    inner: std::fs::File,
    hasher: sha2::Sha256,
}

impl std::io::Read for HashingReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use sha2::Digest;
        let count = std::io::Read::read(&mut self.inner, buf)?;
        self.hasher.update(&buf[..count]);
        Ok(count)
    }
}

impl HashingReader {
    fn finish(mut self) -> Result<String, String> {
        use sha2::Digest;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = std::io::Read::read(&mut self.inner, &mut buffer).map_err(|error| error.to_string())?;
            if count == 0 { break; }
            self.hasher.update(&buffer[..count]);
        }
        Ok(self.hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
    }
}

fn extract_tar_component(
    archive_path: &Path,
    destination: &Path,
    allowed_prefix: &str,
    app: &tauri::AppHandle,
    operation: &str,
    game_id: &str,
    stage: &str,
    message: &str,
    control: &OperationControl,
    written: &mut u64,
    overall_total: u64,
    started: std::time::Instant,
    progress_base: f64,
    progress_span: f64,
) -> Result<String, String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let file = std::fs::File::open(archive_path).map_err(|error| format!("{}: {error}", archive_path.display()))?;
    use sha2::Digest;
    let reader = HashingReader { inner: file, hasher: sha2::Sha256::new() };
    let mut archive = tar::Archive::new(reader);
    let mut last_emit = std::time::Instant::now();
    let fraction = if overall_total > 0 { *written as f64 / overall_total as f64 } else { 0.0 };
    emit_operation_stats(app, operation, game_id, stage, message, progress_base + fraction * progress_span, *written, overall_total, started);
    last_emit = std::time::Instant::now() - std::time::Duration::from_millis(200);
    for entry in archive.entries().map_err(|error| error.to_string())? {
        operation_check(control)?;
        let mut entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path().map_err(|error| error.to_string())?.into_owned();
        let text = path.to_string_lossy().replace('\\', "/");
        if !text.starts_with(allowed_prefix) || text.split('/').any(|part| part == "..") {
            return Err("Backup contains an unsafe path.".into());
        }
        let shown = text.strip_prefix(allowed_prefix).unwrap_or(&text);
        let activity = format!("Restoring {shown}");
        if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
            let fraction = if overall_total > 0 { *written as f64 / overall_total as f64 } else { 1.0 };
            emit_operation_stats(app, operation, game_id, stage, &activity, progress_base + fraction * progress_span, *written, overall_total, started);
            last_emit = std::time::Instant::now();
        }
        let target = destination.join(&path);
        if !target.starts_with(destination) {
            return Err("Backup contains an unsafe path.".into());
        }
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut output = std::fs::File::create(&target).map_err(|error| error.to_string())?;
            // Copy in bounded chunks so one huge game file still advances the
            // restore bar instead of jumping only after the whole file is done.
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                let count = std::io::Read::read(&mut entry, &mut buffer).map_err(|error| error.to_string())?;
                if count == 0 { break; }
                std::io::Write::write_all(&mut output, &buffer[..count]).map_err(|error| error.to_string())?;
                *written = (*written).saturating_add(count as u64);
                if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                    let fraction = if overall_total > 0 { *written as f64 / overall_total as f64 } else { 1.0 };
                    emit_operation_stats(app, operation, game_id, stage, &activity, progress_base + fraction * progress_span, *written, overall_total, started);
                    last_emit = std::time::Instant::now();
                }
            }
        }
        if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
            let fraction = if overall_total > 0 { *written as f64 / overall_total as f64 } else { 1.0 };
            emit_operation_stats(app, operation, game_id, stage, &activity, progress_base + fraction * progress_span, *written, overall_total, started);
            last_emit = std::time::Instant::now();
        }
    }
    let reader = archive.into_inner();
    reader.finish()
}

#[tauri::command]
async fn restore_game_backup(app: tauri::AppHandle) -> Result<Option<serde_json::Value>, String> {
    use steamtron_core::library::{LaunchTarget, Library};
    use steamtron_library::managed::{add_managed_game, ManagedAddRequest, ManifestProvider};

    require_auth()?;
    let Some(folder) = rfd::FileDialog::new()
        .set_title("Choose a SteamTron game backup folder")
        .pick_folder()
    else {
        return Ok(None);
    };
    emit_operation_at(&app, "restore", "", "read", "running", "Reading backup metadata…", 0.0);
    let manifest_path = std::fs::read_dir(&folder)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("appmanifest_") && name.ends_with(".json"))
        })
        .ok_or_else(|| "This folder is not a SteamTron game backup (missing appmanifest_*.json).".to_string())?;
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path).map_err(|error| error.to_string())?
    ).map_err(|error| format!("Backup manifest is invalid: {error}"))?;
    if manifest.get("backup_format").and_then(|value| value.as_u64()) != Some(2) {
        return Err("Unsupported SteamTron backup format.".into());
    }
    let game_id = manifest.get("game_id").and_then(|value| value.as_str()).unwrap_or("").to_owned();
    let title = manifest.get("title").and_then(|value| value.as_str()).unwrap_or("").to_owned();
    let appid = manifest.get("steam_app_id").and_then(|value| value.as_u64()).map(|value| value as u32);
    let content_directory = manifest.get("content_directory").and_then(|value| value.as_str()).unwrap_or("").to_owned();
    let content_parts: Vec<&str> = content_directory.split('/').collect();
    if content_parts.len() != 2 || content_parts[0] != "common" || content_parts[1].is_empty() {
        return Err("Backup content path is invalid.".into());
    }
    let content_folder = content_parts[1].to_owned();
    let content_tar = folder.join("content.tar");
    if !content_tar.is_file() {
        return Err("Backup does not contain content.tar.".into());
    }
    let expected_content_sha = manifest.get("content_archive").and_then(|value| value.get("sha256")).and_then(|value| value.as_str()).map(str::to_owned);

    let mut library = Library::load().map_err(|error| error.to_string())?;
    let existing = appid
        .and_then(|appid| library.game_by_app_id(appid).cloned())
        .or_else(|| if game_id.is_empty() { None } else { library.game(&game_id).cloned() })
        .or_else(|| library.games().iter().find(|game| game.title == title).cloned());
    let target_game = if let Some(game) = existing {
        game
    } else {
        let appid = appid.ok_or_else(|| "The backup has no Steam App ID and the game is not in the library.".to_string())?;
        let title = if title.trim().is_empty() { format!("Steam App {appid}") } else { title.clone() };
        let drive = library.default_drive().cloned().or_else(|| library.drives().first().cloned()).ok_or_else(|| "Register a drive before restoring a backup.".to_string())?;
        let backend = platform_backend().ok();
        let backend = backend.filter(|backend| backend.status().map(|status| status.ready).unwrap_or(false));
        let added = add_managed_game(
            ManagedAddRequest {
                app_id: appid,
                title,
                drive_root: drive.root.clone(),
                drive_label: drive.label.clone(),
                provider: ManifestProvider::Api,
                proton: String::new(),
            },
            backend.as_ref().map(|backend| backend as &dyn steamtron_register::RegistrationBackend),
        )
        .map_err(|error| error.to_string())?;
        let mut library = Library::load().map_err(|error| error.to_string())?;
        library.game(&added.game_id).cloned().ok_or_else(|| "Restored game entry was not found.".to_string())?
    };
    let target_root = target_game.installation.root.clone();
    let base = target_root.parent().and_then(|path| path.parent()).ok_or_else(|| "Installation path is invalid.".to_string())?;
    let staging = base.join(format!(".restore-{}", std::process::id()));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let control = register_operation("restore", "");
    let emit_app = app.clone();
    let restore_folder = folder.clone();
    let staging_cleanup = staging.clone();
    let target_root_worker = target_root.clone();
    let content_folder_worker = content_folder.clone();
    let fixes_directory = manifest.get("fixes_directory").and_then(|value| value.as_str()).unwrap_or("").to_owned();
    let manifest_components = manifest.get("components").cloned().unwrap_or(serde_json::Value::Null);
    let manifest_worker = manifest.clone();
    let expected_content_sha_worker = expected_content_sha.clone();
    let global_appid_worker = appid.map(|id| id.to_string());
    let target_game_worker = target_game.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        // Use archive sizes as the progress denominator. Scanning every tar
        // header first made the UI sit at 10% for minutes on multi-gigabyte
        // backups; extraction already reads the same bytes.
        let content_total = std::fs::metadata(restore_folder.join("content.tar")).map(|metadata| metadata.len()).unwrap_or(0);
        let fixes_total = if !fixes_directory.is_empty() { std::fs::metadata(restore_folder.join("fixes.tar")).map(|metadata| metadata.len()).unwrap_or(0) } else { 0 };
        let manifests_total = if manifest_worker.get("components").and_then(|value| value.get("manifest_bundle")).is_some() { std::fs::metadata(restore_folder.join("manifest-bundle.tar")).map(|metadata| metadata.len()).unwrap_or(0) } else { 0 };
        let overall_total = content_total + fixes_total + manifests_total;
        let mut written = 0u64;
        let started = std::time::Instant::now();
        let content_digest = extract_tar_component(&restore_folder.join("content.tar"), &staging, "common/", &emit_app, "restore", "", "extract", "Restoring game backup…", &control, &mut written, overall_total, started, 0.0, 100.0)?;
        if let Some(expected) = expected_content_sha_worker.as_deref() {
            if content_digest != expected {
                return Err("Backup content checksum does not match its manifest.".into());
            }
        }
        let staged_content = staging.join("common").join(&content_folder_worker);
        if !staged_content.is_dir() {
            return Err("Backup did not restore the expected game content.".into());
        }
        // Extract every component before touching the installed game. A cancel
        // or failure during extraction only discards the staging folder.
        let staged_fixes = if !fixes_directory.is_empty() {
            let _ = extract_tar_component(&restore_folder.join("fixes.tar"), &staging, "fixes/", &emit_app, "restore", "", "fixes", "Restoring fixes…", &control, &mut written, overall_total, started, 0.0, 100.0)?;
            let path = staging.join("fixes").join(&content_folder_worker);
            path.is_dir().then_some(path)
        } else {
            None
        };
        let staged_manifests = if manifest_worker.get("components").and_then(|value| value.get("manifest_bundle")).is_some() {
            let _ = extract_tar_component(&restore_folder.join("manifest-bundle.tar"), &staging, "manifests/", &emit_app, "restore", "", "manifests", "Restoring depot manifests…", &control, &mut written, overall_total, started, 0.0, 100.0)?;
            let path = staging.join("manifests").join(&content_folder_worker);
            path.is_dir().then_some(path)
        } else {
            None
        };
        operation_check(&control)?;
        let _ = manifest_components;

        emit_operation_at(&emit_app, "restore", "", "commit", "running", "Finalizing restored files…", 100.0);
        // Commit with rollback. The previous content/fixes are moved aside in
        // staging first; any commit error restores them and removes the new files.
        let old_target = staging.join(".old-target");
        if target_root_worker.exists() {
            std::fs::rename(&target_root_worker, &old_target).map_err(|error| error.to_string())?;
        }
        let fixes_destination = managed_fixes_dir(&target_game_worker);
        let old_fixes = staging.join(".old-fixes");
        if let Some(destination) = &fixes_destination {
            if destination.exists() {
                std::fs::rename(destination, &old_fixes).map_err(|error| error.to_string())?;
            }
        }
        let commit = (|| -> Result<(), String> {
            if let Some(parent) = target_root_worker.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::rename(&staged_content, &target_root_worker).map_err(|error| error.to_string())?;
            if let (Some(staged), Some(destination)) = (&staged_fixes, &fixes_destination) {
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                std::fs::rename(staged, destination).map_err(|error| error.to_string())?;
            }
            if let Some(staged_manifests) = &staged_manifests {
                for entry in std::fs::read_dir(staged_manifests).map_err(|error| error.to_string())?.flatten() {
                    if entry.path().is_file() {
                        let name = entry.file_name();
                        std::fs::copy(entry.path(), target_root_worker.join(&name)).map_err(|error| error.to_string())?;
                        if let Some(global_appid) = global_appid_worker.as_deref() {
                            let directory = steamtron_core::paths::layout().manifests_for(global_appid);
                            let _ = std::fs::create_dir_all(&directory);
                            let _ = std::fs::copy(entry.path(), directory.join(&name));
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = commit {
            let _ = std::fs::remove_dir_all(&target_root_worker);
            if old_target.exists() {
                let _ = std::fs::rename(&old_target, &target_root_worker);
            }
            if let Some(destination) = &fixes_destination {
                let _ = std::fs::remove_dir_all(destination);
                if old_fixes.exists() {
                    let _ = std::fs::rename(&old_fixes, destination);
                }
            }
            return Err(error);
        }
        if let Err(error) = operation_check(&control) {
            let _ = std::fs::remove_dir_all(&target_root_worker);
            if old_target.exists() {
                let _ = std::fs::rename(&old_target, &target_root_worker);
            }
            if let Some(destination) = &fixes_destination {
                let _ = std::fs::remove_dir_all(destination);
                if old_fixes.exists() {
                    let _ = std::fs::rename(&old_fixes, destination);
                }
            }
            return Err(error);
        }
        Ok(())
    })
    .await;
    unregister_operation("restore", "");
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging_cleanup);
    }
    result.map_err(|error| format!("restore task failed: {error}"))??;
    emit_operation_at(&app, "restore", "", "register", "running", "Finalizing library entry…", 100.0);

    let mut game = target_game;
    game.installation.root = target_root.clone();
    game.installation.confirmed = true;
    game.fixes_enabled = manifest.get("fixes_enabled").and_then(|value| value.as_bool()).unwrap_or(true);
    if let Some(settings) = manifest.get("launch_settings") {
        if let Some(mode) = settings.get("prefix_mode").and_then(|value| value.as_str()) {
            game.launch.prefix_mode = mode.to_owned();
        }
        if let Some(proton) = settings.get("proton").and_then(|value| value.as_str()) {
            game.launch.proton = proton.to_owned();
        }
        if let Some(requires) = settings.get("requires_steam").and_then(|value| value.as_bool()) {
            game.launch.requires_steam = requires;
        }
        if let Some(environment) = settings.get("environment") {
            if let Ok(environment) = serde_json::from_value(environment.clone()) {
                game.launch.environment = environment;
            }
        }
    }
    let mut targets = Vec::new();
    if let Some(items) = manifest.get("launcher_targets").and_then(|value| value.as_array()) {
        for (index, item) in items.iter().enumerate() {
            let relative = item.get("executable").and_then(|value| value.as_str()).unwrap_or("");
            if relative.is_empty() { continue; }
            let executable = target_root.join(relative);
            targets.push(LaunchTarget {
                id: item.get("id").and_then(|value| value.as_str()).map(str::to_owned).unwrap_or_else(|| format!("t{}", index + 1)),
                name: item.get("name").and_then(|value| value.as_str()).unwrap_or("Game").to_owned(),
                executable,
                arguments: item.get("arguments").and_then(|value| value.as_array()).map(|values| values.iter().filter_map(|value| value.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
                primary: item.get("primary").and_then(|value| value.as_bool()).unwrap_or(index == 0),
            });
        }
    }
    if targets.is_empty() {
        if let Some((executable, name)) = download::detect_launch_target(&target_root) {
            targets.push(LaunchTarget { id: "t1".into(), name, executable, arguments: Vec::new(), primary: true });
        }
    }
    game.launch_targets = targets;
    steamtron_core::library::mark_setup_completed(&mut game);
    let appmanifest_path = if !game.installation.manifest.as_os_str().is_empty() {
        game.installation.manifest.clone()
    } else {
        target_root.parent().and_then(|path| path.parent())
            .ok_or_else(|| "The managed library folder is invalid.".to_string())?
            .join(format!("appmanifest_{}.json", game.metadata_app_id.map(|value| value.to_string()).unwrap_or_else(|| game.id.clone())))
    };
    if let Some(parent) = appmanifest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let appmanifest_temporary = appmanifest_path.with_extension("json.steamtron.tmp");
    std::fs::write(&appmanifest_temporary, serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    std::fs::rename(&appmanifest_temporary, &appmanifest_path).map_err(|error| error.to_string())?;
    library.upsert_game(game.clone()).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    let _ = std::fs::remove_dir_all(&staging_cleanup);
    emit_operation_at(&app, "restore", "", "done", "done", "Restore complete", 100.0);
    let library = Library::load().map_err(|error| error.to_string())?;
    Ok(Some(serde_json::json!({
        "title": game.title,
        "root": target_root.display().to_string(),
        "library": library_json(&library),
    })))
}

#[tauri::command]
fn move_managed_game(game_id: String, drive_id: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library.game(&game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    let drive = library.drives().iter().find(|drive| drive.id == drive_id).cloned().ok_or_else(|| "unknown drive".to_string())?;
    let layout = steamtron_core::library::game_layout(&drive.root, &game).map_err(|error| error.to_string())?;
    if game.installation.root == layout.content {
        return Err("The game is already on that drive.".into());
    }
    if layout.content.exists() {
        return Err("A game folder already exists on the target drive.".into());
    }
    if let Some(parent) = layout.content.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if std::fs::rename(&game.installation.root, &layout.content).is_err() {
        copy_tree(&game.installation.root, &layout.content)?;
        std::fs::remove_dir_all(&game.installation.root).map_err(|error| error.to_string())?;
    }
    if let Some(fixes) = managed_fixes_dir(&game) {
        if fixes.is_dir() {
            if let Some(parent) = layout.fixes.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::rename(&fixes, &layout.fixes);
        }
    }
    game.installation.root = layout.content;
    game.installation.manifest = layout.manifest;
    game.installation.mode = steamtron_core::library::InstallMode::Managed;
    library.upsert_game(game).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(library_json(&library))
}

fn uninstall_managed_game_at(
    library_path: &std::path::Path,
    game_id: &str,
) -> Result<(String, bool), String> {
    use steamtron_core::library::Library;

    let mut library = Library::open(library_path).map_err(|error| error.to_string())?;
    let mut game = library
        .game(game_id)
        .cloned()
        .ok_or_else(|| "unknown game".to_string())?;
    if !game_process_pids(&game).is_empty() {
        return Err("Stop the game before uninstalling it.".into());
    }
    if game.installation.mode == steamtron_core::library::InstallMode::Linked {
        return Err("Unlink external games instead of deleting their folder.".into());
    }
    let root = game.installation.root.clone();
    if root.as_os_str().is_empty() || root == std::path::Path::new("/") {
        return Err("Refusing to delete an unsafe game folder.".into());
    }
    let existed = root.exists();
    if existed {
        std::fs::remove_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
    }
    // Uninstall removes content only. The library entry, metadata, artwork,
    // Steam registration, fixes, and prefix stay so the user can download the
    // game again or explicitly remove it from the library.
    game.installation.confirmed = false;
    game.launch_targets.clear();
    game.setup_completed_at = None;
    library.upsert_game(game.clone()).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok((game.title, existed))
}

#[tauri::command]
fn uninstall_managed_game(game_id: String) -> Result<serde_json::Value, String> {
    require_auth()?;
    let _ = download::cancel(&game_id);
    let (title, removed) = uninstall_managed_game_at(&steamtron_core::library::Library::default_path(), &game_id)?;
    Ok(serde_json::json!({
        "uninstalled": removed,
        "title": title,
        "kept_in_library": true,
    }))
}

#[tauri::command]
async fn regenerate_managed_manifests(game_id: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library.game(&game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    let app_id = game.metadata_app_id.ok_or_else(|| "This game has no Steam App ID.".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        use steamtron_library::managed::write_managed_manifests;

        let bundle = steamtron_library::download_bundle(&app_id.to_string()).map_err(|error| error.to_string())?;
        write_managed_manifests(&app_id.to_string(), &bundle.manifests).map_err(|error| error.to_string())?;
        Ok::<_, String>(serde_json::json!({
            "appid": app_id,
            "manifests": bundle.manifests.len(),
            "lua": format!("manifest-bundles/{app_id}.zip"),
        }))
    })
    .await
    .map_err(|error| format!("manifest task failed: {error}"))?
}

#[tauri::command]
fn export_managed_saves(game_id: String) -> Result<Option<String>, String> {
    use std::io::Write;
    use steamtron_core::library::Library;

    require_auth()?;
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library.game(&game_id).ok_or_else(|| "unknown game".to_string())?;
    let Some(path) = rfd::FileDialog::new().set_title("Export save snapshot").set_file_name(format!("{}_saves.zip", steamtron_core::fixes::export_name(&game.title).trim_end_matches(".zip"))).add_filter("ZIP", &["zip"]).save_file() else {
        return Ok(None);
    };
    let mut files = Vec::new();
    if let Some(mut folders) = save_folders(game) {
        for (relative, folder) in folders.drain(..) {
            if let Ok(entries) = walk_files(&folder) {
                for file in entries {
                    if let Ok(inside) = file.strip_prefix(&folder) {
                        let name = format!("{relative}/{}", inside.display()).replace('\\', "/");
                        files.push((file.clone(), name));
                    }
                }
            }
        }
    }
    if files.is_empty() {
        return Err("No save files were found for this game yet.".into());
    }
    let file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (source, name) in files {
        writer.start_file(name, options).map_err(|error| error.to_string())?;
        let bytes = std::fs::read(&source).map_err(|error| error.to_string())?;
        writer.write_all(&bytes).map_err(|error| error.to_string())?;
    }
    writer.finish().map_err(|error| error.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
fn import_managed_saves(game_id: String) -> Result<Option<serde_json::Value>, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library.game(&game_id).ok_or_else(|| "unknown game".to_string())?;
    let Some(path) = rfd::FileDialog::new().set_title("Import save snapshot").add_filter("ZIP", &["zip"]).pick_file() else {
        return Ok(None);
    };
    let drive_c = game.installation.prefix.join("drive_c");
    if drive_c.as_os_str().is_empty() || !game.installation.prefix.is_dir() {
        return Err("This game does not have an active prefix.".into());
    }
    let file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut count = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(name) = entry.enclosed_name() else { continue };
        let name_text = name.to_string_lossy().replace('\\', "/");
        let destination = if name_text.starts_with("SAVES/") || name_text.starts_with("Savegame/") || name_text.starts_with("bin/SAVES/") || name_text.starts_with("bin/Savegame/") {
            game.installation.root.join(&name)
        } else {
            drive_c.join(&name)
        };
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output = std::fs::File::create(&destination).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        count += 1;
    }
    Ok(Some(serde_json::json!({ "files": count })))
}

fn walk_files(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut result = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())?.flatten() {
            let path = entry.path();
            if path.is_dir() { stack.push(path); } else { result.push(path); }
        }
    }
    Ok(result)
}

fn save_folders(game: &steamtron_core::library::Game) -> Option<Vec<(String, std::path::PathBuf)>> {
    let prefix = &game.installation.prefix;
    let drive_c = prefix.join("drive_c");
    if !drive_c.is_dir() { return None; }
    let safe = game.title.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], " ");
    let candidates = [
        drive_c.join("users/steamuser/Saved Games").join(&safe),
        drive_c.join("users/steamuser/Documents/My Games").join(&safe),
        drive_c.join("users/steamuser/Documents").join(&safe),
        drive_c.join("users/steamuser/AppData/Local").join(&safe),
        drive_c.join("users/steamuser/AppData/Roaming").join(&safe),
        drive_c.join("users/Public/Documents").join(&safe),
    ];
    let mut result = Vec::new();
    for candidate in candidates {
        if candidate.is_dir() && std::fs::read_dir(&candidate).map(|mut entries| entries.next().is_some()).unwrap_or(false) {
            if let Ok(relative) = candidate.strip_prefix(&drive_c) {
                result.push((relative.display().to_string(), candidate));
            }
        }
    }
    if result.is_empty() { None } else { Some(result) }
}

fn parse_lua_metadata(lua: &str) -> (Vec<(String, String)>, String) {
    let mut depots = Vec::new();
    let mut keys = String::new();
    for line in lua.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("setManifestid(") {
            let mut parts = rest.splitn(2, ',');
            let depot = parts.next().unwrap_or("").trim().to_owned();
            let manifest_part = parts.next().unwrap_or("").trim();
            if let Some(start) = manifest_part.find('"') {
                if let Some(end) = manifest_part[start + 1..].find('"') {
                    let manifest = manifest_part[start + 1..start + 1 + end].to_owned();
                    if !depot.is_empty() && !manifest.is_empty() {
                        depots.push((depot, manifest));
                    }
                }
            }
        } else if let Some(rest) = line.strip_prefix("addappid(") {
            let mut parts = rest.splitn(2, ',');
            let depot = parts.next().unwrap_or("").trim();
            let tail = parts.next().unwrap_or("");
            if let Some(start) = tail.find('"') {
                if let Some(end) = tail[start + 1..].find('"') {
                    let key = &tail[start + 1..start + 1 + end];
                    if !depot.is_empty() && key.len() == 64 {
                        keys.push_str(&format!("{depot};{key}\n"));
                    }
                }
            }
        }
    }
    (depots, keys)
}

fn depot_downloader_path() -> Option<PathBuf> {
    let stable = steamtron_core::paths::layout().tools().join("DepotDownloaderMod");
    if stable.is_file() {
        return Some(stable);
    }
    let temp = std::env::temp_dir()
        .join("SteamManifestDownloader")
        .join("DepotDownloaderMod")
        .join("DepotDownloaderMod");
    if !temp.is_file() {
        return None;
    }
    let _ = std::fs::copy(&temp, &stable);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&stable, std::fs::Permissions::from_mode(0o755));
    }
    stable.is_file().then_some(stable)
}

fn ensure_verify_metadata(
    app_id: u32,
    lua_path: &Path,
    root: &Path,
) -> Result<Vec<(String, String)>, String> {
    let lua = std::fs::read_to_string(lua_path).map_err(|error| format!("{}: {error}", lua_path.display()))?;
    let (depots, keys) = parse_lua_metadata(&lua);
    if depots.is_empty() {
        return Ok(depots);
    }
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    if !keys.is_empty() {
        std::fs::write(root.join("steam.keys"), keys).map_err(|error| error.to_string())?;
    }
    let missing = |pair: &(String, String)| !root.join(format!("{}_{}.manifest", pair.0, pair.1)).is_file();
    if depots.iter().any(missing) {
        let cached = steamtron_core::paths::layout().manifests_for(&app_id.to_string());
        let cached_root = steamtron_core::paths::layout().manifests();
        for (depot, manifest) in &depots {
            let name = format!("{depot}_{manifest}.manifest");
            let destination = root.join(&name);
            if destination.is_file() {
                continue;
            }
            let source = [cached.join(&name), cached_root.join(&name)]
                .into_iter()
                .find(|path| path.is_file());
            if let Some(source) = source {
                std::fs::copy(&source, &destination).map_err(|error| error.to_string())?;
            }
        }
    }
    if depots.iter().any(missing) {
        let bundle = steamtron_library::download_bundle(&app_id.to_string())
            .map_err(|error| format!("Could not fetch depot manifests: {error}"))?;
        for (name, bytes) in &bundle.manifests {
            let safe = Path::new(name).file_name().and_then(|value| value.to_str()).unwrap_or("manifest");
            std::fs::write(root.join(safe), bytes).map_err(|error| error.to_string())?;
        }
    }
    if let Some((depot, manifest)) = depots.iter().find(|pair| missing(pair)) {
        return Err(format!(
            "Depot metadata for {depot}_{manifest}.manifest is unavailable from the manifest source."
        ));
    }
    Ok(depots)
}

fn run_ddm_verify(
    app: &tauri::AppHandle,
    game_id: &str,
    root: &Path,
    ddm: &Path,
    depots: &[(String, String)],
    log: &Path,
    control: &OperationControl,
) -> Result<PathBuf, String> {
    use std::io::{BufRead, BufReader, Write};
    let stderr_path = log.with_extension("stderr.log");
    let mut output = std::fs::File::create(log).map_err(|error| error.to_string())?;
    let mut stderr = std::fs::OpenOptions::new().create(true).append(true).open(&stderr_path).map_err(|error| error.to_string())?;
    let mut last_emit = std::time::Instant::now();
    for (index, (depot, manifest)) in depots.iter().enumerate() {
        operation_check(control)?;
        let manifest_file = format!("{depot}_{manifest}.manifest");
        let mut command = std::process::Command::new(ddm);
        command
            .current_dir(root)
            .args(["-app", &game_id, "-depot", depot, "-manifest", manifest, "-depotkeys", "steam.keys", "-manifestfile", &manifest_file, "-dir", ".", "-max-downloads", "8", "-verify-all"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::from(stderr.try_clone().map_err(|error| error.to_string())?));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|error| format!("could not start DepotDownloaderMod: {error}"))?;
        if let Ok(mut pid) = control.pid.lock() {
            *pid = Some(child.id());
        }
        let stdout = child.stdout.take().ok_or("DepotDownloaderMod stdout unavailable")?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let mut file_percent = 0.0f64;
        while control.cancelled.load(Ordering::SeqCst) == false && reader.read_line(&mut line).unwrap_or(0) > 0 {
            let _ = write!(output, "{line}");
            if let Some(position) = line.find('%') {
                if let Ok(percent) = line[..position].trim().parse::<f64>() {
                    file_percent = percent.clamp(0.0, 100.0);
                }
            }
            if last_emit.elapsed() >= std::time::Duration::from_millis(250) {
                let percent = (index as f64 + file_percent / 100.0) / depots.len() as f64 * 100.0;
                emit_operation_at(app, "verify", game_id, "check", "running", "Checking files against depot manifests…", percent);
                last_emit = std::time::Instant::now();
            }
            line.clear();
        }
        if control.cancelled.load(Ordering::SeqCst) {
            let _ = std::process::Command::new("kill").args(["-CONT", &format!("-{}", child.id())]).status();
            let _ = std::process::Command::new("kill").args(["-TERM", &format!("-{}", child.id())]).status();
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        if !status.success() && !control.cancelled.load(Ordering::SeqCst) {
            return Err(format!("DepotDownloaderMod failed while verifying depot {depot}."));
        }
        let percent = (index + 1) as f64 / depots.len() as f64 * 100.0;
        emit_operation_at(app, "verify", game_id, "check", "running", "Checking files against depot manifests…", percent);
    }
    Ok(log.to_path_buf())
}

#[tauri::command]
async fn verify_managed_files(app: tauri::AppHandle, game_id: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    let app_id = game
        .metadata_app_id
        .ok_or_else(|| "This game has no Steam App ID for integrity verification.".to_string())?;
    let root = game.installation.root.clone();
    if !root.is_dir() {
        return Err("The installation folder does not exist.".into());
    }
    let cache = steamtron_core::paths::layout().cache_dir().join("smd");
    let lua = steamtron_core::library::managed_lua_path(game)
        .filter(|path| path.is_file())
        .or_else(|| {
            steam::steam_root()
                .ok()
                .map(|root| root.join("config/stplug-in").join(format!("{app_id}.lua")))
                .filter(|path| path.is_file())
        });
    let lua_path = if let Some(path) = lua {
        path
    } else {
        let bundle = steamtron_library::download_bundle(&app_id.to_string())
            .map_err(|error| format!("Could not read the Lua manifest bundle: {error}"))?;
        std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        let path = cache.join(format!("{app_id}.verify.lua"));
        std::fs::write(&path, bundle.lua.as_bytes()).map_err(|error| error.to_string())?;
        path
    };
    let smd_lua = download::smd_lua(app_id, &lua_path, &cache)?;
    let logs = steamtron_core::paths::layout().diagnostics_for(&game_id);
    std::fs::create_dir_all(&logs).map_err(|error| error.to_string())?;
    let log = logs.join(format!("verify-{}.log", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|value| value.as_secs()).unwrap_or(0)));
    if let Some(ddm) = depot_downloader_path() {
        emit_operation_at(&app, "verify", &game_id, "check", "running", "Preparing depot metadata…", 0.0);
        let lua_path = lua_path.clone();
        let root_path = root.clone();
        let metadata = tauri::async_runtime::spawn_blocking(move || ensure_verify_metadata(app_id, &lua_path, &root_path))
            .await
            .map_err(|error| format!("manifest preparation failed: {error}"))??;
        if !metadata.is_empty() {
            let control = register_operation("verify", &game_id);
            let emit_app = app.clone();
            let emit_game = game_id.clone();
            let root_path = root.clone();
            let log_path = log.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                run_ddm_verify(&emit_app, &emit_game, &root_path, &ddm, &metadata, &log_path, &control)
            })
            .await;
            unregister_operation("verify", &game_id);
            let log_path = result
                .map_err(|error| format!("verification task failed: {error}"))??;
            emit_operation_at(&app, "verify", &game_id, "done", "done", "Verification complete", 100.0);
            return Ok(serde_json::json!({ "verified": true, "log": log_path.display().to_string() }));
        }
    }
    emit_operation_at(&app, "verify", &game_id, "check", "running", "Checking files against depot manifests…", 0.0);
    let emit_app = app.clone();
    let emit_game = game_id.clone();
    let control = register_operation("verify", &game_id);
    let result = tauri::async_runtime::spawn_blocking(move || {
        use std::io::{BufRead, BufReader, Write};
        let smd = download::resolve_smd()?;
        let stderr_log = log.with_extension("stderr.log");
        let stderr = std::fs::File::create(&stderr_log).map_err(|error| error.to_string())?;
        let mut command = std::process::Command::new(&smd);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .arg("--data-dir")
            .arg(cache.join("data"))
            .arg("download")
            .arg(&smd_lua)
            .args(["--update", &root.display().to_string(), "--repair", "--like-steam", "--platform", "windows", "--language", "english", "--json", "-v"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::from(stderr))
            .spawn()
            .map_err(|error| format!("could not start smd: {error}"))?;
        if let Ok(mut pid) = control.pid.lock() {
            *pid = Some(child.id());
        }
        let stdout = child.stdout.take().ok_or("smd stdout unavailable")?;
        let mut file = std::fs::File::create(&log).map_err(|error| error.to_string())?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let mut depots_total = 0u32;
        let mut depots_done = 0u32;
        let mut file_percent = 0.0f64;
        let mut last_emit = std::time::Instant::now();
        while control.cancelled.load(Ordering::SeqCst) == false
            && reader.read_line(&mut line).unwrap_or(0) > 0
        {
            let _ = write!(file, "{line}");
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else { line.clear(); continue };
            if let Some(depots) = value.get("depots").and_then(|depots| depots.as_array()) {
                depots_total = depots.len() as u32;
            }
            if let Some(total) = value.get("total").and_then(|total| total.as_u64()) {
                if total > 0 { depots_total = total as u32; }
            }
            if let Some(output) = value.get("output").and_then(|output| output.as_str()) {
                if let Some(position) = output.find('%') {
                    if let Ok(percent) = output[..position].trim().parse::<f64>() {
                        file_percent = percent.clamp(0.0, 100.0);
                    }
                }
            }
            if value.get("type").and_then(|kind| kind.as_str()) == Some("depot_complete") {
                depots_done = depots_done.saturating_add(1);
                file_percent = 0.0;
            }
            let percent = if depots_total > 0 {
                ((depots_done as f64 + file_percent / 100.0) / depots_total as f64 * 100.0).min(100.0)
            } else {
                file_percent
            };
            if last_emit.elapsed() >= std::time::Duration::from_millis(250) {
                emit_operation_at(&emit_app, "verify", &emit_game, "check", "running", "Checking files against depot manifests…", percent);
                last_emit = std::time::Instant::now();
            }
            line.clear();
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        if !status.success() {
            return Err("Integrity verification failed. See the diagnostic log.".into());
        }
        Ok::<_, String>(log)
    })
    .await;
    unregister_operation("verify", &game_id);
    let log_path = result
        .map_err(|error| format!("verification task failed: {error}"))??;
    emit_operation_at(&app, "verify", &game_id, "done", "done", "Verification complete", 100.0);
    Ok(serde_json::json!({ "verified": true, "log": log_path.display().to_string() }))
}

#[tauri::command]
async fn managed_game_size(game_id: String) -> Result<u64, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    let root = library
        .game(&game_id)
        .map(|game| game.installation.root.clone())
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    tauri::async_runtime::spawn_blocking(move || directory_size(&root))
        .await
        .map_err(|error| format!("size task failed: {error}"))
}


fn proton_cache_path(appid: u32) -> std::path::PathBuf {
    steamtron_core::paths::layout()
        .cache_dir()
        .join("protondb")
        .join(format!("{appid}.json"))
}

#[tauri::command]
async fn proton_tier(appid: u32) -> Result<Option<String>, String> {
    let path = proton_cache_path(appid);
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            let fresh = value
                .get("at")
                .and_then(|at| at.as_u64())
                .map(|at| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or(0);
                    now.saturating_sub(at) < 7 * 24 * 60 * 60
                })
                .unwrap_or(false);
            if fresh {
                if let Some(tier) = value.get("tier").and_then(|tier| tier.as_str()) {
                    return Ok(Some(tier.to_owned()));
                }
            }
        }
    }
    let url = format!("https://www.protondb.com/api/v1/reports/summaries/{appid}.json");
    let response = store_client()
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let payload: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
    let raw = payload
        .get("tier")
        .and_then(|tier| tier.as_str())
        .or_else(|| payload.get("bestReportedTier").and_then(|tier| tier.as_str()));
    let Some(raw) = raw else { return Ok(None) };
    let mut chars = raw.chars();
    let tier = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => return Ok(None),
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(
        &path,
        serde_json::json!({ "tier": tier, "at": now }).to_string(),
    );
    Ok(Some(tier))
}

fn igdb_cache_path() -> std::path::PathBuf {
    steamtron_core::paths::layout()
        .cache_dir()
        .join("store/igdb-ids.json")
}

fn read_igdb_cache() -> serde_json::Map<String, serde_json::Value> {
    let text = std::fs::read_to_string(igdb_cache_path()).unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write_igdb_cache(appid: u32, igdb_id: &str) {
    let mut cache = read_igdb_cache();
    cache.insert(appid.to_string(), serde_json::Value::String(igdb_id.to_owned()));
    let path = igdb_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(&cache) {
        let _ = std::fs::write(path, text);
    }
}

#[tauri::command]
async fn igdb_id_for(appid: u32, title: Option<String>) -> Option<String> {
    // 1. SteamTron's own cache.
    if let Some(value) = read_igdb_cache().get(&appid.to_string()) {
        if let Some(number) = value.as_u64() {
            return Some(number.to_string());
        }
        if let Some(text) = value.as_str() {
            return Some(text.to_owned());
        }
    }
    // 2. Catalogue browse by title, exact-or-edition match only.
    let title = title?;
    let data = store_get(
        "/v1/games",
        &[
            ("q", title.clone()),
            ("genre", String::new()),
            ("page", "1".to_owned()),
        ],
    )
    .await
    .ok()?;
    let items = data.get("items").and_then(|items| items.as_array())?;
    for item in items {
        let name = item.get("name").and_then(|name| name.as_str()).unwrap_or("");
        if !store_title_matches(&title, name) {
            continue;
        }
        let Some(id) = item.get("id").and_then(|id| id.as_u64()) else { continue };
        let id = id.to_string();
        write_igdb_cache(appid, &id);
        return Some(id);
    }
    None
}


fn normalise_title(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

/// Strip common edition suffixes so "Batman: Arkham Asylum" matches its
/// "Game of the Year Edition" Steam listing, without matching sequels.
fn base_title(value: &str) -> String {
    let mut title = normalise_title(value);
    const EDITIONS: &[&str] = &[
        "gameoftheyearedition",
        "gameoftheyear",
        "definitiveedition",
        "completeedition",
        "ultimateedition",
        "deluxeedition",
        "premiumedition",
        "enhancededition",
        "specialedition",
        "goldedition",
        "legacyedition",
        "digitaldeluxe",
        "collectorsedition",
        "anniversaryedition",
        "directorscut",
        "remastered",
        "goty",
        "standardedition",
    ];
    for edition in EDITIONS {
        while title.contains(edition) {
            title = title.replace(edition, "");
        }
    }
    title
}

fn store_title_matches(wanted: &str, candidate: &str) -> bool {
    let wanted = base_title(wanted);
    let candidate = base_title(candidate);
    if wanted.is_empty() {
        return false;
    }
    if wanted == candidate {
        return true;
    }
    // Steam often appends a subtitle to the catalogue title, e.g.
    // "Cricket 26 - The Official Game of the Ashes". Keep the base title equal
    // and reject numbered sequels ("Red Dead Redemption" vs "… 2").
    candidate
        .strip_prefix(&wanted)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|next| !next.is_ascii_digit())
}

async fn search_steam_title(term: &str) -> Result<Option<u32>, String> {
    if term.trim().is_empty() {
        return Ok(None);
    }
    let response = store_client()
        .get("https://store.steampowered.com/api/storesearch/")
        .query(&[("term", term.trim()), ("l", "english"), ("cc", "US")])
        .header("Accept", "application/json")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let payload: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
    for item in payload.get("items").and_then(|items| items.as_array()).into_iter().flatten() {
        let name = item.get("name").and_then(|name| name.as_str()).unwrap_or("");
        // Match the base title only: edition suffixes are fine, sequels are not.
        if store_title_matches(term, name) {
            if let Some(appid) = item.get("id").and_then(|id| id.as_u64()) {
                if appid > 0 && appid <= u32::MAX as u64 {
                    return Ok(Some(appid as u32));
                }
            }
        }
    }
    Ok(None)
}

#[tauri::command]
async fn store_resolve_steam(title: String) -> Result<Option<u32>, String> {
    search_steam_title(&title).await
}

fn steam_id_cache_path() -> std::path::PathBuf {
    steamtron_core::paths::layout()
        .cache_dir()
        .join("store/steam-ids.json")
}

fn read_steam_id_cache() -> serde_json::Map<String, serde_json::Value> {
    let text = std::fs::read_to_string(steam_id_cache_path()).unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write_steam_id_cache(igdb_id: u64, steam_id: Option<u32>) {
    let mut cache = read_steam_id_cache();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    cache.insert(
        igdb_id.to_string(),
        serde_json::json!({ "steam": steam_id, "at": now }),
    );
    let path = steam_id_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(&cache) {
        let _ = std::fs::write(path, text);
    }
}

#[tauri::command]
async fn store_resolve_steam_cached(igdb_id: u64, title: String) -> Result<Option<u32>, String> {
    if let Some(entry) = read_steam_id_cache().get(&igdb_id.to_string()) {
        let fresh = entry
            .get("at")
            .and_then(|at| at.as_u64())
            .map(|at| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_secs())
                    .unwrap_or(0);
                now.saturating_sub(at) < 7 * 24 * 60 * 60
            })
            .unwrap_or(false);
        if fresh {
            if let Some(steam) = entry.get("steam").and_then(|steam| steam.as_u64()) {
                return Ok(Some(steam as u32));
            }
            // Do not trust a cached negative: title matching may have improved
            // since the entry was written.
        }
    }
    let steam = search_steam_title(&title).await?;
    write_steam_id_cache(igdb_id, steam);
    Ok(steam)
}

fn steam_size_cache_path(appid: u32) -> std::path::PathBuf {
    steamtron_core::paths::layout()
        .cache_dir()
        .join("store/sizes")
        .join(format!("{appid}.json"))
}

#[tauri::command]
async fn steam_download_size(appid: u32) -> Result<u64, String> {
    let path = steam_size_cache_path(appid);
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            let fresh = value
                .get("at")
                .and_then(|at| at.as_u64())
                .map(|at| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or(0);
                    now.saturating_sub(at) < 24 * 60 * 60
                })
                .unwrap_or(false);
            if fresh {
                if let Some(size) = value.get("size").and_then(|size| size.as_u64()) {
                    if size > 0 {
                        return Ok(size);
                    }
                }
            }
        }
    }
    let response = store_client()
        .get(format!("https://api.steamcmd.net/v1/info/{appid}"))
        .header("Accept", "application/json")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Ok(0);
    }
    let payload: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
    let depots = payload
        .get("data")
        .and_then(|data| data.get(appid.to_string()))
        .and_then(|info| info.get("depots"))
        .and_then(|depots| depots.as_object());
    let mut total = 0u64;
    if let Some(depots) = depots {
        for (key, depot) in depots {
            if !key.chars().all(|character| character.is_ascii_digit()) {
                continue;
            }
            // Skip DLC and shared redistributable depots: the detail size is
            // for the base app, matching Steam's install size.
            if depot.get("dlcappid").is_some() || depot.get("sharedinstall").is_some() {
                continue;
            }
            let manifest = depot.get("manifests").and_then(|value| value.get("public"));
            let size = manifest
                .and_then(|value| value.get("size"))
                .and_then(|value| value.as_str())
                .and_then(|value| value.parse::<u64>().ok())
                .or_else(|| {
                    manifest
                        .and_then(|value| value.get("download"))
                        .and_then(|value| value.as_str())
                        .and_then(|value| value.parse::<u64>().ok())
                })
                .or_else(|| depot.get("maxsize").and_then(|value| value.as_u64()))
                .unwrap_or(0);
            total = total.saturating_add(size);
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(&path, serde_json::json!({ "size": total, "at": now }).to_string());
    Ok(total)
}

#[tauri::command]
async fn remove_managed_game(game_id: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    if managed_runs()
        .lock()
        .map_err(|_| "Game state lock failed.".to_string())?
        .contains_key(&game_id)
    {
        return Err("Stop the game before removing it from the library.".into());
    }
    // Removing the entry while its content is downloading would orphan the
    // downloader and leave a stale Downloading state in the open window.
    let _ = download::cancel(&game_id);
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .cloned()
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    let mut unregistered = false;
    if let Some(registration) = game.steam.as_ref() {
        if let Ok(backend) = platform_backend() {
            if backend
                .verify(&registration.app_id.to_string())
                .map(|state| state.registered)
                .unwrap_or(false)
            {
                backend
                    .unregister(&registration.app_id.to_string())
                    .map_err(|error| error.to_string())?;
                unregistered = true;
            }
        }
    }
    library.remove_game(&game_id);
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "title": game.title,
        "unregistered": unregistered,
    }))
}


#[tauri::command]
fn choose_prelaunch_script() -> Result<Option<serde_json::Value>, String> {
    require_auth()?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose pre-launch script")
        .pick_file()
    else {
        return Ok(None);
    };
    let sha256 = steamtron_core::prelaunch::approve(&path)?;
    Ok(Some(serde_json::json!({
        "path": path.display().to_string(),
        "sha256": sha256,
    })))
}

#[tauri::command]
fn set_managed_prelaunch(
    game_id: String,
    path: Option<String>,
    sha256: Option<String>,
) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library
        .game(&game_id)
        .cloned()
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    let path = path.unwrap_or_default();
    if path.trim().is_empty() {
        game.launch.prelaunch.clear();
        game.launch.prelaunch_sha256.clear();
    } else {
        let script = std::path::PathBuf::from(path.trim());
        let approved = sha256.unwrap_or_default();
        steamtron_core::prelaunch::verify(&script.display().to_string(), &approved)?;
        game.launch.prelaunch = script.display().to_string();
        game.launch.prelaunch_sha256 = approved.to_ascii_lowercase();
    }
    library
        .upsert_game(game.clone())
        .map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!(game))
}

#[tauri::command]
fn update_managed_launch_settings(
    game_id: String,
    proton: Option<String>,
    arguments: Option<Vec<String>>,
    environment: Option<std::collections::BTreeMap<String, String>>,
    prefix_mode: Option<String>,
    requires_steam: Option<bool>,
) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library
        .game(&game_id)
        .cloned()
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    if let Some(proton) = proton {
        let proton = proton.trim().to_owned();
        if !proton.is_empty() && !std::path::Path::new(&proton).exists() {
            return Err(format!("Proton runner does not exist: {proton}"));
        }
        game.launch.proton = proton.clone();
        game.installation.proton = proton;
    }
    if let Some(arguments) = arguments {
        game.launch.arguments = arguments;
    }
    if let Some(environment) = environment {
        game.launch.environment = environment;
    }
    if let Some(mode) = prefix_mode {
        let mode = mode.trim().to_owned();
        if mode != "steam" && mode != "umu" {
            return Err("prefix source must be steam or umu".into());
        }
        if mode == "steam" {
            let app_id = game
                .metadata_app_id
                .ok_or_else(|| "Steam prefix needs a Steam App ID".to_string())?;
            let steam_root = steam::steam_root().map_err(|error| error.to_string())?;
            game.installation.prefix = steam_root
                .join("steamapps")
                .join("compatdata")
                .join(app_id.to_string())
                .join("pfx");
        } else {
            let prefix = steamtron_core::library::drive_root_for(&game.installation.root)
                .and_then(|drive| steamtron_core::library::game_layout(&drive, &game).ok())
                .map(|layout| layout.prefix)
                .unwrap_or_else(|| {
                    let folder = steamtron_core::library::safe_folder(&game.title)
                        .unwrap_or_else(|_| game.id.clone());
                    steamtron_core::paths::layout().prefixes().join(folder)
                });
            game.installation.prefix = prefix;
        }
        game.launch.prefix_mode = mode;
    }
    if let Some(requires_steam) = requires_steam {
        game.launch.requires_steam = requires_steam;
        if requires_steam {
            game.launch.environment.remove("PROTON_DISABLE_LSTEAMCLIENT");
            if let Some(overrides) = game.launch.environment.get_mut("WINEDLLOVERRIDES") {
                let kept: Vec<&str> = overrides
                    .split(';')
                    .filter(|part| !part.split('=').next().is_some_and(|name| name.trim().eq_ignore_ascii_case("lsteamclient")))
                    .filter(|part| !part.is_empty())
                    .collect();
                if kept.is_empty() {
                    game.launch.environment.remove("WINEDLLOVERRIDES");
                } else {
                    *overrides = kept.join(";");
                }
            }
            if game.launch.prefix_mode != "steam" {
                if let Some(app_id) = game.metadata_app_id {
                    let steam_root = steam::steam_root().map_err(|error| error.to_string())?;
                    game.installation.prefix = steam_root
                        .join("steamapps")
                        .join("compatdata")
                        .join(app_id.to_string())
                        .join("pfx");
                    game.launch.prefix_mode = "steam".into();
                }
            }
        }
    }
    library
        .upsert_game(game.clone())
        .map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!(game))
}

fn managed_log_dir(game_id: &str) -> std::path::PathBuf {
    steamtron_core::paths::layout().diagnostics_for(game_id)
}

#[tauri::command]
fn managed_game_log(game_id: String) -> Result<serde_json::Value, String> {
    let directory = managed_log_dir(&game_id);
    let entries = std::fs::read_dir(&directory).map_err(|error| error.to_string())?;
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("play-") || !name.ends_with(".log") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if newest.as_ref().map(|(time, _)| modified > *time).unwrap_or(true) {
            newest = Some((modified, path));
        }
    }
    let Some((_, path)) = newest else {
        return Ok(serde_json::json!({ "path": "", "content": "" }));
    };
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    let start = bytes.len().saturating_sub(64 * 1024);
    let content = String::from_utf8_lossy(&bytes[start..]).to_string();
    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "content": content,
    }))
}

#[tauri::command]
fn clear_managed_game_logs(game_id: String) -> Result<usize, String> {
    let directory = managed_log_dir(&game_id);
    let Ok(entries) = std::fs::read_dir(&directory) else { return Ok(0) };
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("play-") && name.ends_with(".log") {
            if std::fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}


#[tauri::command]
fn prepare_managed_download(game_id: String, drive_id: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library
        .game(&game_id)
        .cloned()
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    if !game_process_pids(&game).is_empty() {
        return Err("Stop the game before downloading it.".into());
    }
    let drive = library
        .drives()
        .iter()
        .find(|drive| drive.id == drive_id)
        .cloned()
        .ok_or_else(|| "unknown drive".to_string())?;
    let layout = steamtron_core::library::game_layout(&drive.root, &game)
        .map_err(|error| error.to_string())?;
    game.installation.mode = steamtron_core::library::InstallMode::Managed;
    game.installation.root = layout.content;
    game.installation.prefix = layout.prefix;
    game.installation.manifest = layout.manifest;
    library.upsert_game(game).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(library_json(&library))
}

#[tauri::command]
async fn start_managed_download(app: tauri::AppHandle, game_id: String) -> Result<download::DownloadState, String> {
    tauri::async_runtime::spawn_blocking(move || download::start(app, game_id))
        .await
        .map_err(|error| format!("download task failed: {error}"))?
}

#[tauri::command]
fn cancel_managed_download(game_id: String) -> Result<(), String> {
    download::cancel(&game_id)
}

#[tauri::command]
fn pause_managed_download(game_id: String) -> Result<(), String> {
    download::pause(&game_id)
}

#[tauri::command]
fn resume_managed_download(game_id: String) -> Result<(), String> {
    download::resume(&game_id)
}

#[tauri::command]
fn managed_download_status(game_id: String) -> Option<download::DownloadState> {
    download::status(&game_id)
}


#[tauri::command]
fn detach_umutron_paths() -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    let mut library = Library::load().map_err(|error| error.to_string())?;
    let detached = library.detach_umutron_paths();
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "detached": detached,
        "library": library_json(&library),
    }))
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg(target_os = "linux")]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "linux")]
mod linux_tray {
    use ksni::blocking::TrayMethods;
    use ksni::menu::{MenuItem, StandardItem};

    struct SteamTronTray {
        app: tauri::AppHandle,
        icons: Vec<ksni::Icon>,
    }

    impl ksni::Tray for SteamTronTray {
        const MENU_ON_ACTIVATE: bool = false;

        fn id(&self) -> String {
            "steamtron".into()
        }

        fn title(&self) -> String {
            "SteamTron".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icons.clone()
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            super::show_main_window(&self.app);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                StandardItem {
                    label: "Show SteamTron".into(),
                    activate: Box::new(|tray: &mut SteamTronTray| super::show_main_window(&tray.app)),
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                StandardItem {
                    label: "Quit SteamTron".into(),
                    activate: Box::new(|tray: &mut SteamTronTray| tray.app.exit(0)),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    pub fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
        let Some(image) = app.default_window_icon() else {
            return Ok(());
        };
        let rgba = image.rgba();
        let mut data = vec![0u8; rgba.len()];
        for (source, target) in rgba.chunks_exact(4).zip(data.chunks_exact_mut(4)) {
            target[0] = source[3];
            target[1] = source[0];
            target[2] = source[1];
            target[3] = source[2];
        }
        let tray = SteamTronTray {
            app: app.handle().clone(),
            icons: vec![ksni::Icon {
                width: image.width() as i32,
                height: image.height() as i32,
                data,
            }],
        };
        let handle = tray.spawn()?;
        std::mem::forget(handle);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    linux_tray::setup(app)
}

#[cfg(not(target_os = "linux"))]
fn setup_tray(_app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[tauri::command]
fn reconcile_registrations() -> Result<steamtron_library::managed::ReconcileReport, String> {
    let backend = platform_backend().map_err(|error| error.to_string())?;
    steamtron_library::managed::reconcile_registrations(&backend)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn add_managed_game(
    request: steamtron_library::managed::ManagedAddRequest,
) -> Result<steamtron_library::managed::ManagedAdded, String> {
    use steamtron_library::managed::ManifestProvider;

    if request.provider == ManifestProvider::Api {
        require_auth()?;
    }
    let backend = platform_backend().ok();
    let backend = backend.filter(|backend| {
        backend
            .status()
            .map(|status| status.ready)
            .unwrap_or(false)
    });
    // Manifest download and optional Steam registration are blocking; keep
    // them off the main thread so the window stays responsive.
    tauri::async_runtime::spawn_blocking(move || {
        let backend = backend.as_ref().map(|backend| backend as &dyn steamtron_register::RegistrationBackend);
        steamtron_library::managed::add_managed_game(request, backend)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("add task failed: {error}"))?
}

#[tauri::command]
fn registration_verify(appid: String) -> Result<steamtron_register::RegistrationState, String> {
    let backend = platform_backend().map_err(|error| error.to_string())?;
    backend.verify(&appid).map_err(|error| error.to_string())
}

#[tauri::command]
fn library_snapshot() -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    // Use the enriched projection: raw Game structs do not carry the live
    // `installed` flag, prefix state, or recent-play order the UI depends on.
    Ok(library_json(&library))
}

#[tauri::command]
fn library_remember_picker(key: String, folder: String) -> Result<(), String> {
    use steamtron_core::library::Library;

    let mut library = Library::load().map_err(|error| error.to_string())?;
    library
        .set_picker_folder(&key, folder)
        .map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())
}


fn proton_install_child() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("--proton-install") {
        return None;
    }
    let family = args.get(2)?;
    let tag = args.get(3)?;
    let name = args.get(4)?;
    let resume = args.get(5).map(String::as_str) == Some("--resume");
    Some(proton_sources::run_install(family, tag, name, resume))
}

fn main() {
    if let Some(code) = proton_install_child() {
        std::process::exit(code);
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    if let Some(code) = try_overlay_wrapper() {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.maximize();
            }
            setup_tray(app)?;
            std::thread::spawn(|| {
                let _ = download::resolve_smd();
            });
            std::thread::spawn(|| {
                steamtron_library::migrate_manifest_cache();
                steamtron_library::managed::cleanup_legacy_lua_files();
                steamtron_library::managed::organize_manifest_store();
                steamtron_library::managed::populate_missing_appmanifests();
            });
            download::restore(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                #[cfg(target_os = "linux")]
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            backend_status,
            export_game_backups,
            notify_task_result,
            pick_import_paths,
            import_game_metadata,
            game_artwork,
            search_games,
            add_game,
            import_game_file,
            remove_game,
            update_versions,
            update_pin,
            clear_update_pin,
            install_desktop_entry,
            repair_launch_options,
            repair_library_entries,
            restart_steam,
            steam_running,
            ensure_steam_running,
            profile_status,
            profile_login,
            profile_logout,
            doctor_report,
            repair_steam_library,
            list_games,
            list_fixes,
            auto_launch_targets,
            auto_pin_installed,
            reconcile_managed_installs,
            pick_launch_executable,
            add_launch_target,
            update_launch_target,
            remove_launch_target,
            play_game,
            play_steam_game,
            stop_game,
            game_running,
            windows_security_supported,
            windows_security_status,
            windows_security_apply,
            windows_security_remove,
            windows_security_repair,
            install_fixes,
            use_fixes_folder,
            open_fixes_folder,
            export_fixes,
            apply_launch_option,
            remove_fixes,
            library_snapshot,
            add_managed_game,
            play_managed,
            reconcile_registrations,
            quit_app,
            managed_game_size,
            verify_managed_files,
            backup_managed_game,
            restore_game_backup,
            move_managed_game,
            export_managed_saves,
            import_managed_saves,
            uninstall_managed_game,
            cancel_operation,
            regenerate_managed_manifests,
            steam_download_size,
            remove_managed_game,
            detach_umutron_paths,
            update_managed_launch_settings,
            choose_prelaunch_script,
            set_managed_prelaunch,
            managed_game_log,
            start_managed_download,
            prepare_managed_download,
            cancel_managed_download,
            pause_managed_download,
            resume_managed_download,
            managed_download_status,
            clear_managed_game_logs,
            proton_tier,
            igdb_id_for,
            store_resolve_steam,
            store_resolve_steam_cached,
            fullscreen_state,
            toggle_fullscreen,
            set_ui_scale,
            reboot_system,
            shutdown_system,
            managed_artwork,
            store_genres,
            store_cached_genres,
            store_cached_catalog,
            store_catalog,
            store_game,
            managed_dlc,
            app_details,
            app_version,
            update_library_settings,
            add_library_drive,
            available_library_drives,
            set_default_library_drive,
            remove_library_drive,
            diagnostics_summary,
            clear_diagnostics,
            proton_runners,
            proton_source_releases,
            install_proton_release,
            proton_install_states,
            resume_proton_install,
            pause_proton_install,
            cancel_proton_install,
            uninstall_proton_release,
            set_default_proton_runner,
            open_path,
            running_managed,
            stop_managed,
            managed_fixes,
            managed_fixes_status,
            export_managed_fixes,
            apply_managed_goldberg,
            remove_managed_goldberg,
            apply_managed_steamless,
            remove_managed_steamless,
            install_managed_fixes,
            remove_managed_fixes,
            pick_managed_executable,
            set_managed_launch_target,
            pick_managed_folder,
            link_managed_game,
            registration_verify,
            library_remember_picker,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SteamTron");
}

#[cfg(test)]
mod artwork_title_tests {
    use super::*;

    #[test]
    fn edition_suffixes_match_the_base_game() {
        assert!(store_title_matches(
            "Batman: Arkham Asylum",
            "Batman: Arkham Asylum Game of the Year Edition"
        ));
        assert!(store_title_matches("Red Dead Redemption", "Red Dead Redemption"));
    }

    #[test]
    fn sequels_and_different_titles_do_not_match() {
        assert!(!store_title_matches("Red Dead Redemption", "Red Dead Redemption 2"));
        assert!(!store_title_matches("Uncharted 4: A Thief's End", "Uncharted: Legacy of Thieves Collection"));
        assert!(!store_title_matches("Batman: Arkham Asylum", "Batman: Arkham City"));
    }
}

#[cfg(test)]
mod backup_manifest_tests {
    use super::*;

    #[test]
    fn progress_reader_reports_incremental_chunks_and_honours_cancel() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("large.bin");
        std::fs::write(&path, vec![7u8; 1024 * 1024]).unwrap();
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let mut counted = 0u64;
        let mut calls = 0usize;
        {
            let mut reader = ProgressReader {
                inner: std::fs::File::open(&path).unwrap(),
                on_read: &mut |count| { counted += count; calls += 1; },
                cancelled: &cancelled,
            };
            let mut sink = std::io::sink();
            std::io::copy(&mut reader, &mut sink).unwrap();
        }
        assert_eq!(counted, 1024 * 1024);
        assert!(calls > 1);

        cancelled.store(true, Ordering::SeqCst);
        let mut reader = ProgressReader {
            inner: std::fs::File::open(&path).unwrap(),
            on_read: &mut |_| {},
            cancelled: &cancelled,
        };
        let mut buffer = [0u8; 16];
        assert!(std::io::Read::read(&mut reader, &mut buffer).is_err());
    }

    #[test]
    fn task_notification_titles_cover_success_and_failure() {
        assert_eq!(notification_summary("backup", true).0, "Backup complete");
        assert_eq!(notification_summary("backup", false).0, "Backup failed");
        assert_eq!(notification_summary("download", true).0, "Download complete");
        assert_eq!(notification_summary("library-import", false).0, "Import failed");
        assert_eq!(notification_summary("unknown", true).0, "Task complete");
        assert_eq!(truncate_notification("abcdef", 3), "abc…");
    }

    #[test]
    fn backup_size_and_free_space_are_reported() {
        assert_eq!(backup_size(1024 * 1024 * 1024), "1.0 GiB");
        let temp = tempfile::tempdir().unwrap();
        let available = available_space(temp.path()).expect("statvfs on a temp directory");
        assert!(available > 0);
        let error = check_backup_space(temp.path(), u64::MAX).unwrap_err();
        assert!(error.contains("Not enough free space"));
    }

    #[test]
    fn replacing_a_backup_keeps_the_new_tree_and_removes_the_previous() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("staging/Game");
        let final_path = temp.path().join("dest/Game");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::create_dir_all(&final_path).unwrap();
        std::fs::write(package.join("new"), b"new").unwrap();
        std::fs::write(final_path.join("old"), b"old").unwrap();
        replace_backup_directory(&package, &final_path).unwrap();
        assert!(final_path.join("new").is_file());
        assert!(!final_path.join("old").exists());
        assert!(!temp.path().join("dest").read_dir().unwrap().any(|entry| {
            entry.unwrap().file_name().to_string_lossy().contains(".previous-")
        }));
    }

    #[test]
    fn archive_component_writes_the_whole_file_and_finishes_at_100_percent() {
        use std::io::Read;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        let data = vec![9u8; 1024 * 1024];
        std::fs::write(source.join("large.bin"), &data).unwrap();
        let tar_path = temp.path().join("content.tar");
        let files = vec![(PathBuf::from("large.bin"), data.len() as u64)];
        let control = OperationControl {
            cancelled: Arc::new(AtomicBool::new(false)),
            pid: Arc::new(Mutex::new(None)),
        };
        let mut written = 0u64;
        let mut percents = Vec::new();
        let mut activities = Vec::new();
        archive_component(
            &source,
            &tar_path,
            "common/Sample",
            &[],
            &files,
            data.len() as u64,
            &mut written,
            data.len() as u64,
            &mut |percent, _, _, message| { percents.push(percent); activities.push(message.to_owned()); },
            &control,
        ).unwrap();
        assert_eq!(written, data.len() as u64);
        assert_eq!(percents.last().copied(), Some(100.0));
        assert!(activities.iter().any(|message| message.contains("Archiving large.bin")));
        let mut archive = tar::Archive::new(std::fs::File::open(&tar_path).unwrap());
        let mut restored = Vec::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap().to_string_lossy().ends_with("large.bin") {
                entry.read_to_end(&mut restored).unwrap();
            }
        }
        assert_eq!(restored, data);
    }

    #[test]
    fn backup_includes_lua_smd_lua_and_bundle_manifests() {
        let mut manifests = std::collections::BTreeMap::new();
        manifests.insert("42_1.manifest".to_owned(), vec![1, 2, 3]);
        let bundle = steamtron_library::Bundle {
            lua: "addappid(42,0,\"key\")\nsetManifestid(42,\"1\")\n".to_owned(),
            manifests,
        };
        let entries = backup_manifest_entries(
            Some(42),
            "42",
            None,
            Some(&bundle),
            Path::new("/nonexistent-game-root"),
            Path::new("/nonexistent-manifest-store"),
        );
        let names: Vec<&str> = entries.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["42.lua", "42.smd.lua", "42_1.manifest"]);
        assert_eq!(entries.iter().find(|(name, _)| name == "42_1.manifest").unwrap().1, vec![1, 2, 3]);
    }
}

#[cfg(test)]
mod uninstall_tests {
    use super::*;
    use steamtron_core::library::{
        Artwork, Game, InstallMode, Installation, LaunchSettings, LaunchTarget, Library,
    };

    #[test]
    fn uninstall_removes_content_but_keeps_the_library_entry() {
        let temp = tempfile::tempdir().unwrap();
        let library_path = temp.path().join("library.json");
        let root = temp.path().join("game");
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("game.exe");
        std::fs::write(&exe, b"exe").unwrap();

        let mut library = Library::open(&library_path).unwrap();
        let game = Game {
            id: "42".into(),
            title: "Sample Game".into(),
            metadata_app_id: None,
            metadata_source: None,
            installation: Installation {
                mode: InstallMode::Managed,
                root: root.clone(),
                prefix: temp.path().join("prefix"),
                proton: String::new(),
                manifest: temp.path().join("appmanifest_42.json"),
                confirmed: true,
            },
            steam: None,
            launch: LaunchSettings::default(),
            launch_targets: vec![LaunchTarget {
                id: "t1".into(),
                name: "Game".into(),
                executable: exe,
                arguments: Vec::new(),
                primary: true,
            }],
            fixes_enabled: true,
            artwork: Artwork::default(),
            setup_completed_at: Some(1),
        };
        library.upsert_game(game).unwrap();
        library.save().unwrap();

        let (title, removed) = uninstall_managed_game_at(&library_path, "42").unwrap();
        assert_eq!(title, "Sample Game");
        assert!(removed);
        assert!(!root.exists());

        let library = Library::open(&library_path).unwrap();
        let game = library.game("42").expect("library entry stays after uninstall");
        assert!(!game.installation.confirmed);
        assert!(game.launch_targets.is_empty());
        assert!(game.setup_completed_at.is_none());
    }
}
