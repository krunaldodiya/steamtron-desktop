use serde::Serialize;
use std::collections::BTreeSet;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "linux")]
use std::path::Path;
use steamtron_core::process::hidden_command;
use steamtron_core::steam;
use steamtron_register::{is_elf64, platform_backend, RegistrationBackend};

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorCheck {
    pub id: &'static str,
    pub title: &'static str,
    pub status: CheckStatus,
    pub detail: String,
    pub repair: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorReport {
    pub healthy: bool,
    pub checks: Vec<DoctorCheck>,
}

fn check(
    id: &'static str,
    title: &'static str,
    status: CheckStatus,
    detail: impl Into<String>,
) -> DoctorCheck {
    DoctorCheck {
        id,
        title,
        status,
        detail: detail.into(),
        repair: None,
    }
}

#[cfg(target_os = "linux")]
const BACKEND_LABEL: &str = "SLSsteam Moon";
#[cfg(target_os = "windows")]
const BACKEND_LABEL: &str = "BetterSteamTools";
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
const BACKEND_LABEL: &str = "Steam registration backend";

#[cfg(target_os = "linux")]
const LINUX_REPAIR_SCRIPT: &str = include_str!("../scripts/linux-repair-slsteam.sh");

pub fn run() -> DoctorReport {
    let mut checks = Vec::new();
    let steam_root = match steam::steam_root() {
        Ok(root) => {
            checks.push(check(
                "steam-root",
                "Steam installation",
                CheckStatus::Ok,
                root.display().to_string(),
            ));
            Some(root)
        }
        Err(error) => {
            checks.push(check(
                "steam-root",
                "Steam installation",
                CheckStatus::Error,
                error.to_string(),
            ));
            None
        }
    };

    if let Some(root) = &steam_root {
        let running = steam_running();
        checks.push(check(
            "steam-process",
            "Steam process",
            if running {
                CheckStatus::Ok
            } else {
                CheckStatus::Warning
            },
            if running {
                "Steam is running.".to_string()
            } else {
                "Steam is not running; start it manually after repairs.".to_string()
            },
        ));
        #[cfg(target_os = "linux")]
        {
            let injected = running && steam_injected();
            checks.push(check(
                "steam-injection",
                "SLSsteam injection",
                if !running {
                    CheckStatus::Warning
                } else if injected {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Warning
                },
                if !running {
                    "Steam is not running; start it through the SLSsteam wrapper.".to_string()
                } else if injected {
                    "Steam is running with SLSsteam LD_AUDIT injection.".to_string()
                } else {
                    "Steam is running without SLSsteam injection. Exit Steam and start it through the SLSsteam wrapper.".to_string()
                },
            ));
        }
        let lua_dir = if root.join("config/stplug-in").is_dir() {
            root.join("config/stplug-in")
        } else {
            root.join("config/lua")
        };
        checks.push(check(
            "stplug-in",
            "Lua manifest directory",
            if lua_dir.is_dir() {
                CheckStatus::Ok
            } else {
                CheckStatus::Warning
            },
            lua_dir.display().to_string(),
        ));
    }

    let steam_available = steam_root.is_some();
    // Windows BetterSteamTools installation is not implemented yet. Do not
    // offer a repair action that cannot run. Linux repair also requires an
    // existing Steam installation.
    let backend_install_available = steam_available && cfg!(target_os = "linux");

    let backend = platform_backend().and_then(|backend| {
        let status = backend.status()?;
        Ok((backend, status))
    });
    let mut backend_ready = false;
    match backend {
        Ok((_, status)) => {
            backend_ready = status.ready;
            let mut detail = format!("{}: {}", status.backend, status.details);
            if !status.ready && !cfg!(target_os = "linux") {
                detail.push_str(
                    " Automatic BetterSteamTools installation is not available in this build.",
                );
            }
            let mut item = check(
                "unlock-backend",
                BACKEND_LABEL,
                if status.ready {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Error
                },
                detail,
            );
            if !status.ready && backend_install_available {
                item.repair = Some("install-unlock-backend");
            }
            checks.push(item);
        }
        Err(error) => {
            let detail = if !steam_available {
                format!("{error}. Install Steam first; SteamTron does not install Steam.")
            } else if !cfg!(target_os = "linux") {
                format!(
                    "{error}. Automatic {BACKEND_LABEL} installation is not available in this build."
                )
            } else {
                error.to_string()
            };
            let mut item = check("unlock-backend", BACKEND_LABEL, CheckStatus::Error, detail);
            if backend_install_available {
                item.repair = Some("install-unlock-backend");
            }
            checks.push(item);
        }
    }

    let managed = steam::managed_appids().unwrap_or_default();
    let games = steam::games().unwrap_or_default();
    let installed_appids: BTreeSet<&str> = games.iter().map(|game| game.appid.as_str()).collect();
    if !managed.is_empty() && !backend_ready {
        let mut item = check(
            "managed-ownership",
            "Managed game ownership",
            CheckStatus::Error,
            format!(
                "{} managed Lua entries exist but {} is not active. Steam can show Purchase for installed games.",
                managed.len(),
                BACKEND_LABEL
            ),
        );
        if backend_install_available {
            item.repair = Some("install-unlock-backend");
        }
        checks.push(item);
    }
    for appid in &managed {
        if !installed_appids.contains(appid.as_str()) {
            checks.push(check(
                "managed-appmanifest",
                "Managed appmanifest",
                CheckStatus::Warning,
                format!("Lua entry {appid} has no installed appmanifest; the game may not appear as installed."),
            ));
        }
    }

    let healthy = checks.iter().all(|item| item.status == CheckStatus::Ok);
    DoctorReport { healthy, checks }
}

#[cfg(target_os = "linux")]
fn audit_has_elf64_slssteam(value: &[u8]) -> bool {
    let Ok(value) = std::str::from_utf8(value) else {
        return false;
    };
    value.split(':').any(|entry| {
        entry.contains("SLSsteam") && Path::new(entry).is_file() && is_elf64(Path::new(entry))
    })
}

#[cfg(target_os = "linux")]
pub fn steam_injected() -> bool {
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let is_steam = cmdline
            .split(|byte| *byte == 0)
            .any(|arg| arg == b"steam" || arg.ends_with(b"/steam"));
        if !is_steam {
            continue;
        }
        let Ok(environ) = fs::read(entry.path().join("environ")) else {
            continue;
        };
        if environ.split(|byte| *byte == 0).any(|arg| {
            arg.strip_prefix(b"LD_AUDIT=")
                .is_some_and(audit_has_elf64_slssteam)
        }) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "linux")]
fn steam_running() -> bool {
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
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
fn steam_running() -> bool {
    hidden_command("tasklist")
        .args(["/FI", "IMAGENAME eq steam.exe", "/NH"])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains("steam.exe")
        })
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// Launch the platform helper installer. The caller must present this as a
/// user-consented repair, never as an automatic background action.
#[cfg(target_os = "linux")]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

pub fn launch_unlock_installer() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        let state_dir = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".local/share/steamtron");
        fs::create_dir_all(&state_dir).map_err(|error| error.to_string())?;
        let script = state_dir.join("repair-slsteam.sh");
        fs::write(&script, LINUX_REPAIR_SCRIPT).map_err(|error| error.to_string())?;
        let mut permissions = fs::metadata(&script)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&script, permissions).map_err(|error| error.to_string())?;

        let terminals = [
            ("konsole", vec!["-e"]),
            ("x-terminal-emulator", vec!["-e"]),
            ("gnome-terminal", vec!["--"]),
        ];
        for (terminal, arguments) in terminals {
            if which(terminal).is_none() {
                continue;
            }
            let log = state_dir.join("installer.log");
            let marker = state_dir.join("installer.exit");
            let _ = fs::remove_file(&marker);
            let command = format!(
                "bash {} 2>&1 | tee {}; status=${{PIPESTATUS[0]}}; echo \"installer exit: $status\" >> {}; echo $status > {}; exit $status",
                shell_quote(&script),
                shell_quote(&log),
                shell_quote(&log),
                shell_quote(&marker)
            );
            hidden_command(terminal)
                .args(arguments)
                .arg("bash")
                .arg("-c")
                .arg(command)
                .spawn()
                .map_err(|error| error.to_string())?;
            return Ok("SLSsteam Moon repair opened.".into());
        }
        Err("No terminal emulator was found to run the SLSsteam Moon repair.".into())
    }
    #[cfg(target_os = "windows")]
    {
        Err("Windows helper installation is not implemented yet.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_returns_checks() {
        let report = run();
        assert!(!report.checks.is_empty());
    }

    #[test]
    fn check_status_is_serializable() {
        let value = check("x", "x", CheckStatus::Error, "missing");
        assert_eq!(value.status, CheckStatus::Error);
    }
}
