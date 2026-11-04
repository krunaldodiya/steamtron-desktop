use std::path::Path;
use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

use serde::Serialize;
use steamtron_core::config::Config;
use steamtron_core::steam;
use steamtron_unlock::{platform_backend, UnlockBackend};

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

fn check(id: &'static str, title: &'static str, status: CheckStatus, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        id,
        title,
        status,
        detail: detail.into(),
        repair: None,
    }
}

fn with_repair(mut value: DoctorCheck, repair: &'static str) -> DoctorCheck {
    value.repair = Some(repair);
    value
}

pub fn run() -> DoctorReport {
    let mut checks = Vec::new();
    let steam_root = match steam::steam_root() {
        Ok(root) => {
            checks.push(check("steam-root", "Steam installation", CheckStatus::Ok, root.display().to_string()));
            Some(root)
        }
        Err(error) => {
            checks.push(check("steam-root", "Steam installation", CheckStatus::Error, error.to_string()));
            None
        }
    };

    if let Some(root) = &steam_root {
        checks.push(check(
            "steam-process",
            "Steam process",
            if steam_running() { CheckStatus::Ok } else { CheckStatus::Warning },
            if steam_running() {
                "Steam is running.".to_string()
            } else {
                "Steam is not running; restart it after repairs.".to_string()
            },
        ));
        checks.push(check(
            "stplug-in",
            "Lua manifest directory",
            if root.join("config/stplug-in").is_dir() { CheckStatus::Ok } else { CheckStatus::Warning },
            root.join("config/stplug-in").display().to_string(),
        ));
    }

    let backend = platform_backend().and_then(|backend| {
        let status = backend.status()?;
        Ok((backend, status))
    });
    let mut backend_ready = false;
    match backend {
        Ok((_, status)) => {
            backend_ready = status.ready;
            checks.push(check(
                "unlock-backend",
                "Steam library backend",
                if status.ready { CheckStatus::Ok } else { CheckStatus::Error },
                format!("{}: {}", status.backend, status.details),
            ));
            if !status.ready {
                if let Some(last) = checks.last_mut() {
                    last.repair = Some("install-unlock-backend");
                }
            }
        }
        Err(error) => {
            checks.push(with_repair(
                check("unlock-backend", "Steam library backend", CheckStatus::Error, error.to_string()),
                "install-unlock-backend",
            ));
        }
    }

    let managed = steam::managed_appids().unwrap_or_default();
    let games = steam::games().unwrap_or_default();
    let installed_appids: BTreeSet<&str> = games.iter().map(|game| game.appid.as_str()).collect();
    if !managed.is_empty() && !backend_ready {
        checks.push(with_repair(
            check(
                "managed-ownership",
                "Managed game ownership",
                CheckStatus::Error,
                format!(
                    "{} managed Lua entries exist but no unlock backend is active. Steam can show Purchase for installed games.",
                    managed.len()
                ),
            ),
            "install-unlock-backend",
        ));
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

    let config = Config::load();
    match config {
        Ok(config) => {
            for entry in config.entries() {
                let status = if entry.fixes.is_dir() { CheckStatus::Ok } else { CheckStatus::Error };
                let item = check(
                    "fixes-path",
                    "Fixes path",
                    status,
                    format!("{}: {}", entry.appid, entry.fixes.display()),
                );
                checks.push(item);
            }
            if config.entries().next().is_none() {
                checks.push(check("fixes-config", "Fixes configuration", CheckStatus::Ok, "No fixes configured."));
            }
        }
        Err(error) => checks.push(check("fixes-config", "Fixes configuration", CheckStatus::Error, error.to_string())),
    }

    #[cfg(target_os = "linux")]
    {
        let overlay_ok = which("fuse-overlayfs").is_some()
            && (which("fusermount3").is_some() || which("fusermount").is_some());
        checks.push(check(
            "overlay-tools",
            "Linux overlay tools",
            if overlay_ok { CheckStatus::Ok } else { CheckStatus::Error },
            if overlay_ok {
                "fuse-overlayfs and fusermount are available."
            } else {
                "Install fuse-overlayfs and fuse3."
            },
        ));
    }

    let healthy = checks.iter().all(|item| item.status != CheckStatus::Error);
    DoctorReport { healthy, checks }
}

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

#[cfg(target_os = "linux")]
fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// Launch the platform helper installer. The caller must present this as a
/// user-consented repair, never as an automatic background action.
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

pub fn launch_unlock_installer() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        let candidates = [
            std::env::var_os("STEAMTRON_SLSSTEAM_INSTALLER").map(std::path::PathBuf::from),
            std::env::var_os("HOME").map(|home| {
                std::path::PathBuf::from(home)
                    .join("Downloads/SLSteam-moon/install-slsteam-moon.sh")
            }),
        ];
        let Some(source_script) = candidates.into_iter().flatten().find(|path| path.is_file()) else {
            return Err("SLSsteam installer was not found. Install SLSsteam/Lumen and refresh Doctor.".into());
        };
        let source = fs::read_to_string(&source_script).map_err(|error| error.to_string())?;
        let filtered = source
            .lines()
            .map(|line| {
                if line.trim() == "install_lumen" {
                    "# install_lumen skipped by SteamTron"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let script = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".local/share/steamtron/install-slsteam-only.sh");
        fs::create_dir_all(script.parent().unwrap_or_else(|| std::path::Path::new(".")))
            .map_err(|error| error.to_string())?;
        fs::write(&script, filtered).map_err(|error| error.to_string())?;
        let terminals = [
            ("konsole", vec!["-e"]),
            ("x-terminal-emulator", vec!["-e"]),
            ("gnome-terminal", vec!["--"]),
        ];
        for (terminal, arguments) in terminals {
            if which(terminal).is_none() {
                continue;
            }
            let log = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".local/share/steamtron/installer.log");
            let command = format!(
                "bash {} --noplugin --nolaunch 2>&1 | tee {}; status=${{PIPESTATUS[0]}}; echo \"installer exit: $status\" >> {}; exit $status",
                shell_quote(&script),
                shell_quote(&log),
                shell_quote(&log)
            );
            Command::new(terminal)
                .args(arguments)
                .arg("bash")
                .arg("-c")
                .arg(command)
                .spawn()
                .map_err(|error| error.to_string())?;
            return Ok("SLSsteam installer opened.".into());
        }
        Err("No terminal emulator was found to run the SLSsteam installer.".into())
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
