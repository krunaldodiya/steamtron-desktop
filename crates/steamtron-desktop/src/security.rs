//! Opt-in, per-game Microsoft Defender exclusions.
//!
//! Nothing here disables Defender. The elevated scripts only add or remove
//! exclusion paths for the current game and SteamTron's overlay data.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use steamtron_core::config::Config;
use steamtron_core::process::hidden_command;
use steamtron_core::{data_home, steam};

const MAX_PROCESS_EXCLUSIONS: usize = 48;
const MAX_EXE_DEPTH: usize = 3;

#[derive(Clone, Debug, Serialize)]
pub struct SecurityPath {
    pub label: String,
    pub path: String,
    pub exists: bool,
    pub excluded: bool,
}

/// Record of the exclusions SteamTron added last time. Defender does not expose
/// the exclusion list to a normal (non-elevated) process, so this is how the
/// status view and the automatic path know the work was done.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct AppliedExclusions {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    processes: Vec<String>,
    #[serde(default)]
    applied_at: u64,
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
    let applied = read_marker(appid);
    if !supported() {
        return Ok(status_value(
            appid,
            paths,
            DefenderState::default(),
            Vec::new(),
            false,
            &applied,
        ));
    }
    if !cfg!(target_os = "windows") {
        return Ok(status_value(
            appid,
            paths,
            DefenderState::default(),
            Vec::new(),
            false,
            &applied,
        ));
    }
    let defender = read_defender().unwrap_or_default();
    let detections = read_detections(&paths);
    Ok(status_value(appid, paths, defender, detections, true, &applied))
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
    let processes = planned_processes(appid);
    if paths_to_add.is_empty() && processes.is_empty() {
        return Err("No existing SteamTron or game paths were found to exclude.".into());
    }
    run_elevated(&exclusion_script(&paths_to_add, &processes, false))?;
    write_marker(appid, &paths_to_add, &processes)?;
    status(appid)
}

/// Add the scoped exclusions automatically before an operation that writes or
/// runs fixes. No-op when the recorded exclusions already cover this game.
///
/// Defender hides the exclusion list from non-elevated processes, so the marker
/// written by [`apply`] is the source of truth for skipping the elevation.
pub fn ensure(appid: &str, include_game: bool) -> Result<(), String> {
    if !supported() {
        return Ok(());
    }
    validate_appid(appid)?;
    let paths = planned_paths(appid, include_game)?;
    let applied = read_marker(appid);
    let missing = paths
        .iter()
        .filter(|entry| entry.label != "Game folder" || (include_game && entry.exists))
        .any(|entry| !marker_covers_path(&applied, &entry.path));
    if !missing {
        return Ok(());
    }
    apply(appid, include_game).map(|_| ())
}

pub fn remove(appid: &str) -> Result<SecurityStatus, String> {
    validate_appid(appid)?;
    if !supported() {
        return Err("Windows Security exceptions are only available on Windows.".into());
    }
    let paths = planned_paths(appid, true)?;
    let applied = read_marker(appid);
    let mut paths_to_remove: Vec<PathBuf> = paths
        .iter()
        .filter(|entry| entry.excluded)
        .map(|entry| PathBuf::from(&entry.path))
        .collect();
    for path in &applied.paths {
        let path = PathBuf::from(path);
        if !paths_to_remove.contains(&path) {
            paths_to_remove.push(path);
        }
    }
    if paths_to_remove.is_empty() && applied.processes.is_empty() {
        clear_marker(appid);
        return status(appid);
    }
    run_elevated(&exclusion_script(&paths_to_remove, &applied.processes, true))?;
    clear_marker(appid);
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
    let library = library_game(appid);
    let legacy = Config::load()
        .ok()
        .and_then(|config| config.entry(appid).map(|entry| entry.fixes.clone()))
        .unwrap_or_else(|| base.join("fixes").join(appid));
    let managed = library
        .as_ref()
        .and_then(|game| steamtron_core::library::fixes_dir(game).ok());
    let mut paths: Vec<SecurityPath> = fixes_candidates(managed, legacy)
        .into_iter()
        .map(|(label, path)| SecurityPath {
            label: label.into(),
            exists: path.exists(),
            path: path.display().to_string(),
            excluded: false,
        })
        .collect();
    for (label, path) in [
        ("Overlay upper layer", base.join("upper").join(appid)),
        ("Mounted overlay", base.join("mount").join(appid)),
    ] {
        paths.push(SecurityPath {
            label: label.into(),
            exists: path.exists(),
            path: path.display().to_string(),
            excluded: false,
        });
    }
    if include_game {
        let game = steam::games()
            .ok()
            .and_then(|games| games.into_iter().find(|game| game.appid == appid))
            .map(|game| game.common)
            .or_else(|| library.as_ref().map(|game| game.installation.root.clone()));
        if let Some(game) = game.filter(|game| game.exists()) {
            paths.push(SecurityPath {
                label: "Game folder".into(),
                exists: true,
                path: game.display().to_string(),
                excluded: false,
            });
            // The real-path overlay keeps pristine content in a sibling base
            // folder and mounts the union at a sibling mount folder. Defender
            // resolves files served through that mount to these paths, so both
            // must be excluded or the fix executables are quarantined.
            if let (Some(parent), Some(name)) = (game.parent(), game.file_name()) {
                let base = parent.join(".steamtron-base").join(name);
                if base.is_dir() {
                    paths.push(SecurityPath {
                        label: "Game base".into(),
                        exists: true,
                        path: base.display().to_string(),
                        excluded: false,
                    });
                }
            }
        }
    }
    Ok(paths)
}

fn library_game(appid: &str) -> Option<steamtron_core::library::Game> {
    let id: u32 = appid.parse().ok()?;
    steamtron_core::library::Library::load()
        .ok()
        .and_then(|library| library.game_by_app_id(id).cloned())
}

/// Managed fixes replace the app-data folder once a game has a managed
/// footprint. Keep a legacy folder in the list when it still exists so old
/// fixes are covered until they are adopted.
fn fixes_candidates(managed: Option<PathBuf>, legacy: PathBuf) -> Vec<(&'static str, PathBuf)> {
    match managed {
        Some(managed) if managed != legacy => {
            let mut paths = vec![("SteamTron fixes", managed)];
            if legacy.is_dir() {
                paths.push(("Legacy fixes", legacy));
            }
            paths
        }
        _ => vec![("SteamTron fixes", legacy)],
    }
}

fn resolved_fixes(appid: &str) -> PathBuf {
    let base = data_home().join("steamtron");
    let legacy = Config::load()
        .ok()
        .and_then(|config| config.entry(appid).map(|entry| entry.fixes.clone()))
        .unwrap_or_else(|| base.join("fixes").join(appid));
    library_game(appid)
        .and_then(|game| steamtron_core::library::fixes_dir(&game).ok())
        .unwrap_or(legacy)
}

/// Defender resolves files opened through SteamTron's WinFsp overlay drive to
/// its volume device path, so folder and drive-letter exclusions do not match.
/// Process exclusions for the wrapper and the game executables do.
fn planned_processes(appid: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        push_process_name(&mut names, &executable);
    }
    if let Some(game) = library_game(appid) {
        for target in &game.launch_targets {
            push_process_name(&mut names, &target.executable);
        }
        collect_exe_names(&game.installation.root, 0, &mut names);
    }
    collect_exe_names(&resolved_fixes(appid), 0, &mut names);
    names
}

fn push_process_name(names: &mut Vec<String>, path: &Path) {
    if names.len() >= MAX_PROCESS_EXCLUSIONS {
        return;
    }
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return;
    };
    if name.is_empty()
        || names
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(name))
    {
        return;
    }
    names.push(name.to_owned());
}

fn collect_exe_names(directory: &Path, depth: usize, names: &mut Vec<String>) {
    if depth > MAX_EXE_DEPTH || names.len() >= MAX_PROCESS_EXCLUSIONS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_exe_names(&path, depth + 1, names);
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
        {
            push_process_name(names, &path);
        }
    }
}

fn marker_path(appid: &str) -> PathBuf {
    data_home()
        .join("steamtron")
        .join("security")
        .join(format!("{appid}.json"))
}

fn read_marker(appid: &str) -> AppliedExclusions {
    std::fs::read(marker_path(appid))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn marker_covers_path(applied: &AppliedExclusions, path: &str) -> bool {
    let path = normalize(path);
    applied.paths.iter().any(|excluded| {
        let excluded = normalize(excluded);
        path == excluded || path.starts_with(&format!("{excluded}\\"))
    })
}

fn write_marker(appid: &str, paths: &[PathBuf], processes: &[String]) -> Result<(), String> {
    let marker = AppliedExclusions {
        paths: paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        processes: processes.to_vec(),
        applied_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    let path = marker_path(appid);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&marker).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("{}: {error}", path.display()))
}

fn clear_marker(appid: &str) {
    let _ = std::fs::remove_file(marker_path(appid));
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
    applied: &AppliedExclusions,
) -> SecurityStatus {
    let exclusions: Vec<String> = defender
        .exclusions
        .iter()
        .map(|value| normalize(value))
        .collect();
    for entry in &mut paths {
        entry.excluded = marker_covers_path(applied, &entry.path)
            || exclusions.iter().any(|excluded| {
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

fn exclusion_script(paths: &[PathBuf], processes: &[String], remove: bool) -> String {
    let mut script = String::from("$ErrorActionPreference = 'Stop'\ntry {\n");
    let command = if remove {
        "Remove-MpPreference"
    } else {
        "Add-MpPreference"
    };
    for path in paths {
        script.push_str(&format!(
            "  {command} -ExclusionPath '{}' -ErrorAction Stop\n",
            ps_quote(&path.display().to_string())
        ));
    }
    for process in processes {
        script.push_str(&format!(
            "  {command} -ExclusionProcess '{}' -ErrorAction Stop\n",
            ps_quote(process)
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
    let output = hidden_command("powershell.exe")
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
        let processes = vec!["PlayGTAV.exe".to_owned()];
        let add = exclusion_script(&paths, &processes, false);
        let remove = exclusion_script(&paths, &processes, true);
        assert!(add.contains("Add-MpPreference"));
        assert!(add.contains("-ExclusionProcess 'PlayGTAV.exe'"));
        assert!(!add.contains("Set-MpPreference"));
        assert!(!add.contains("DisableRealtimeMonitoring"));
        assert!(remove.contains("Remove-MpPreference"));
        assert!(remove.contains("-ExclusionProcess 'PlayGTAV.exe'"));
        assert!(!remove.contains("Set-MpPreference"));
    }

    #[test]
    fn managed_fixes_replace_the_legacy_candidate() {
        let managed = PathBuf::from(r"D:\SteamTronLibrary\steamtronapps\fixes\Game");
        let legacy = PathBuf::from(r"C:\Users\Test\AppData\Local\steamtron\fixes\42");
        let candidates = fixes_candidates(Some(managed.clone()), legacy.clone());
        assert_eq!(candidates[0].0, "SteamTron fixes");
        assert_eq!(candidates[0].1, managed);
        assert_eq!(candidates.len(), 1, "a missing legacy folder is not listed");
        let fallback = fixes_candidates(None, legacy.clone());
        assert_eq!(fallback[0].1, legacy);
    }

    #[test]
    fn marker_covers_children_only_under_the_excluded_root() {
        let applied = AppliedExclusions {
            paths: vec![r"D:\SteamTronLibrary\steamtronapps\fixes".into()],
            ..Default::default()
        };
        assert!(marker_covers_path(
            &applied,
            r"D:\SteamTronLibrary\steamtronapps\fixes\Game\file.dll"
        ));
        assert!(!marker_covers_path(
            &applied,
            r"D:\SteamTronLibrary\steamtronapps\common\Game\file.exe"
        ));
    }

    #[test]
    fn process_names_are_deduped_case_insensitively() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("Game.exe"), b"").unwrap();
        std::fs::write(temp.path().join("Launcher.exe"), b"").unwrap();
        std::fs::create_dir_all(temp.path().join("Binaries")).unwrap();
        std::fs::write(temp.path().join("Binaries").join("game.EXE"), b"").unwrap();
        let mut names = Vec::new();
        collect_exe_names(temp.path(), 0, &mut names);
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|name| name == "Game.exe"));
        assert!(names.iter().any(|name| name == "Launcher.exe"));
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
