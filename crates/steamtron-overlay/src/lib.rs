//! Portable overlay configuration with the Linux `fuse-overlayfs` backend.
//!
//! The overlay engine deliberately owns the game/fixes layout. Platform
//! backends only present that layout to the game process.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;
use uuid::Uuid;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::LinuxOverlay;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::WindowsOverlay;

#[cfg(target_os = "linux")]
pub type PlatformOverlay = LinuxOverlay;

#[cfg(target_os = "windows")]
pub type PlatformOverlay = WindowsOverlay;

/// Overlay failures that must be surfaced to the desktop UI or launcher.
#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("invalid game layout: {0}")]
    InvalidGame(String),
    #[error("overlay tool is missing: {0}")]
    ToolMissing(String),
    #[error("could not mount the fixes overlay: {0}")]
    MountFailed(String),
    #[error("could not unmount the fixes overlay: {0}")]
    UnmountFailed(String),
    #[error("overlay path already contains a live mount: {0}")]
    AlreadyMounted(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl OverlayError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// One game plus the fixes directory that should sit above it.
#[derive(Clone, Debug)]
pub struct OverlayGame {
    common: PathBuf,
    fixes: PathBuf,
    appid: Option<String>,
}

impl OverlayGame {
    pub fn new(
        common: impl Into<PathBuf>,
        fixes: impl Into<PathBuf>,
        appid: Option<impl Into<String>>,
    ) -> Result<Self, OverlayError> {
        let common = common.into();
        let fixes = fixes.into();
        let appid = appid.map(Into::into);

        if let Some(value) = appid.as_deref() {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(OverlayError::InvalidGame(
                    "Steam App ID must be numeric".into(),
                ));
            }
        }

        let common = fs::canonicalize(&common).map_err(|error| OverlayError::io(&common, error))?;
        let fixes = fs::canonicalize(&fixes).map_err(|error| OverlayError::io(&fixes, error))?;
        if !common.is_dir() {
            return Err(OverlayError::InvalidGame(format!(
                "game directory is not a directory: {}",
                common.display()
            )));
        }
        if !fixes.is_dir() {
            return Err(OverlayError::InvalidGame(format!(
                "fixes directory is not a directory: {}",
                fixes.display()
            )));
        }
        if common == fixes || common.starts_with(&fixes) || fixes.starts_with(&common) {
            return Err(OverlayError::InvalidGame(
                "game and fixes directories must be separate".into(),
            ));
        }

        Ok(Self {
            common,
            fixes,
            appid,
        })
    }

    pub fn common(&self) -> &Path {
        &self.common
    }

    pub fn fixes(&self) -> &Path {
        &self.fixes
    }

    pub fn appid(&self) -> Option<&str> {
        self.appid.as_deref()
    }

    /// Stable identifier used for the persistent upper/work layers.
    pub fn key(&self) -> String {
        if let Some(appid) = self.appid.as_deref() {
            return appid.to_owned();
        }
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        digest.update(self.common.as_os_str().as_encoded_bytes());
        digest.update(b"\0");
        digest.update(self.fixes.as_os_str().as_encoded_bytes());
        format!("{:x}", digest.finalize())[..16].to_owned()
    }

    /// Rewrite an absolute command argument from the game root to the merged root.
    ///
    /// Windows canonicalization yields verbatim `\\?\D:\...` paths while the
    /// launch arguments carry plain `D:\...`; compare without the verbatim
    /// prefix and case-insensitively there, or the rewrite silently no-ops and
    /// the game runs from the untouched content instead of the merged view.
    pub fn rewrite_arg(&self, value: &str, merged: &Path) -> String {
        let prefix = self.common.to_string_lossy();
        let prefix = prefix.strip_prefix(r"\\?\").unwrap_or(&prefix);
        let value = value.strip_prefix(r"\\?\").unwrap_or(value);
        let matches = value.len() >= prefix.len()
            && if cfg!(windows) {
                value[..prefix.len()].eq_ignore_ascii_case(prefix)
            } else {
                &value[..prefix.len()] == prefix
            };
        if !matches {
            return value.to_owned();
        }
        let root = merged.to_string_lossy();
        let root = root.trim_end_matches(['\\', '/']);
        let rest = value[prefix.len()..].trim_start_matches(['\\', '/']);
        if rest.is_empty() {
            return root.to_owned();
        }
        format!("{root}{}{rest}", std::path::MAIN_SEPARATOR)
    }

    /// Rewrite a working directory if it is inside the game root.
    pub fn rewrite_cwd(&self, cwd: &Path, merged: &Path) -> PathBuf {
        PathBuf::from(self.rewrite_arg(&cwd.to_string_lossy(), merged))
    }
}

/// Per-user data root, matching the existing Python SteamTron layout.
pub fn default_data_home() -> PathBuf {
    if let Some(value) = std::env::var_os("STEAMTRON_DATA_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(value) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
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

/// Runtime directory owned by this process/session.
pub(crate) fn runtime_base() -> PathBuf {
    std::env::temp_dir().join(format!("steamtron-{}", user_id()))
}

#[cfg(unix)]
fn user_id() -> u32 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata("/proc/self")
        .map(|metadata| metadata.uid())
        .unwrap_or(0)
}

#[cfg(not(unix))]
fn user_id() -> u32 {
    0
}

pub(crate) fn session_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// Common runtime layout for the platform backend.
#[derive(Clone, Debug)]
pub struct OverlayLayout {
    pub upper: PathBuf,
    pub work: PathBuf,
    pub runtime: PathBuf,
    pub merged: PathBuf,
}

impl OverlayLayout {
    pub fn for_game(game: &OverlayGame, data_home: &Path) -> Self {
        let key = game.key();
        let id = session_id();
        let short_key = &key[..key.len().min(10)];
        let runtime = runtime_base().join(format!("{short_key}-{id}"));
        Self {
            upper: data_home.join("steamtron/upper").join(&key),
            work: data_home.join("steamtron/work").join(format!("{key}-{id}")),
            merged: runtime.join("merged"),
            runtime,
        }
    }

    pub fn prepare(&self) -> Result<(), OverlayError> {
        prepare_private_dir(&self.runtime)?;
        prepare_private_dir(&self.merged)?;
        prepare_private_dir(&self.upper)?;
        prepare_private_dir(&self.work)?;
        Ok(())
    }
}

fn prepare_private_dir(path: &Path) -> Result<(), OverlayError> {
    fs::create_dir_all(path).map_err(|error| OverlayError::io(path, error))?;
    if fs::symlink_metadata(path)
        .map_err(|error| OverlayError::io(path, error))?
        .file_type()
        .is_symlink()
    {
        return Err(OverlayError::InvalidGame(format!(
            "overlay directory is a symbolic link: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| OverlayError::io(path, error))?;
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod rewrite_tests {
    use super::*;

    fn game() -> OverlayGame {
        OverlayGame {
            common: PathBuf::from(r"\\?\D:\Lib\Game"),
            fixes: PathBuf::from(r"\\?\D:\Lib\fixes"),
            appid: Some("1".into()),
        }
    }

    #[test]
    fn rewrite_arg_maps_verbatim_game_root_to_merged_drive() {
        // canonicalize() yields `\\?\D:\...` while Play passes `D:\...`.
        assert_eq!(
            game().rewrite_arg(r"D:\Lib\Game\Atlas\Binaries\Win64\Game.exe", Path::new("S:\\")),
            r"S:\Atlas\Binaries\Win64\Game.exe"
        );
    }

    #[test]
    fn rewrite_arg_ignores_case_and_leaves_outside_paths_alone() {
        assert_eq!(
            game().rewrite_arg(r"d:\lib\game\Atlas\Game.exe", Path::new("S:\\")),
            r"S:\Atlas\Game.exe"
        );
        assert_eq!(
            game().rewrite_arg(r"E:\Other\Game.exe", Path::new("S:\\")),
            r"E:\Other\Game.exe"
        );
    }
}
