use serde::Serialize;
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
#[cfg(target_os = "windows")]
const WINDOWS_REPAIR_SCRIPT: &str = include_str!("../scripts/windows-install-bettersteamtools.ps1");

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

    let steam_available = steam_root.is_some();
    let backend_install_available = steam_available;
    let backend = platform_backend().and_then(|backend| {
        let status = backend.status()?;
        Ok((backend, status))
    });

    match backend {
        Ok((_, status)) => {
            let detail = format!("{}: {}", status.backend, status.details);
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
            } else {
                format!("{error}.")
            };
            let mut item = check("unlock-backend", BACKEND_LABEL, CheckStatus::Error, detail);
            if backend_install_available {
                item.repair = Some("install-unlock-backend");
            }
            checks.push(item);
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

#[cfg(target_os = "windows")]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn launch_unlock_installer() -> Result<String, String> {
    let state_dir = steamtron_core::paths::layout().home();
    fs::create_dir_all(&state_dir).map_err(|error| error.to_string())?;

    #[cfg(target_os = "linux")]
    {
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
                "bash {} > {} 2>&1; status=$?; echo \"installer exit: $status\" >> {}; echo $status > {}; exit $status",
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
        let steam_root = steam::steam_root().map_err(|error| error.to_string())?;
        let script = state_dir.join("install-bettersteamtools.ps1");
        fs::write(&script, WINDOWS_REPAIR_SCRIPT).map_err(|error| error.to_string())?;
        let log = state_dir.join("installer.log");
        let marker = state_dir.join("installer.exit");
        let _ = fs::remove_file(&marker);
        let args = format!(
            "$ErrorActionPreference='Stop'; try {{ $argLine = '-NoProfile -ExecutionPolicy Bypass -File \"' + {script} + '\" -SteamRoot \"' + {root} + '\" -Log \"' + {log} + '\" -Marker \"' + {marker} + '\"'; $p = Start-Process -FilePath powershell.exe -Verb RunAs -Wait -PassThru -ArgumentList $argLine; if ($p.ExitCode -ne 0) {{ Set-Content -LiteralPath {marker} -Value $p.ExitCode -NoNewline }} }} catch {{ Add-Content -LiteralPath {log} -Value $_.Exception.Message; Set-Content -LiteralPath {marker} -Value 1 -NoNewline }}",
            script = ps_quote(&script.to_string_lossy()),
            root = ps_quote(&steam_root.to_string_lossy()),
            log = ps_quote(&log.to_string_lossy()),
            marker = ps_quote(&marker.to_string_lossy()),
        );
        hidden_command("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &args,
            ])
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok("BetterSteamTools installer opened.".into())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Err("Unlock backend installation is not supported on this platform.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_returns_only_the_two_support_checks() {
        let report = run();
        let ids: Vec<_> = report.checks.iter().map(|check| check.id).collect();
        assert_eq!(ids, vec!["steam-root", "unlock-backend"]);
    }

    #[test]
    fn check_status_is_serializable() {
        let value = check("x", "x", CheckStatus::Error, "missing");
        assert_eq!(value.status, CheckStatus::Error);
    }
}
