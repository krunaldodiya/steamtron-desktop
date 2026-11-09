//! Platform registration backends for Steam App IDs.
//!
//! The registration backend receives only the App ID, display title, Lua
//! manifest and optional depot manifests. Content downloads are a separate
//! concern: registration never fetches game data.
//!
//! * Linux — SLSsteam / Lumen (`<steam>/config/stplug-in/<appid>.lua`).
//! * Windows — BetterSteamTools / OpenSteamTool.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistrationError {
    #[error("Steam was not found")]
    SteamNotFound,
    #[error("invalid App ID")]
    InvalidAppId,
    #[error("backend is not installed: {0}")]
    NotInstalled(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl RegistrationError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BackendStatus {
    pub platform: &'static str,
    pub backend: &'static str,
    pub installed: bool,
    pub ready: bool,
    pub steam_root: String,
    pub lua_dir: String,
    pub details: String,
}

/// Everything a backend needs to register one App ID.
#[derive(Clone, Debug)]
pub struct RegistrationRequest<'a> {
    pub app_id: &'a str,
    pub title: &'a str,
    pub lua: &'a str,
    pub manifests: &'a BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct Registration {
    pub app_id: String,
    pub title: String,
    pub backend: String,
    pub lua_path: PathBuf,
    pub manifests: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RegistrationState {
    pub app_id: String,
    pub backend: String,
    pub registered: bool,
    pub lua_path: String,
}

pub trait RegistrationBackend {
    /// Stable identifier stored in `library.json` (`slssteam-moon`, ...).
    fn id(&self) -> &'static str;
    fn status(&self) -> Result<BackendStatus, RegistrationError>;
    fn register(&self, request: &RegistrationRequest<'_>) -> Result<Registration, RegistrationError>;
    fn verify(&self, app_id: &str) -> Result<RegistrationState, RegistrationError>;
    fn unregister(&self, app_id: &str) -> Result<(), RegistrationError>;
}

fn validate_appid(appid: &str) -> Result<(), RegistrationError> {
    if appid.is_empty() || !appid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(RegistrationError::InvalidAppId);
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<(), RegistrationError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| RegistrationError::io(parent, error))?;
    }
    let temporary = path.with_extension("steamtron.tmp");
    fs::write(&temporary, bytes).map_err(|error| RegistrationError::io(&temporary, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))
            .map_err(|error| RegistrationError::io(&temporary, error))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    fs::rename(&temporary, path).map_err(|error| RegistrationError::io(path, error))
}

fn write_manifests(
    target_dir: &Path,
    manifests: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<PathBuf>, RegistrationError> {
    let mut written = Vec::new();
    for (name, data) in manifests {
        if name.contains('/') || name.contains('\\') || !name.ends_with(".manifest") {
            return Err(RegistrationError::NotInstalled(format!(
                "invalid manifest name: {name}"
            )));
        }
        let target = target_dir.join(name);
        write_atomic(&target, data, 0o600)?;
        written.push(target);
    }
    Ok(written)
}

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::SlsteamBackend;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::BtsBackend;

#[cfg(target_os = "linux")]
pub fn platform_backend() -> Result<SlsteamBackend, RegistrationError> {
    SlsteamBackend::discover()
}

#[cfg(target_os = "windows")]
pub fn platform_backend() -> Result<BtsBackend, RegistrationError> {
    BtsBackend::discover()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("SteamTron Desktop supports Linux and Windows");
