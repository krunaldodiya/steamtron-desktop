use std::collections::BTreeSet;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::process::Command;

use serde::Serialize;
use steamtron_core::config::Config;
use steamtron_core::steam;
use steamtron_register::{platform_backend, RegistrationBackend};

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
        let running = steam_running();
        checks.push(check(
            "steam-process",
            "Steam process",
            if running { CheckStatus::Ok } else { CheckStatus::Warning },
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
            if lua_dir.is_dir() { CheckStatus::Ok } else { CheckStatus::Warning },
            lua_dir.display().to_string(),
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

    #[cfg(target_os = "windows")]
    {
        let winfsp = winfsp_installed();
        checks.push(check(
            "overlay-tools",
            "WinFsp overlay runtime",
            if winfsp { CheckStatus::Ok } else { CheckStatus::Error },
            if winfsp {
                "WinFsp is installed."
            } else {
                "Install WinFsp before launching games through the fixes overlay."
            },
        ));
    }

    let healthy = checks.iter().all(|item| item.status != CheckStatus::Error);
    DoctorReport { healthy, checks }
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
        if environ
            .split(|byte| *byte == 0)
            .any(|arg| arg.starts_with(b"LD_AUDIT=") && arg.windows(8).any(|window| window == b"SLSsteam"))
        {
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
    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq steam.exe", "/NH"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_ascii_lowercase().contains("steam.exe"))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn winfsp_installed() -> bool {
    let mut candidates = Vec::new();
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(value) = std::env::var_os(variable) {
            candidates.push(std::path::PathBuf::from(value).join("WinFsp/bin/winfsp-x64.dll"));
        }
    }
    for key in ["HKLM\\SOFTWARE\\WOW6432Node\\WinFsp", "HKLM\\SOFTWARE\\WinFsp"] {
        if let Ok(output) = Command::new("reg")
            .args(["query", key, "/v", "InstallDir"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Some(value) = text
                    .lines()
                    .find(|line| line.contains("InstallDir"))
                    .and_then(|line| line.split_once("REG_SZ"))
                    .map(|(_, value)| value.trim().trim_matches('"'))
                {
                    candidates.push(std::path::PathBuf::from(value).join("bin/winfsp-x64.dll"));
                }
            }
        }
    }
    candidates.into_iter().any(|path| path.is_file())
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
            let state_dir = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".local/share/steamtron");
            let log = state_dir.join("installer.log");
            let marker = state_dir.join("installer.exit");
            let _ = fs::remove_file(&marker);
            let command = format!(
                "bash {} --noplugin --nolaunch 2>&1 | tee {}; status=${{PIPESTATUS[0]}}; echo \"installer exit: $status\" >> {}; echo $status > {}; exit $status",
                shell_quote(&script),
                shell_quote(&log),
                shell_quote(&log),
                shell_quote(&marker)
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
