//! Platform adapters for the Steam unlock/library backend.
//!
//! The manager (SteamTron Desktop) only consumes this interface. The actual
//! Steam client hook remains a platform implementation:
//!   Linux   -> SLSsteam / Lumen
//!   Windows -> BetterSteamTools / OpenSteamTool

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnlockError {
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

impl UnlockError {
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

#[derive(Clone, Debug)]
pub struct InstalledLua {
    pub appid: String,
    pub lua_path: PathBuf,
    pub manifests: Vec<PathBuf>,
}

pub trait UnlockBackend {
    fn status(&self) -> Result<BackendStatus, UnlockError>;
    fn install_lua(
        &self,
        appid: &str,
        lua: &str,
        manifests: &BTreeMap<String, Vec<u8>>,
    ) -> Result<InstalledLua, UnlockError>;
    fn remove_lua(&self, appid: &str) -> Result<(), UnlockError>;
}

fn validate_appid(appid: &str) -> Result<(), UnlockError> {
    if appid.is_empty() || !appid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(UnlockError::InvalidAppId);
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<(), UnlockError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| UnlockError::io(parent, error))?;
    }
    let temporary = path.with_extension("steamtron.tmp");
    fs::write(&temporary, bytes).map_err(|error| UnlockError::io(&temporary, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))
            .map_err(|error| UnlockError::io(&temporary, error))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    fs::rename(&temporary, path).map_err(|error| UnlockError::io(path, error))
}

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxUnlock;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsUnlock;

#[cfg(target_os = "linux")]
pub fn platform_backend() -> Result<LinuxUnlock, UnlockError> {
    LinuxUnlock::discover()
}

#[cfg(target_os = "windows")]
pub fn platform_backend() -> Result<WindowsUnlock, UnlockError> {
    WindowsUnlock::discover()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("SteamTron Desktop supports Linux and Windows");
