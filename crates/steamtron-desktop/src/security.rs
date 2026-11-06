//! Opt-in, per-game Microsoft Defender exclusions.
//!
//! Nothing here disables Defender. The elevated scripts only add or remove
//! exclusion paths for the current game and SteamTron's overlay data.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use steamtron_core::config::Config;
use steamtron_core::{data_home, steam};

#[derive(Clone, Debug, Serialize)]
pub struct SecurityPath {
    pub label: String,
    pub path: String,
    pub exists: bool,
    pub excluded: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SecurityDetection {
    pub threat: String,
    pub action: String,
    pub resources: Vec<String>,
    pub when: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SecurityStatus {
    pub supported: bool,
    pub defender: bool,
    pub realtime: bool,
    pub appid: String,
    pub paths: Vec<SecurityPath>,
    pub missing: usize,
    pub detections: Vec<SecurityDetection>,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
struct DefenderState {
    defender: bool,
    realtime: bool,
    exclusions: Vec<String>,
}

pub fn supported() -> bool {
    cfg!(target_os = "windows")
}

pub fn status(appid: &str) -> Result<SecurityStatus, String> {
    validate_appid(appid)?;
    let paths = planned_paths(appid, true)?;
    if !supported() {
        return Ok(status_value(
            appid,
            paths,
            DefenderState::default(),
            Vec::new(),
            false,
        ));
    }
    if !cfg!(target_os = "windows") {
        return Ok(status_value(
            appid,
            paths,
            DefenderState::default(),
            Vec::new(),
            false,
        ));
    }
    let defender = read_defender().unwrap_or_default();
    let detections = read_detections(&paths);
    Ok(status_value(appid, paths, defender, detections, true))
}

pub fn require_exception(appid: &str) -> Result<(), String> {
    if !supported() {
        return Ok(());
    }
    let current = status(appid)?;
    if !current.defender || current.realtime == false || current.missing == 0 {
        return Ok(());
    }
    Err("Add the Windows Security exception for this game before applying fixes. The exception is scoped to SteamTron's fixes, upper overlay, and mounted overlay paths; it does not disable antivirus.".into())
}

pub fn apply(appid: &str, include_game: bool) -> Result<SecurityStatus, String> {
    validate_appid(appid)?;
    if !supported() {
        return Err("Windows Security exceptions are only available on Windows.".into());
    }
    let paths = planned_paths(appid, include_game)?;
    create_required_dirs(&paths)?;
    let paths_to_add: Vec<PathBuf> = paths
        .iter()
        .filter(|entry| entry.exists && entry.label != "Game folder")
        .map(|entry| PathBuf::from(&entry.path))
        .chain(
            paths
                .iter()
                .filter(|entry| entry.label == "Game folder" && include_game && entry.exists)
                .map(|entry| PathBuf::from(&entry.path)),
        )
        .collect();
    if paths_to_add.is_empty() {
        return Err("No existing SteamTron or game paths were found to exclude.".into());
    }
    run_elevated(&exclusion_script(&paths_to_add, false))?;
    let current = status(appid)?;
    if current.missing > 0 {
        return Err(format!(
            "Windows Security did not apply {} exclusion path(s). Real-time protection or tamper protection may be blocking the change.",
            current.missing
        ));
    }
    Ok(current)
}

pub fn remove(appid: &str) -> Result<SecurityStatus, String> {
    validate_appid(appid)?;
    if !supported() {
        return Err("Windows Security exceptions are only available on Windows.".into());
    }
    let paths = planned_paths(appid, true)?;
    let paths_to_remove: Vec<PathBuf> = paths
        .iter()
        .filter(|entry| entry.excluded)
        .map(|entry| PathBuf::from(&entry.path))
        .collect();
    if paths_to_remove.is_empty() {
        return status(appid);
    }
    run_elevated(&exclusion_script(&paths_to_remove, true))?;
    status(appid)
}

pub fn repair(appid: &str) -> Result<SecurityStatus, String> {
    apply(appid, true)
}

fn validate_appid(appid: &str) -> Result<(), String> {
    if appid.is_empty() || !appid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Invalid Steam App ID.".into());
    }
    Ok(())
}

fn planned_paths(appid: &str, include_game: bool) -> Result<Vec<SecurityPath>, String> {
    let base = data_home().join("steamtron");
    let config = Config::load().ok();
    let fixes = config
        .as_ref()
        .and_then(|config| config.entry(appid))
        .map(|entry| entry.fixes.clone())
        .unwrap_or_else(|| base.join("fixes").join(appid));
    let upper = base.join("upper").join(appid);
    let mount = base.join("mount").join(appid);
    let game = steam::games()
        .ok()
        .and_then(|games| games.into_iter().find(|game| game.appid == appid))
        .map(|game| game.common);

    let mut paths = vec![
        SecurityPath {
            label: "SteamTron fixes".into(),
            path: fixes.display().to_string(),
            exists: fixes.exists(),
            excluded: false,
        },
        SecurityPath {
            label: "Overlay upper layer".into(),
            path: upper.display().to_string(),
            exists: upper.exists(),
            excluded: false,
        },
        SecurityPath {
            label: "Mounted overlay".into(),
            path: mount.display().to_string(),
            exists: mount.exists(),
            excluded: false,
        },
    ];
    if include_game {
        if let Some(game) = game.filter(|game| game.exists()) {
            paths.push(SecurityPath {
                label: "Game folder".into(),
                path: game.display().to_string(),
                exists: true,
                excluded: false,
            });
        }
    }
    Ok(paths)
}

fn create_required_dirs(paths: &[SecurityPath]) -> Result<(), String> {
    for entry in paths.iter().filter(|entry| entry.label != "Game folder") {
        let path = Path::new(&entry.path);
        std::fs::create_dir_all(path)
            .map_err(|error| format!("Could not prepare {}: {error}", path.display()))?;
    }
    Ok(())
}

fn status_value(
    appid: &str,
    mut paths: Vec<SecurityPath>,
    defender: DefenderState,
    detections: Vec<SecurityDetection>,
    supported_here: bool,
) -> SecurityStatus {
    let exclusions: Vec<String> = defender
        .exclusions
        .iter()
        .map(|value| normalize(value))
        .collect();
    for entry in &mut paths {
        entry.excluded = exclusions.iter().any(|excluded| {
            let path = normalize(&entry.path);
            path == *excluded || path.starts_with(&format!("{excluded}\\"))
        });
    }
    let missing = paths.iter().filter(|entry| !entry.excluded).count();
    let message = if !supported_here {
        "Windows Security exceptions are only available on Windows.".into()
    } else if !defender.defender {
        "Microsoft Defender is not available. Third-party antivirus exclusions must be configured manually.".into()
    } else if missing == 0 {
        "Windows Security exceptions are active for this game.".into()
    } else {
        format!(
            "{missing} Windows Security exception path(s) are missing. Add the exception before applying fixes."
        )
    };
    SecurityStatus {
        supported: supported_here,
        defender: defender.defender,
        realtime: defender.realtime,
        appid: appid.to_owned(),
        paths,
        missing,
        detections,
        message,
    }
}

fn normalize(value: &str) -> String {
    let value = value.trim().trim_matches('"');
    let value = value
        .strip_prefix("file:_")
        .or_else(|| value.strip_prefix("containerfile:_"))
        .or_else(|| value.strip_prefix("file:"))
        .unwrap_or(value);
    value
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn read_defender() -> Result<DefenderState, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
$status = Get-MpComputerStatus
$pref = Get-MpPreference
[pscustomobject]@{
  defender = [bool]$status.AntivirusEnabled
  realtime = [bool]$status.RealTimeProtectionEnabled
  exclusions = @($pref.ExclusionPath)
} | ConvertTo-Json -Compress
"#;
    let output = run_powershell(script)?;
    let value: serde_json::Value =
        serde_json::from_str(output.trim_start_matches('\u{feff}').trim())
            .map_err(|error| format!("Could not read Microsoft Defender status: {error}"))?;
    Ok(DefenderState {
        defender: value
            .get("defender")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        realtime: value
            .get("realtime")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        exclusions: value
            .get("exclusions")
            .map(|value| match value {
                serde_json::Value::Array(values) => values
                    .iter()
                    .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                    .collect(),
                serde_json::Value::String(value) => vec![value.clone()],
                _ => Vec::new(),
            })
            .unwrap_or_default(),
    })
}

fn read_detections(paths: &[SecurityPath]) -> Vec<SecurityDetection> {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
Get-MpThreatDetection | Select-Object ThreatName,ActionSuccess,Resources,InitialDetectionTime | ConvertTo-Json -Compress
"#;
    let Ok(output) = run_powershell(script) else {
        return Vec::new();
    };
    let Ok(value) =
        serde_json::from_str::<serde_json::Value>(output.trim_start_matches('\u{feff}').trim())
    else {
        return Vec::new();
    };
    let values = match value {
        serde_json::Value::Array(values) => values,
        serde_json::Value::Object(_) => vec![value],
        _ => return Vec::new(),
    };
    let roots: Vec<String> = paths.iter().map(|entry| normalize(&entry.path)).collect();
    values
        .into_iter()
        .filter_map(|value| {
            let resources: Vec<String> = match value.get("Resources") {
                Some(serde_json::Value::String(value)) => vec![value.clone()],
                Some(serde_json::Value::Array(items)) => items
                    .iter()
                    .flat_map(|item| match item {
                        serde_json::Value::String(value) => vec![value.clone()],
                        serde_json::Value::Array(values) => values
                            .iter()
                            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                            .collect(),
                        _ => Vec::new(),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let related = resources.iter().any(|resource| {
                let resource = normalize(resource);
                roots
                    .iter()
                    .any(|root| resource == *root || resource.starts_with(&format!("{root}\\")))
            });
            if !related {
                return None;
            }
            Some(SecurityDetection {
                threat: value
                    .get("ThreatName")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Unknown")
                    .to_owned(),
                action: value
                    .get("ActionSuccess")
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                resources,
                when: value
                    .get("InitialDetectionTime")
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn exclusion_script(paths: &[PathBuf], remove: bool) -> String {
    let mut script = String::from("$ErrorActionPreference = 'Stop'\ntry {\n");
    for path in paths {
        let command = if remove {
            "Remove-MpPreference"
        } else {
            "Add-MpPreference"
        };
        script.push_str(&format!(
            "  {command} -ExclusionPath '{}' -ErrorAction Stop\n",
            ps_quote(&path.display().to_string())
        ));
    }
    script.push_str("  'OK'\n} catch {\n  $_.Exception.Message\n  exit 1\n}\n");
    script
}

fn run_elevated(script: &str) -> Result<(), String> {
    let directory = std::env::temp_dir().join(format!(
        "steamtron-security-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create the security helper directory: {error}"))?;
    let script_path = directory.join("apply.ps1");
    let result_path = directory.join("result.txt");
    let wrapped = format!(
        "try {{\n{}\n  'OK' | Set-Content -LiteralPath '{}' -Encoding UTF8\n}} catch {{\n  $_.Exception.Message | Set-Content -LiteralPath '{}' -Encoding UTF8\n  exit 1\n}}\n",
        script,
        ps_quote(&result_path.display().to_string()),
        ps_quote(&result_path.display().to_string())
    );
    std::fs::write(&script_path, wrapped)
        .map_err(|error| format!("Could not write the security helper: {error}"))?;
    let outer = format!(
        "$p = Start-Process -FilePath 'powershell.exe' -Verb RunAs -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{}') -PassThru -Wait; exit $p.ExitCode",
        ps_quote(&script_path.display().to_string())
    );
    let output = run_powershell(&outer);
    let message = std::fs::read_to_string(&result_path).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&directory);
    match output {
        Ok(_) => Ok(()),
        Err(error) => Err(if message.trim().is_empty() {
            format!("Windows Security change was not approved or failed: {error}")
        } else {
            message.trim().to_owned()
        }),
    }
}

fn run_powershell(script: &str) -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("PowerShell is only available on Windows.".into());
    }
    let script =
        format!("$OutputEncoding = [Console]::OutputEncoding = [Text.Encoding]::UTF8\n{script}");
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|error| format!("Could not run PowerShell: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if detail.is_empty() {
            format!("PowerShell exited with {}", output.status)
        } else {
            detail
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn ps_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusion_scripts_never_disable_protection() {
        let paths = vec![PathBuf::from(r"C:\Users\Test\AppData\Local\steamtron\fixes\42")];
        let add = exclusion_script(&paths, false);
        let remove = exclusion_script(&paths, true);
        assert!(add.contains("Add-MpPreference"));
        assert!(!add.contains("Set-MpPreference"));
        assert!(!add.contains("DisableRealtimeMonitoring"));
        assert!(remove.contains("Remove-MpPreference"));
        assert!(!remove.contains("Set-MpPreference"));
    }

    #[test]
    fn powershell_single_quotes_are_escaped() {
        assert_eq!(ps_quote(r"C:\Games\John's Game"), r"C:\Games\John''s Game");
    }

    #[test]
    fn detection_resources_are_normalized() {
        assert_eq!(
            normalize(r#"file:_C:\Users\Test\AppData\Local\steamtron\fixes\42"#),
            r"c:\users\test\appdata\local\steamtron\fixes\42"
        );
    }
}
