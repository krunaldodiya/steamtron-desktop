//! Global application directories for SteamTron.
//!
//! One layout is used by every crate and platform:
//!
//! | Purpose | Linux | Windows |
//! |---|---|---|
//! | Data | `$XDG_DATA_HOME/steamtron` | `%LOCALAPPDATA%\\SteamTron` |
//! | Config | `$XDG_CONFIG_HOME/steamtron` | `%APPDATA%\\SteamTron` |
//! | Cache | `$XDG_CACHE_HOME/steamtron` | `%LOCALAPPDATA%\\SteamTron\\cache` |
//!
//! Per-game content never lives here: it belongs to a drive under
//! `<drive>/SteamTronLibrary/steamtronapps`. UmuTron data is read-only for
//! importers.

use std::path::{Path, PathBuf};

/// Resolved global directories for one installation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub data_home: PathBuf,
    pub config_home: PathBuf,
    pub cache_home: PathBuf,
}

impl Layout {
    pub fn new(data_home: impl Into<PathBuf>, config_home: impl Into<PathBuf>, cache_home: impl Into<PathBuf>) -> Self {
        Self {
            data_home: data_home.into(),
            config_home: config_home.into(),
            cache_home: cache_home.into(),
        }
    }

    fn from_environment() -> Self {
        Self {
            data_home: env_or("STEAMTRON_DATA_HOME", default_data_home()),
            config_home: env_or("STEAMTRON_CONFIG_HOME", default_config_home()),
            cache_home: env_or("STEAMTRON_CACHE_HOME", default_cache_home()),
        }
    }

    /// `<data home>/steamtron`.
    pub fn home(&self) -> PathBuf {
        self.data_home.join("steamtron")
    }

    /// `<config home>/steamtron`.
    pub fn config_dir(&self) -> PathBuf {
        self.config_home.join("steamtron")
    }

    /// `<cache home>/steamtron`.
    pub fn cache_dir(&self) -> PathBuf {
        self.cache_home.join("steamtron")
    }

    pub fn library(&self) -> PathBuf {
        self.home().join("library.json")
    }

    /// Pre-migration SteamTron configuration, read-only for importers.
    pub fn legacy_config(&self) -> PathBuf {
        self.home().join("config.json")
    }

    /// Pre-migration per-App-ID fixes, read-only for importers.
    pub fn legacy_fixes_root(&self) -> PathBuf {
        self.home().join("fixes")
    }

    pub fn artwork(&self) -> PathBuf {
        self.home().join("artwork")
    }

    pub fn tools(&self) -> PathBuf {
        self.home().join("tools")
    }

    pub fn goldberg(&self, tag: &str) -> PathBuf {
        self.tools().join("goldberg").join(format!("gse_fork-{tag}"))
    }

    pub fn steamless(&self) -> PathBuf {
        self.tools().join("steamless").join("steamless-linux")
    }

    pub fn seven_zip(&self) -> PathBuf {
        self.tools().join("7zz")
    }

    pub fn runners(&self) -> PathBuf {
        self.home().join("runners")
    }

    pub fn diagnostics(&self) -> PathBuf {
        self.home().join("diagnostics")
    }

    pub fn diagnostics_for(&self, game_id: &str) -> PathBuf {
        self.diagnostics().join(game_id)
    }

    pub fn play_history(&self) -> PathBuf {
        self.home().join("play-history")
    }

    /// Persistent copy-on-write upper layers keyed by game identity.
    pub fn overlay_upper(&self) -> PathBuf {
        self.home().join("overlay-upper")
    }

    /// Launch request/session/lock files for supervisors.
    pub fn launch(&self) -> PathBuf {
        self.home().join("launch")
    }

    /// Default prefixes for games that do not live on a managed drive.
    pub fn prefixes(&self) -> PathBuf {
        self.home().join("prefixes")
    }

    pub fn backups(&self) -> PathBuf {
        self.home().join("backups")
    }

    pub fn integrity(&self) -> PathBuf {
        self.home().join("integrity")
    }

    /// Credentials and tokens for the account service.
    pub fn credentials(&self) -> PathBuf {
        self.home().join("api-token.json")
    }
}

/// The layout for the current process.
pub fn layout() -> Layout {
    Layout::from_environment()
}

fn env_or(key: &str, fallback: PathBuf) -> PathBuf {
    match std::env::var_os(key) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => fallback,
    }
}

#[cfg(target_os = "windows")]
fn default_data_home() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(target_os = "windows"))]
fn default_data_home() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

#[cfg(target_os = "windows")]
fn default_config_home() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(target_os = "windows"))]
fn default_config_home() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

#[cfg(target_os = "windows")]
fn default_cache_home() -> PathBuf {
    default_data_home().join("cache")
}

#[cfg(not(target_os = "windows"))]
fn default_cache_home() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache")
}

#[cfg(not(target_os = "windows"))]
fn xdg(key: &str, relative: &str) -> PathBuf {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(relative)
        })
}

/// Create one private directory (0700 on Unix) and return it.
pub fn ensure_private_dir(path: &Path) -> Result<PathBuf, std::io::Error> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_at(root: &Path) -> Layout {
        Layout::new(root.join("data"), root.join("config"), root.join("cache"))
    }

    #[test]
    fn global_paths_are_stable_and_namespaced() {
        let temp = tempfile::tempdir().unwrap();
        let layout = layout_at(temp.path());
        let home = temp.path().join("data").join("steamtron");
        assert_eq!(layout.home(), home);
        assert_eq!(layout.library(), home.join("library.json"));
        assert_eq!(layout.legacy_config(), home.join("config.json"));
        assert_eq!(layout.legacy_fixes_root(), home.join("fixes"));
        assert_eq!(layout.artwork(), home.join("artwork"));
        assert_eq!(layout.tools(), home.join("tools"));
        assert_eq!(layout.goldberg("2026_02_16"), home.join("tools/goldberg/gse_fork-2026_02_16"));
        assert_eq!(layout.steamless(), home.join("tools/steamless/steamless-linux"));
        assert_eq!(layout.diagnostics_for("game-1"), home.join("diagnostics/game-1"));
        assert_eq!(layout.overlay_upper(), home.join("overlay-upper"));
        assert_eq!(layout.launch(), home.join("launch"));
        assert_eq!(layout.config_dir(), temp.path().join("config").join("steamtron"));
        assert_eq!(layout.cache_dir(), temp.path().join("cache").join("steamtron"));
    }


    #[test]
    fn private_directory_is_created() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("diagnostics/game");
        ensure_private_dir(&path).unwrap();
        assert!(path.is_dir());
    }
}
