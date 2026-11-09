#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

mod artwork;

use serde::Serialize;
#[cfg(target_os = "linux")]
use tauri::Manager;
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
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
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
                        return Err("SLSsteam was not installed within 180 seconds. Check ~/.local/share/steamtron/installer.log for the installer error.".into());
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let lumen = home.join(".local/share/Lumen");
            let webkit = home.join(".local/share/Steam/steamui/webkit");
            for file in ["luatools.js", "steamdb-webkit.css"] {
                let _ = std::fs::remove_file(webkit.join(file));
            }
            if lumen.exists() {
                let _ = std::process::Command::new("pkill")
                    .arg("-f")
                    .arg(lumen.join("lumen"))
                    .status();
                let _ = std::fs::remove_dir_all(&lumen);
            }
            let repaired = repair_missing_appmanifests().unwrap_or_default();
            Ok(format!(
                "SLSsteam is installed; Lumen was removed. Recreated {} appmanifest{}. You can now start Steam manually.",
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

#[tauri::command]
async fn export_backup() -> Result<Option<backup::BackupSummary>, String> {
    require_auth()?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Save SteamTron library backup")
        .set_file_name("steamtron-library-backup.zip")
        .add_filter("SteamTron backup", &["zip"])
        .save_file()
    else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        backup::export_backup(&path, None).map(Some).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Backup task failed: {error}"))?
}

#[tauri::command]
async fn restore_backup() -> Result<Option<backup::RestoreSummary>, String> {
    require_auth()?;
    require_steam_stopped()?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose SteamTron library backup")
        .add_filter("SteamTron backup", &["zip"])
        .pick_file()
    else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        backup::restore_backup(&path).map(Some).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Restore task failed: {error}"))?
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
fn stop_game(appid: String) -> Result<(), String> {
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
    let name = export_name(&game_title, &appid);
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
fn set_enabled(appid: String, enabled: bool) -> Result<String, String> {
    require_auth()?;
    require_steam_stopped()?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    let Some(entry) = config.entry(&appid) else {
        return Err("No fixes configured for this game.".into());
    };
    let fixes = entry.fixes.clone();
    config
        .upsert(&appid, fixes, enabled)
        .map_err(|error| error.to_string())?;
    config
        .set_launch_option_disabled(&appid, !enabled)
        .map_err(|error| error.to_string())?;
    configure_launch_option(&appid, enabled)?;
    Ok(if enabled {
        "Fixes enabled and Steam launch option set. Start Steam manually.".into()
    } else {
        "Fixes disabled and Steam launch option cleared. Start Steam manually.".into()
    })
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

#[tauri::command]
fn play_managed(game_id: String) -> Result<serde_json::Value, String> {
    use std::fs::File;
    use std::process::Stdio;
    use steamtron_core::library::{LaunchMode, Library};
    use steamtron_core::runtime::{managed_launch_plan, primary_target, steam_launch_plan};

    require_auth()?;
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("Unknown game {game_id}"))?;
    let steam_root = steam::steam_root().map_err(|error| error.to_string())?;
    let steam_mode = game.launch.requires_steam
        || game
            .steam
            .as_ref()
            .is_some_and(|registration| matches!(registration.launch_mode, LaunchMode::SteamClient));

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

    let (mut command, mode, program, arguments, overlay, fixes) = if steam_mode {
        let backend = platform_backend().map_err(|error| error.to_string())?;
        let app_id = steamtron_library::managed::steam_ready(game, &backend)
            .map_err(|error| error.to_string())?;
        let plan = steam_launch_plan(app_id, &steam_root);
        let command = if cfg!(unix) {
            let mut command = std::process::Command::new("setsid");
            command.arg(&plan.program).args(&plan.arguments);
            command
        } else {
            let mut command = std::process::Command::new(&plan.program);
            command.args(&plan.arguments);
            command
        };
        (
            command,
            "steam",
            plan.program.display().to_string(),
            plan.arguments,
            false,
            None,
        )
    } else {
        let target = primary_target(game)
            .cloned()
            .ok_or_else(|| "This game has no launch target yet.".to_string())?;
        let plan = managed_launch_plan(game, &target, &steam_root)
            .map_err(|error| error.to_string())?;
        let fixes = managed_fixes_dir(game);
        let use_overlay =
            game.fixes_enabled && fixes.as_ref().is_some_and(|path| dir_has_entries(path));
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
                    &steam_root.display().to_string(),
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

    if !steam_mode {
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
    }
    let child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|error| format!("Could not start the game: {error}"))?;
    let pid = child.id();
    if !steam_mode {
        managed_runs()
            .lock()
            .map_err(|_| "Game state lock failed.".to_string())?
            .insert(game.id.clone(), pid);
    }
    let tracked = game.id.clone();
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
        if !steam_mode {
            if let Ok(mut runs) = managed_runs().lock() {
                if runs.get(&tracked) == Some(&pid) {
                    runs.remove(&tracked);
                }
            }
        }
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

#[tauri::command]
fn running_managed() -> Result<Vec<String>, String> {
    let runs = managed_runs()
        .lock()
        .map_err(|_| "Game state lock failed.".to_string())?;
    let mut games: Vec<String> = runs.keys().cloned().collect();
    games.sort();
    Ok(games)
}

#[tauri::command]
fn stop_managed(game_id: String) -> Result<(), String> {
    let pid = managed_runs()
        .lock()
        .map_err(|_| "Game state lock failed.".to_string())?
        .get(&game_id)
        .copied()
        .ok_or_else(|| "This game is not running.".to_string())?;
    #[cfg(unix)]
    {
        steamtron_core::runtime::terminate_process_group(pid, false)
            .map_err(|error| error.to_string())?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < deadline {
            let running = managed_runs()
                .lock()
                .map_err(|_| "Game state lock failed.".to_string())?
                .contains_key(&game_id);
            if !running {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        steamtron_core::runtime::terminate_process_group(pid, true)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|error| format!("Could not stop the game: {error}"))?;
        if !status.success() {
            return Err(format!("taskkill exited with {status}"));
        }
        Ok(())
    }
}

#[tauri::command]
fn pick_managed_executable(game_id: String) -> Result<Option<String>, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(&game_id)
        .ok_or_else(|| format!("Unknown game {game_id}"))?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose the game executable")
        .set_directory(&game.installation.root)
        .pick_file()
    else {
        return Ok(None);
    };
    Ok(Some(path.display().to_string()))
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
    serde_json::json!({
        "path": library.path().display().to_string(),
        "settings": library.settings(),
        "drives": library.drives(),
        "games": library.games(),
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
    if let Some(gamemode) = gamemode {
        library.settings_mut().gamemode = gamemode;
    }
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!(library.settings()))
}

#[tauri::command]
fn add_library_drive() -> Result<Option<serde_json::Value>, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let Some(root) = rfd::FileDialog::new()
        .set_title("Choose a drive or folder for SteamTron games")
        .pick_folder()
    else {
        return Ok(None);
    };
    let label = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Drive")
        .to_owned();
    let mut library = Library::load().map_err(|error| error.to_string())?;
    library
        .add_drive(&root, label, library.drives().is_empty())
        .map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(Some(library_json(&library)))
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
    let umutron = steamtron_core::paths::layout()
        .data_home
        .join("umutron/proton-manager/runners");
    if let Ok(entries) = std::fs::read_dir(&umutron) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let runner = path.join("proton");
                if runner.is_file() {
                    push(runner, "umutron");
                }
            } else if path.is_file() {
                push(path, "umutron");
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
    for runner in &settings.proton_runners {
        if !detected.iter().any(|entry| entry["path"] == runner.as_str()) {
            detected.push(runner_entry(std::path::Path::new(runner), "custom"));
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
fn add_proton_runner() -> Result<Option<serde_json::Value>, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose a Proton runner")
        .pick_file()
    else {
        return Ok(None);
    };
    let runner = if path.join("proton").is_file() {
        path.join("proton")
    } else {
        path
    };
    if !runner.is_file() {
        return Err(format!("not a Proton runner: {}", runner.display()));
    }
    let value = runner.display().to_string();
    let mut library = Library::load().map_err(|error| error.to_string())?;
    {
        let settings = library.settings_mut();
        if !settings.proton_runners.contains(&value) {
            settings.proton_runners.push(value.clone());
        }
        if settings.default_proton.is_empty() {
            settings.default_proton = value;
        }
    }
    library.save().map_err(|error| error.to_string())?;
    Ok(Some(proton_runners_json()))
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
fn remove_proton_runner(path: String) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    require_auth()?;
    let mut library = Library::load().map_err(|error| error.to_string())?;
    {
        let settings = library.settings_mut();
        settings.proton_runners.retain(|runner| runner != &path);
        if settings.default_proton == path {
            settings.default_proton.clear();
        }
    }
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
    let value = serde_json::json!({
        "name": name,
        "release_date": release_date,
        "genres": genres,
        "description": description,
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
    let response = store_client()
        .get(format!("{base}{path}"))
        .query(query)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .await
        .map_err(|error| error.to_string())?;
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
async fn store_game(id: u64) -> Result<serde_json::Value, String> {
    store_get(&format!("/v1/games/{id}"), &[]).await
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn reconcile_registrations() -> Result<steamtron_library::managed::ReconcileReport, String> {
    let backend = platform_backend().map_err(|error| error.to_string())?;
    steamtron_library::managed::reconcile_registrations(&backend)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn add_managed_game(
    request: steamtron_library::managed::ManagedAddRequest,
) -> Result<steamtron_library::managed::ManagedAdded, String> {
    use steamtron_library::managed::ManifestProvider;

    if request.provider == ManifestProvider::Api {
        require_auth()?;
    }
    let backend = platform_backend().map_err(|error| error.to_string())?;
    let status = backend.status().map_err(|error| error.to_string())?;
    if !status.ready {
        return Err(format!(
            "Registration backend is not ready: {}. Open Settings → Doctor.",
            status.details
        ));
    }
    steamtron_library::managed::add_managed_game(request, &backend)
        .map_err(|error| error.to_string())
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
    Ok(serde_json::json!({
        "path": library.path().display().to_string(),
        "settings": library.settings(),
        "drives": library.drives(),
        "games": library.games(),
    }))
}

#[tauri::command]
fn library_import_preview(path: String) -> Result<serde_json::Value, String> {
    use std::path::PathBuf;

    let plan = steamtron_core::import::plan_path(PathBuf::from(&path))
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({ "path": path, "plan": plan }))
}

#[tauri::command]
fn library_import_commit(path: String) -> Result<serde_json::Value, String> {
    use std::path::PathBuf;
    use steamtron_core::library::Library;

    let plan = steamtron_core::import::plan_path(PathBuf::from(&path))
        .map_err(|error| error.to_string())?;
    let target = Library::default_path();
    let report = steamtron_core::import::apply_plan(&plan, &target)
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "library": target.display().to_string(),
        "report": report,
    }))
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

#[tauri::command]
fn library_migration_status() -> Result<serde_json::Value, String> {
    use std::path::PathBuf;
    use steamtron_core::library::Library;

    let library_path = Library::default_path();
    let library = Library::open(&library_path).map_err(|error| error.to_string())?;
    let mut sources = Vec::new();
    let layout = steamtron_core::paths::layout();
    let umutron = std::env::var_os("STEAMTRON_UMUTRON_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| layout.umutron_library());
    let legacy = layout.legacy_config();
    for (kind, path) in [("umutron", umutron), ("steamtron", legacy)] {
        if !path.is_file() {
            continue;
        }
        match steamtron_core::import::plan_path(&path) {
            Ok(plan) => sources.push(serde_json::json!({
                "kind": kind,
                "path": path.display().to_string(),
                "plan": plan,
            })),
            Err(error) => sources.push(serde_json::json!({
                "kind": kind,
                "path": path.display().to_string(),
                "error": error.to_string(),
            })),
        }
    }
    Ok(serde_json::json!({
        "library": {
            "path": library_path.display().to_string(),
            "games": library.games().len(),
        },
        "sources": sources,
    }))
}

fn main() {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    if let Some(code) = try_overlay_wrapper() {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(not(target_os = "linux"))]
            let _ = app;
            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.maximize();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backend_status,
            export_backup,
            restore_backup,
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
            set_enabled,
            apply_launch_option,
            remove_fixes,
            library_snapshot,
            add_managed_game,
            play_managed,
            reconcile_registrations,
            quit_app,
            managed_artwork,
            store_genres,
            store_cached_genres,
            store_cached_catalog,
            store_catalog,
            store_game,
            app_details,
            app_version,
            update_library_settings,
            add_library_drive,
            set_default_library_drive,
            diagnostics_summary,
            clear_diagnostics,
            proton_runners,
            add_proton_runner,
            set_default_proton_runner,
            remove_proton_runner,
            open_path,
            running_managed,
            stop_managed,
            managed_fixes,
            install_managed_fixes,
            remove_managed_fixes,
            pick_managed_executable,
            set_managed_launch_target,
            registration_verify,
            library_import_preview,
            library_import_commit,
            library_remember_picker,
            library_migration_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running SteamTron");
}
