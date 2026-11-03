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
use steamtron_library::{add_bundle, bundle_from_file, download_bundle, normalize_appid, remove_game as remove_library_game, search as steam_search, AddedGame, RemovedGame, SearchResult};

#[derive(Serialize)]
struct GameView {
    appid: String,
    name: String,
    build_id: String,
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
fn repair_backend() -> Result<String, String> {
    require_auth()?;
    launch_unlock_installer()
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
fn search_games(query: String) -> Result<Vec<SearchResult>, String> {
    require_auth()?;
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

#[tauri::command]
fn restart_steam() -> Result<String, String> {
    require_auth()?;
    restart_steam_impl()
}

#[cfg(target_os = "linux")]
fn restart_steam_impl() -> Result<String, String> {
    use std::process::{Command, Stdio};
    use std::time::Duration;

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
    std::thread::sleep(Duration::from_secs(3));
    Command::new("setsid")
        .arg("steam")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start Steam: {error}"))?;
    Ok("Steam restart requested.".into())
}

#[cfg(not(target_os = "linux"))]
fn restart_steam_impl() -> Result<String, String> {
    Err("Steam restart is only implemented on Linux.".into())
}

#[tauri::command]
fn list_libraries() -> Result<Vec<String>, String> {
    require_auth()?;
    steam::libraries()
        .map(|libraries| libraries.into_iter().map(|path| path.display().to_string()).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn add_game(appid_or_url: String, title: String, library: Option<String>) -> Result<AddedGame, String> {
    require_auth()?;
    let appid = normalize_appid(&appid_or_url).ok_or_else(|| "Enter a numeric App ID or Steam store URL.".to_string())?;
    let backend = unlock_backend()?;
    let bundle = download_bundle(&appid).map_err(|error| error.to_string())?;
    add_bundle(&appid, &title, bundle, &backend, library.as_deref().map(std::path::Path::new)).map_err(|error| error.to_string())
}

#[tauri::command]
fn import_game_file(appid_or_url: String, title: String, library: Option<String>) -> Result<Option<AddedGame>, String> {
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
    add_bundle(&appid, &title, bundle, &backend, library.as_deref().map(std::path::Path::new))
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
        .map(|game| GameView {
            managed: managed.contains(&game.appid),
            appid: game.appid,
            name: game.name,
            build_id: game.build_id,
            common: game.common.display().to_string(),
            manifest: game.manifest.display().to_string(),
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
fn export_fixes(appid: String) -> Result<Option<String>, String> {
    require_auth()?;
    let config = Config::load().map_err(|error| error.to_string())?;
    let Some(entry) = config.entry(&appid) else {
        return Err("No fixes configured for this game.".into());
    };
    let name = export_name(&appid, &appid);
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
            list_libraries,
            search_games,
            add_game,
            import_game_file,
            remove_game,
            update_versions,
            update_pin,
            clear_update_pin,
            restart_steam,
            profile_status,
            profile_login,
            profile_logout,
            doctor_report,
            repair_backend,
            list_games,
            list_fixes,
            install_fixes,
            export_fixes,
            set_enabled,
            apply_launch_option,
            remove_fixes
        ])
        .run(tauri::generate_context!())
        .expect("error while running SteamTron");
}
