#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use steamtron_core::config::Config;
use steamtron_core::launch::{clear_managed_launch_option, has_launch_option, launch_option_value, set_launch_option};
use steamtron_core::fixes::{export_name, export_zip, install_zip};
use steamtron_core::steam;
use steamtron_unlock::{platform_backend, UnlockBackend};
use steamtron_doctor::{run as run_doctor, launch_unlock_installer};
use steamtron_auth::{login as auth_login, logout as auth_logout, status as auth_status, AuthState};
use steamtron_library::updates::{available_updates, clear_pin, pin_update, Updates};
use steamtron_library::{add_bundle, bundle_from_file, download_bundle, normalize_appid, remove_game as remove_library_game, repair_missing_appmanifests, search as steam_search, AddedGame, RemovedGame, RepairedGame, SearchResult};

#[derive(Serialize)]
struct GameView {
    appid: String,
    name: String,
    build_id: String,
    installed: bool,
    version: Option<String>,
    common: String,
    manifest: String,
    managed: bool,
}

#[derive(Serialize)]
struct FixView {
    appid: String,
    fixes: String,
    enabled: bool,
    launch_option_set: bool,
    launch_option_error: Option<String>,
}

fn require_auth() -> Result<(), String> {
    if auth_status().authenticated {
        Ok(())
    } else {
        Err("Sign in to SteamTron before using the desktop app.".into())
    }
}

fn configure_launch_option(appid: &str, enabled: bool) -> Result<bool, String> {
    if !enabled {
        clear_managed_launch_option(appid).map_err(|error| error.to_string())?;
        return Ok(false);
    }
    let config = Config::load().map_err(|error| error.to_string())?;
    let entry = config.entry(appid).ok_or_else(|| "No fixes configured for this game.".to_string())?;
    let game = steam::games()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|game| game.appid == appid)
        .ok_or_else(|| "This game is not installed in a Steam library.".to_string())?;
    let exe = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| "Could not resolve the SteamTron executable path.".to_string())?;
    let value = launch_option_value(&exe, appid, &game.common, &entry.fixes);
    set_launch_option(appid, Some(&value)).map_err(|error| error.to_string())?;
    Ok(true)
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
                .map(|status| status.installed)
                .unwrap_or(false)
        };
        stop_steam()?;
        let repair_result = (|| -> Result<String, String> {
            if !installed() {
                launch_unlock_installer()?;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
                while !installed() {
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
                "SLSsteam is installed; Lumen was removed. Recreated {} appmanifest{}.",
                repaired.len(),
                if repaired.len() == 1 { "" } else { "s" }
            ))
        })();
        let start_result = start_steam();
        match (repair_result, start_result) {
            (Ok(message), Ok(())) => Ok(format!("{message} Steam was restarted.")),
            (Err(error), Ok(())) => Err(format!("{error} Steam was restarted.")),
            (Ok(message), Err(error)) => Err(format!("{message} Steam could not be started: {error}")),
            (Err(error), Err(start_error)) => Err(format!("{error} Steam could not be started: {start_error}")),
        }
    })
    .await
    .map_err(|error| format!("Repair task failed: {error}"))?
}

fn unlock_backend() -> Result<impl UnlockBackend, String> {
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
    let config = Config::load().map_err(|error| error.to_string())?;
    let mut repaired = Vec::new();
    let mut errors = Vec::new();
    for entry in config.entries().filter(|entry| entry.enabled && !entry.launch_option_disabled) {
        if has_launch_option(&entry.appid) {
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
    Ok(())
}

#[cfg(target_os = "linux")]
fn restart_steam_impl() -> Result<String, String> {
    stop_steam()?;
    start_steam()?;
    Ok("Steam restarted.".into())
}

#[cfg(not(target_os = "linux"))]
fn restart_steam_impl() -> Result<String, String> {
    Err("Steam restart is only implemented on Linux.".into())
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
fn backend_status() -> Result<steamtron_unlock::BackendStatus, String> {
    require_auth()?;
    platform_backend()
        .and_then(|backend| backend.status())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_games() -> Result<Vec<GameView>, String> {
    require_auth()?;
    let managed = steam::managed_appids().map_err(|error| error.to_string())?;
    let games = steam::games().map_err(|error| error.to_string())?;
    Ok(games
        .into_iter()
        .map(|game| {
            let version = if game.build_id == "0" {
                available_updates(&game.appid)
                    .ok()
                    .map(|updates| updates.latest)
                    .filter(|latest| !latest.is_empty())
            } else {
                None
            };
            GameView {
                managed: managed.contains(&game.appid),
                appid: game.appid,
                name: game.name,
                build_id: game.build_id.clone(),
                installed: game.state_flags & 4 != 0 && game.size_on_disk > 0 && game.build_id != "0",
                version,
                common: game.common.display().to_string(),
                manifest: game.manifest.display().to_string(),
            }
        })
        .collect())
}

#[tauri::command]
fn list_fixes() -> Result<Vec<FixView>, String> {
    require_auth()?;
    let config = Config::load().map_err(|error| error.to_string())?;
    Ok(config
        .entries()
        .map(|entry| FixView {
            appid: entry.appid.clone(),
            fixes: entry.fixes.display().to_string(),
            enabled: entry.enabled,
            launch_option_set: has_launch_option(&entry.appid),
            launch_option_error: None,
        })
        .collect())
}

#[tauri::command]
fn install_fixes(appid: String) -> Result<Option<FixView>, String> {
    require_auth()?;
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
    Ok(Some(FixView {
        appid: entry.appid.clone(),
        fixes: entry.fixes.display().to_string(),
        enabled: entry.enabled,
        launch_option_set: has_launch_option(&entry.appid),
        launch_option_error: launch.err(),
    }))
}

#[tauri::command]
fn use_fixes_folder(appid: String) -> Result<Option<FixView>, String> {
    require_auth()?;
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
    Ok(Some(FixView {
        appid: entry.appid.clone(),
        fixes: entry.fixes.display().to_string(),
        enabled: entry.enabled,
        launch_option_set: has_launch_option(&entry.appid),
        launch_option_error: launch.err(),
    }))
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
    match configure_launch_option(&appid, enabled) {
        Ok(true) => Ok("Fixes enabled and Steam launch option set. Restart Steam before launching.".into()),
        Ok(false) => Ok("Fixes disabled and Steam launch option cleared.".into()),
        Err(error) => Err(format!("Fixes state saved, but the Steam launch option failed: {error}. Restart Steam and retry.")),
    }
}

#[tauri::command]
fn apply_launch_option(appid: String, enabled: bool) -> Result<String, String> {
    require_auth()?;
    configure_launch_option(&appid, enabled)?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    config
        .set_launch_option_disabled(&appid, !enabled)
        .map_err(|error| error.to_string())?;
    Ok(if enabled {
        "Steam launch option set. Restart Steam before launching.".into()
    } else {
        "Steam launch option cleared. Restart Steam before launching.".into()
    })
}

#[tauri::command]
fn remove_fixes(appid: String) -> Result<(), String> {
    require_auth()?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    config.remove(&appid).map_err(|error| error.to_string())?;
    clear_managed_launch_option(&appid).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn try_overlay_wrapper() -> Option<i32> {
    use steamtron_overlay::{LinuxOverlay, OverlayGame};
    use std::process::Command;

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--steamtron-overlay") {
        return None;
    }
    let mut appid = None;
    let mut common = None;
    let mut fixes = None;
    let mut command = Vec::new();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--appid" => { appid = args.get(index + 1).cloned(); index += 2; }
            "--common" => { common = args.get(index + 1).cloned(); index += 2; }
            "--fixes" => { fixes = args.get(index + 1).cloned(); index += 2; }
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
        let game = OverlayGame::new(common, fixes, Some(appid))
            .map_err(|error| error.to_string())?;
        let mut overlay = LinuxOverlay::new(game);
        let merged = overlay.mount().map_err(|error| error.to_string())?.to_path_buf();
        let rewritten: Vec<String> = command.iter().map(|value| overlay.game().rewrite_arg(value, &merged)).collect();
        let mut process = Command::new(&rewritten[0]);
        process.args(&rewritten[1..]).current_dir(&merged);
        if let Some(appid) = overlay.game().appid() {
            process.env("SteamAppId", appid);
            process.env("SteamGameId", appid);
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

fn main() {
    #[cfg(target_os = "linux")]
    if let Some(code) = try_overlay_wrapper() {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            backend_status,
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
            profile_status,
            profile_login,
            profile_logout,
            doctor_report,
            repair_steam_library,
            list_games,
            list_fixes,
            install_fixes,
            use_fixes_folder,
            open_fixes_folder,
            export_fixes,
            set_enabled,
            apply_launch_option,
            remove_fixes
        ])
        .run(tauri::generate_context!())
        .expect("error while running SteamTron");
}
