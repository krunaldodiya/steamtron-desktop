#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use steamtron_core::config::Config;
use steamtron_core::fixes::{export_name, export_zip, install_zip};
use steamtron_core::steam;
use steamtron_unlock::{platform_backend, UnlockBackend};
use steamtron_doctor::{run as run_doctor, launch_unlock_installer};
use steamtron_auth::{login as auth_login, logout as auth_logout, status as auth_status, AuthState};
use steamtron_library::{add_bundle, bundle_from_zip, download_bundle, normalize_appid, remove_game as remove_library_game, search as steam_search, AddedGame, RemovedGame, SearchResult};

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
}

fn require_auth() -> Result<(), String> {
    if auth_status().authenticated {
        Ok(())
    } else {
        Err("Sign in to SteamTron before using the desktop app.".into())
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
fn add_game(appid_or_url: String, title: String) -> Result<AddedGame, String> {
    require_auth()?;
    let appid = normalize_appid(&appid_or_url).ok_or_else(|| "Enter a numeric App ID or Steam store URL.".to_string())?;
    let backend = unlock_backend()?;
    let bundle = download_bundle(&appid).map_err(|error| error.to_string())?;
    add_bundle(&appid, &title, bundle, &backend).map_err(|error| error.to_string())
}

#[tauri::command]
fn import_game_zip(appid_or_url: String, title: String) -> Result<Option<AddedGame>, String> {
    require_auth()?;
    let appid = normalize_appid(&appid_or_url).ok_or_else(|| "Enter a numeric App ID or Steam store URL.".to_string())?;
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Lua manifest ZIP", &["zip"])
        .pick_file()
    else {
        return Ok(None);
    };
    let backend = unlock_backend()?;
    let bundle = bundle_from_zip(&path).map_err(|error| error.to_string())?;
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
    Ok(Some(FixView {
        appid: entry.appid,
        fixes: entry.fixes.display().to_string(),
        enabled: entry.enabled,
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
fn set_enabled(appid: String, enabled: bool) -> Result<(), String> {
    require_auth()?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    let Some(entry) = config.entry(&appid) else {
        return Err("No fixes configured for this game.".into());
    };
    let fixes = entry.fixes.clone();
    config
        .upsert(&appid, fixes, enabled)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn remove_fixes(appid: String) -> Result<(), String> {
    require_auth()?;
    let mut config = Config::load().map_err(|error| error.to_string())?;
    config.remove(&appid).map_err(|error| error.to_string())?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            backend_status,
            search_games,
            add_game,
            import_game_zip,
            remove_game,
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
            remove_fixes
        ])
        .run(tauri::generate_context!())
        .expect("error while running SteamTron");
}
