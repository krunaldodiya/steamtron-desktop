pub mod config;
pub mod launch;
pub mod fixes;
pub mod steam;

use std::path::PathBuf;

pub fn data_home() -> PathBuf {
    if let Some(value) = std::env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share")
}

pub fn steamtron_home() -> PathBuf {
    data_home().join("steamtron")
}
