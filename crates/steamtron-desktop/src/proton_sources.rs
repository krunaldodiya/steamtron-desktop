//! Official Proton release discovery and verified background installation.
//!
//! GE-Proton, UMU-Proton and Proton-CachyOS come from their GitHub releases.
//! Valve Proton is discovered from the Steam libraries. Installs run in a
//! detached child process and publish progress through a state file, so they
//! survive dialog navigation and the main window being hidden or closed.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use xz2::read::XzDecoder;

const REPOS: &[(&str, &str)] = &[
    ("GE-Proton", "GloriousEggroll/proton-ge-custom"),
    ("UMU-Proton", "Open-Wine-Components/umu-proton"),
    ("Proton-CachyOS", "CachyOS/proton-cachyos"),
];
const MAX_ARCHIVE: u64 = 2 * 1024 * 1024 * 1024;
const FAMILIES: &[&str] = &["GE-Proton", "UMU-Proton", "Proton-CachyOS", "Valve-Proton"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRelease {
    pub family: String,
    pub tag: String,
    pub name: String,
    pub url: String,
    pub size: u64,
    pub installed: bool,
    pub installed_path: String,
    pub installable: bool,
    pub removable: bool,
    #[serde(default)]
    pub steam_app_id: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallState {
    pub family: String,
    pub tag: String,
    pub name: String,
    pub status: String,
    pub percent: f64,
    pub completed: u64,
    pub total: u64,
    pub message: String,
    pub pid: u32,
    pub updated_at: u64,
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| error.to_string())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub fn runner_root() -> PathBuf {
    steamtron_core::paths::layout().runners()
}

fn archive_stem(name: &str) -> &str {
    name.strip_suffix(".tar.gz")
        .or_else(|| name.strip_suffix(".tar.xz"))
        .unwrap_or(name)
}

fn release_dir(family: &str, _tag: &str, name: &str) -> PathBuf {
    runner_root().join(format!("{}-{}", sanitize(family), sanitize(archive_stem(name))))
}

pub fn state_path(family: &str, tag: &str, name: &str) -> PathBuf {
    steamtron_core::paths::layout()
        .cache_dir()
        .join("proton-install")
        .join(format!(
            "{}-{}-{}.json",
            sanitize(family),
            sanitize(tag),
            sanitize(archive_stem(name))
        ))
}

fn write_state(path: &Path, state: &InstallState) {
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if let Ok(payload) = serde_json::to_vec(state) {
        if fs::write(&temporary, payload).is_ok() && fs::rename(&temporary, path).is_err() {
            let _ = fs::remove_file(&temporary);
        }
    }
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        pid > 0 && Path::new("/proc").join(pid.to_string()).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        pid > 0
    }
}

pub fn read_states() -> Vec<InstallState> {
    let directory = steamtron_core::paths::layout()
        .cache_dir()
        .join("proton-install");
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut states = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(mut state) = serde_json::from_slice::<InstallState>(&bytes) else {
            continue;
        };
        if is_active(&state.status) && !pid_alive(state.pid) {
            state.status = "error".into();
            state.message = "Installation stopped before it finished.".into();
            state.updated_at = now();
            write_state(&path, &state);
        }
        states.push(state);
    }
    states
}

fn is_active(status: &str) -> bool {
    matches!(status, "starting" | "downloading" | "extracting" | "registering")
}

fn emit(
    path: Option<&Path>,
    family: &str,
    tag: &str,
    name: &str,
    status: &str,
    percent: f64,
    completed: u64,
    total: u64,
    message: &str,
) {
    if let Some(path) = path {
        write_state(
            path,
            &InstallState {
                family: family.to_owned(),
                tag: tag.to_owned(),
                name: name.to_owned(),
                status: status.to_owned(),
                percent: percent.clamp(0.0, 100.0),
                completed,
                total,
                message: message.to_owned(),
                pid: std::process::id(),
                updated_at: now(),
            },
        );
    }
}

pub fn run_install(family: &str, tag: &str, name: &str, resume: bool) -> i32 {
    let path = state_path(family, tag, name);
    emit(Some(&path), family, tag, name, "starting", 0.0, 0, 0, "Starting…");
    match install_with_progress(family, tag, name, Some(&path), resume) {
        Ok(_) => {
            emit(Some(&path), family, tag, name, "done", 100.0, 0, 0, "Installed");
            0
        }
        Err(error) => {
            emit(Some(&path), family, tag, name, "error", 0.0, 0, 0, &error);
            1
        }
    }
}

pub fn install_with_progress(
    family: &str,
    tag: &str,
    name: &str,
    state: Option<&Path>,
    resume: bool,
) -> Result<serde_json::Value, String> {
    let releases = list(family)?;
    migrate_legacy(family, &releases);
    let release = releases
        .into_iter()
        .find(|release| release.tag == tag && release.name == name && release.installable)
        .ok_or_else(|| "this Proton release is not available from its official source".to_string())?;
    let destination = release_dir(family, tag, name);
    let runner = destination.join("proton");
    if runner.is_file() {
        register(&runner)?;
        return Ok(serde_json::json!({ "path": runner.display().to_string(), "installed": true }));
    }
    if release.size == 0 || release.size > MAX_ARCHIVE {
        return Err("runner archive size is invalid".into());
    }
    let root = runner_root();
    let work = root.join(".downloads");
    fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    let stem = sanitize(archive_stem(name));
    let partial = work.join(format!("{stem}.part"));
    let staging = work.join(format!("{stem}.extract"));
    let _ = fs::remove_dir_all(&staging);
    let result = (|| -> Result<(), String> {
        let existing = if resume {
            fs::metadata(&partial).map(|metadata| metadata.len()).unwrap_or(0)
        } else {
            0
        };
        let mut request = client()?
            .get(&release.url)
            .header("User-Agent", "steamtron-desktop");
        if existing > 0 {
            request = request.header("Range", format!("bytes={existing}-"));
        }
        let mut response = request.send().map_err(|error| error.to_string())?;
        if !response.status().is_success() && response.status().as_u16() != 206 {
            return Err(format!("runner download returned HTTP {}", response.status()));
        }
        let append = existing > 0 && response.status().as_u16() == 206;
        let mut total = if append { existing } else { 0 };
        let mut sha256 = Sha256::new();
        let mut sha512 = Sha512::new();
        let mut file = if append {
            let mut source = fs::File::open(&partial).map_err(|error| error.to_string())?;
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                let read = source.read(&mut buffer).map_err(|error| error.to_string())?;
                if read == 0 {
                    break;
                }
                sha256.update(&buffer[..read]);
                sha512.update(&buffer[..read]);
            }
            fs::OpenOptions::new()
                .append(true)
                .open(&partial)
                .map_err(|error| error.to_string())?
        } else {
            fs::File::create(&partial).map_err(|error| error.to_string())?
        };
        emit(state, family, tag, name, "downloading", (total as f64 / release.size as f64 * 90.0).min(90.0), total, release.size, if append { "Resuming…" } else { "Downloading…" });
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let read = response.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > MAX_ARCHIVE {
                return Err("runner archive is too large".into());
            }
            sha256.update(&buffer[..read]);
            sha512.update(&buffer[..read]);
            file.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
            let percent = (total as f64 / release.size as f64 * 90.0).min(90.0);
            emit(state, family, tag, name, "downloading", percent, total, release.size, "Downloading…");
        }
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        let repo = REPOS
            .iter()
            .find(|(name, _)| *name == family)
            .map(|(_, repo)| *repo)
            .unwrap_or("");
        let payload: serde_json::Value = client()?
            .get(format!("https://api.github.com/repos/{repo}/releases/tags/{tag}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "steamtron-desktop")
            .send()
            .map_err(|error| error.to_string())?
            .json()
            .map_err(|error| error.to_string())?;
        let assets = payload
            .get("assets")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let asset = assets
            .iter()
            .find(|asset| asset.get("name").and_then(|value| value.as_str()) == Some(name))
            .ok_or_else(|| "runner asset disappeared from its official release".to_string())?;
        let digest = asset.get("digest").and_then(|value| value.as_str()).unwrap_or("");
        let expected_sha256 = digest.strip_prefix("sha256:").map(str::to_ascii_lowercase);
        let stem = name
            .strip_suffix(".tar.gz")
            .or_else(|| name.strip_suffix(".tar.xz"))
            .unwrap_or(name);
        let checksum_url = assets
            .iter()
            .find(|candidate| {
                candidate.get("name").and_then(|value| value.as_str())
                    == Some(format!("{stem}.sha512sum").as_str())
            })
            .and_then(|candidate| candidate.get("browser_download_url"))
            .and_then(|value| value.as_str())
            .map(str::to_owned);
        if let Some(expected) = expected_sha256 {
            let actual = format!("{:x}", sha256.finalize());
            if actual != expected {
                return Err("runner archive SHA-256 does not match the published digest".into());
            }
        } else {
            let url = checksum_url.ok_or_else(|| "no published checksum available".to_string())?;
            let expected = checksum(&url)?;
            let actual = format!("{:x}", sha512.finalize());
            if actual != expected {
                return Err("runner archive SHA-512 does not match the published checksum".into());
            }
        }
        emit(state, family, tag, name, "extracting", 92.0, total, release.size, "Extracting…");
        fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
        let file = fs::File::open(&partial).map_err(|error| error.to_string())?;
        if name.ends_with(".tar.xz") {
            tar::Archive::new(XzDecoder::new(file))
                .unpack(&staging)
                .map_err(|error| format!("runner archive is invalid: {error}"))?;
        } else {
            tar::Archive::new(GzDecoder::new(file))
                .unpack(&staging)
                .map_err(|error| format!("runner archive is invalid: {error}"))?;
        }
        let found = find_proton(&staging, 0)
            .ok_or_else(|| "runner archive contains no proton executable".to_string())?;
        let extracted = found
            .parent()
            .ok_or_else(|| "runner archive layout is invalid".to_string())?;
        if destination.exists() {
            fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
        }
        if extracted == staging {
            fs::rename(&staging, &destination).map_err(|error| error.to_string())?;
        } else {
            fs::rename(extracted, &destination).map_err(|error| error.to_string())?;
        }
        if !destination.join("proton").is_file() {
            return Err("runner archive did not install a proton executable".into());
        }
        emit(state, family, tag, name, "registering", 98.0, total, release.size, "Registering…");
        register(&destination.join("proton"))?;
        Ok(())
    })();
    let _ = fs::remove_file(&partial);
    let _ = fs::remove_dir_all(&staging);
    result?;
    Ok(serde_json::json!({ "path": runner.display().to_string(), "installed": true }))
}

fn find_proton(root: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }
    let candidate = root.join("proton");
    if candidate.is_file() {
        return Some(candidate);
    }
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_proton(&path, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

fn checksum(url: &str) -> Result<String, String> {
    let response = client()?
        .get(url)
        .header("User-Agent", "steamtron-desktop")
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("checksum returned HTTP {}", response.status()));
    }
    let text = response.text().map_err(|error| error.to_string())?;
    let first = text.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    if first.len() != 128 || !first.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("published SHA-512 checksum is invalid".into());
    }
    Ok(first)
}

fn wait_for_exit(pid: u32) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while pid_alive(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn pause(family: &str, tag: &str, name: &str) -> Result<(), String> {
    let path = state_path(family, tag, name);
    let state = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<InstallState>(&bytes).ok())
        .ok_or_else(|| "this Proton install is not running".to_string())?;
    if !is_active(&state.status) {
        return Err("this Proton install is not running".into());
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &state.pid.to_string()])
            .status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &state.pid.to_string(), "/F"])
            .status();
    }
    wait_for_exit(state.pid);
    let stem = sanitize(archive_stem(name));
    let completed = fs::metadata(runner_root().join(".downloads").join(format!("{stem}.part")))
        .map(|metadata| metadata.len())
        .unwrap_or(state.completed);
    let total = state.total.max(completed);
    let mut state = state;
    state.status = "paused".into();
    state.percent = if total > 0 {
        (completed as f64 / total as f64 * 90.0).min(90.0)
    } else {
        0.0
    };
    state.completed = completed;
    state.total = total;
    state.message = "Paused".into();
    state.updated_at = now();
    write_state(&path, &state);
    Ok(())
}

pub fn cancel(family: &str, tag: &str, name: &str) -> Result<(), String> {
    let path = state_path(family, tag, name);
    let state = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<InstallState>(&bytes).ok())
        .ok_or_else(|| "this Proton install is not running".to_string())?;
    if !is_active(&state.status) && state.status != "paused" {
        return Err("this Proton install is not running".into());
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &state.pid.to_string()])
            .status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &state.pid.to_string(), "/F"])
            .status();
    }
    wait_for_exit(state.pid);
    let work = runner_root().join(".downloads");
    let stem = sanitize(archive_stem(name));
    let _ = fs::remove_file(work.join(format!("{stem}.part")));
    let _ = fs::remove_dir_all(work.join(format!("{stem}.extract")));
    let mut state = state;
    state.status = "cancelled".into();
    state.message = "Cancelled".into();
    state.updated_at = now();
    write_state(&path, &state);
    Ok(())
}

pub fn uninstall(installed_path: &str) -> Result<(), String> {
    use steamtron_core::library::Library;

    let runner = Path::new(installed_path);
    if !runner.is_absolute() {
        return Err("invalid Proton runner path".into());
    }
    let root = runner_root()
        .canonicalize()
        .map_err(|error| format!("{}: {error}", runner_root().display()))?;
    let runner = runner
        .canonicalize()
        .map_err(|error| format!("{}: {error}", runner.display()))?;
    if !runner.starts_with(&root) || runner == root {
        return Err("Steam-managed Proton runners cannot be uninstalled here.".into());
    }
    let directory = runner
        .parent()
        .filter(|parent| *parent != root.as_path())
        .ok_or_else(|| "invalid Proton runner folder".to_string())?
        .to_path_buf();
    if directory.exists() {
        fs::remove_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    }
    let prefix = directory.display().to_string();
    let mut library = Library::load().map_err(|error| error.to_string())?;
    {
        let settings = library.settings_mut();
        settings.proton_runners.retain(|entry| !entry.starts_with(&prefix));
        if settings.default_proton == installed_path || settings.default_proton.starts_with(&prefix) {
            settings.default_proton.clear();
        }
    }
    library.save().map_err(|error| error.to_string())
}

fn register(runner: &Path) -> Result<(), String> {
    use steamtron_core::library::Library;

    let value = runner.display().to_string();
    let mut library = Library::load().map_err(|error| error.to_string())?;
    {
        let settings = library.settings_mut();
        if !settings.proton_runners.contains(&value) {
            settings.proton_runners.push(value.clone());
        }
        if settings.default_proton.is_empty() {
            settings.default_proton = value;
        }
    }
    library.save().map_err(|error| error.to_string())
}

fn source_cache_path(family: &str) -> PathBuf {
    steamtron_core::paths::layout()
        .cache_dir()
        .join("proton-sources")
        .join(format!("{}.json", sanitize(family)))
}

/// Move a runner installed under the old `<family>-<tag>` layout onto its
/// archive-specific folder, using the old install state to pick the variant.
fn migrate_legacy(family: &str, releases: &[SourceRelease]) {
    let mut tags = std::collections::HashSet::new();
    for release in releases {
        if !tags.insert(release.tag.clone()) {
            continue;
        }
        let legacy = runner_root().join(format!("{}-{}", sanitize(family), sanitize(&release.tag)));
        if !legacy.join("proton").is_file() {
            continue;
        }
        let old_state = steamtron_core::paths::layout()
            .cache_dir()
            .join("proton-install")
            .join(format!("{}-{}.json", sanitize(family), sanitize(&release.tag)));
        let installed_name = fs::read(&old_state)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<InstallState>(&bytes).ok())
            .map(|state| state.name);
        let chosen = installed_name
            .and_then(|name| {
                releases
                    .iter()
                    .find(|item| item.tag == release.tag && item.name == name)
            })
            .or_else(|| releases.iter().find(|item| item.tag == release.tag));
        let Some(chosen) = chosen else { continue };
        let destination = release_dir(family, &chosen.tag, &chosen.name);
        if !destination.exists() {
            let _ = fs::rename(&legacy, &destination);
        }
    }
}

fn cached_release(mut release: SourceRelease) -> SourceRelease {
    let runner = release_dir(&release.family, &release.tag, &release.name).join("proton");
    release.installed = runner.is_file();
    release.installed_path = runner.display().to_string();
    release.removable = true;
    release
}

/// Read the persistent source cache, fetching only when missing or forced.
pub fn list_cached(family: &str, force: bool) -> Result<Vec<SourceRelease>, String> {
    if family == "Valve-Proton" {
        return valve_releases();
    }
    let path = source_cache_path(family);
    if !force {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(releases) = serde_json::from_slice::<Vec<SourceRelease>>(&bytes) {
                migrate_legacy(family, &releases);
                return Ok(releases.into_iter().map(cached_release).collect());
            }
        }
    }
    let previous = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<SourceRelease>>(&bytes).ok())
        .unwrap_or_default();
    let fetched = list(family)?;
    migrate_legacy(family, &fetched);
    let mut merged = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for release in fetched {
        if seen.insert((release.tag.clone(), release.name.clone())) {
            merged.push(cached_release(release));
        }
    }
    // Keep locally installed releases that upstream has dropped, so they stay
    // visible and can still be uninstalled; everything else is replaced by the
    // refreshed list.
    for release in previous {
        if seen.contains(&(release.tag.clone(), release.name.clone())) {
            continue;
        }
        let release = cached_release(release);
        if release.installed {
            seen.insert((release.tag.clone(), release.name.clone()));
            merged.push(release);
        }
    }
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_ok() {
            if let Ok(payload) = serde_json::to_vec(&merged) {
                let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
                if fs::write(&temporary, payload).is_ok() && fs::rename(&temporary, &path).is_err() {
                    let _ = fs::remove_file(&temporary);
                }
            }
        }
    }
    Ok(merged)
}

pub fn list(family: &str) -> Result<Vec<SourceRelease>, String> {
    if !FAMILIES.contains(&family) {
        return Err(format!("unknown Proton source: {family}"));
    }
    if family == "Valve-Proton" {
        return valve_releases();
    }
    let (_, repo) = REPOS
        .iter()
        .find(|(name, _)| *name == family)
        .ok_or_else(|| format!("unknown Proton source: {family}"))?;
    let response = client()?
        .get(format!("https://api.github.com/repos/{repo}/releases?per_page=60"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub releases returned HTTP {}", response.status()));
    }
    let payload: serde_json::Value = response.json().map_err(|error| error.to_string())?;
    let releases = payload
        .as_array()
        .ok_or_else(|| "invalid GitHub release response".to_string())?;
    let mut result = Vec::new();
    for release in releases {
        if release.get("draft").and_then(|value| value.as_bool()).unwrap_or(false)
            || release.get("prerelease").and_then(|value| value.as_bool()).unwrap_or(false)
        {
            continue;
        }
        let tag = release.get("tag_name").and_then(|value| value.as_str()).unwrap_or("");
        if tag.is_empty() {
            continue;
        }
        let assets = release.get("assets").and_then(|value| value.as_array());
        let Some(assets) = assets else { continue };
        for asset in assets {
            let name = asset.get("name").and_then(|value| value.as_str()).unwrap_or("");
            let lower = name.to_ascii_lowercase();
            if !(lower.ends_with(".tar.gz") || lower.ends_with(".tar.xz")) {
                continue;
            }
            if lower.contains("aarch64") || lower.contains("arm64") {
                continue;
            }
            let url = asset
                .get("browser_download_url")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let expected = format!("https://github.com/{repo}/releases/download/{tag}/");
            if !url.starts_with(&expected) {
                continue;
            }
            let destination = release_dir(family, tag, name);
            let runner = destination.join("proton");
            result.push(SourceRelease {
                family: family.to_owned(),
                tag: tag.to_owned(),
                name: name.to_owned(),
                url: url.to_owned(),
                size: asset.get("size").and_then(|value| value.as_u64()).unwrap_or(0),
                installed: runner.is_file(),
                installed_path: runner.display().to_string(),
                installable: true,
                removable: true,
                steam_app_id: None,
            });
        }
    }
    if result.is_empty() {
        return Err("No x86_64 runner archives were found for this source.".into());
    }
    Ok(result)
}

fn valve_releases() -> Result<Vec<SourceRelease>, String> {
    let mut installed: Vec<(String, PathBuf)> = Vec::new();
    let libraries = steamtron_core::steam::libraries().map_err(|error| error.to_string())?;
    for library in libraries {
        let common = library.join("common");
        let Ok(entries) = fs::read_dir(&common) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let runner = path.join("proton");
            if !runner.is_file() {
                continue;
            }
            installed.push((entry.file_name().to_string_lossy().into_owned(), runner));
        }
    }
    let mut used = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    let channels: [(&str, u32, &[&str]); 3] = [
        ("Proton Experimental", 1493710, &["experimental"]),
        ("Proton Hotfix", 2180100, &["hotfix"]),
        ("Proton 11.0", 4628710, &["proton 11", "proton - 11", "proton_11"]),
    ];
    for (label, app_id, needles) in channels {
        let found = installed.iter().enumerate().find(|(index, (name, _))| {
            !used.contains(index)
                && needles.iter().any(|needle| name.to_ascii_lowercase().contains(needle))
        });
        if let Some((index, (name, runner))) = found {
            used.insert(index);
            result.push(SourceRelease {
                family: "Valve-Proton".into(),
                tag: label.into(),
                name: name.clone(),
                url: String::new(),
                size: 0,
                installed: true,
                installed_path: runner.display().to_string(),
                installable: false,
                removable: false,
                steam_app_id: Some(app_id),
            });
        } else {
            result.push(SourceRelease {
                family: "Valve-Proton".into(),
                tag: label.into(),
                name: label.into(),
                url: String::new(),
                size: 0,
                installed: false,
                installed_path: String::new(),
                installable: false,
                removable: false,
                steam_app_id: Some(app_id),
            });
        }
    }
    for (index, (name, runner)) in installed.into_iter().enumerate() {
        if used.contains(&index) {
            continue;
        }
        result.push(SourceRelease {
            family: "Valve-Proton".into(),
            tag: name.clone(),
            name,
            url: String::new(),
            size: 0,
            installed: true,
            installed_path: runner.display().to_string(),
            installable: false,
            removable: false,
            steam_app_id: None,
        });
    }
    Ok(result)
}
