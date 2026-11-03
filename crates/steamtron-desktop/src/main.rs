#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use steamtron_core::config::Config;
use steamtron_core::fixes::{export_name, export_zip, install_zip};
use steamtron_core::steam;
use steamtron_unlock::{platform_backend, UnlockBackend};
use steamtron_doctor::{run as run_doctor, launch_unlock_installer};
use steamtron_auth::{login as auth_login, logout as auth_logout, status as auth_status, AuthState};

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
fn doctor_report() -> steamtron_doctor::DoctorReport {
    run_doctor()
}

#[tauri::command]
fn repair_backend() -> Result<String, String> {
    launch_unlock_installer()
}

#[tauri::command]
fn backend_status() -> Result<steamtron_unlock::BackendStatus, String> {
    platform_backend()
        .and_then(|backend| backend.status())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_games() -> Result<Vec<GameView>, String> {
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
    let mut config = Config::load().map_err(|error| error.to_string())?;
    config.remove(&appid).map_err(|error| error.to_string())?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            backend_status,
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
