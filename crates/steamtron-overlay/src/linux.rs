//! Linux backend: `fuse-overlayfs` with a persistent per-game upper layer.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::{
    default_data_home, OverlayError, OverlayGame, OverlayLayout,
};

pub struct LinuxOverlay {
    game: OverlayGame,
    layout: OverlayLayout,
    mount_point: PathBuf,
    explicit_mount_point: Option<PathBuf>,
    mounted: bool,
}

impl LinuxOverlay {
    pub fn new(game: OverlayGame) -> Self {
        Self::with_data_home(game, default_data_home())
    }

    pub fn with_data_home(game: OverlayGame, data_home: impl Into<PathBuf>) -> Self {
        let layout = OverlayLayout::for_game(&game, &data_home.into());
        let mount_point = layout.merged.clone();
        Self {
            game,
            layout,
            mount_point,
            explicit_mount_point: None,
            mounted: false,
        }
    }

    /// Mount the union at a stable local path instead of the session runtime
    /// directory.
    ///
    /// Linux uses this for `common/.steamtron-junction/<Game>` so launchers see
    /// the same install path on every run. The path is a symlink while idle and
    /// the caller removes it before mounting.
    pub fn with_mount_point(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        self.mount_point = path.clone();
        self.explicit_mount_point = Some(path);
        self
    }

    pub fn game(&self) -> &OverlayGame {
        &self.game
    }

    pub fn layout(&self) -> &OverlayLayout {
        &self.layout
    }

    pub fn merged(&self) -> &Path {
        &self.mount_point
    }

    pub fn mount(&mut self) -> Result<&Path, OverlayError> {
        if self.mounted {
            return Ok(&self.mount_point);
        }
        let fuse = require_tool("fuse-overlayfs")?;
        if which("fusermount3").is_none() && which("fusermount").is_none() {
            return Err(OverlayError::ToolMissing(
                "fusermount3 or fusermount".into(),
            ));
        }

        self.layout.prepare()?;
        self.prepare_mount_point()?;
        if is_mountpoint(&self.mount_point) {
            return Err(OverlayError::AlreadyMounted(
                self.mount_point.display().to_string(),
            ));
        }

        let lower = format!(
            "{}:{}",
            self.game.fixes().display(),
            self.game.common().display()
        );
        let options = format!(
            "lowerdir={lower},upperdir={},workdir={}",
            self.layout.upper.display(),
            self.layout.work.display()
        );
        let output = Command::new(&fuse)
            .args(["-o", &options, "-o", "nosuid,nodev"])
            .arg(&self.mount_point)
            .output()
            .map_err(|error| OverlayError::io(&fuse, error))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            self.cleanup_runtime();
            return Err(OverlayError::MountFailed(if detail.is_empty() {
                "fuse-overlayfs returned a non-zero status".into()
            } else {
                detail
            }));
        }

        // fuse-overlayfs normally returns after the mount is visible, but give
        // it a short bounded window for the mountinfo update.
        for _ in 0..20 {
            if is_mountpoint(&self.mount_point) {
                self.mounted = true;
                return Ok(&self.mount_point);
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        self.cleanup_runtime();
        Err(OverlayError::MountFailed(
            "the fixes overlay did not become a mountpoint".into(),
        ))
    }

    /// Ensure an explicit mount point is a plain directory, or create it.
    fn prepare_mount_point(&self) -> Result<(), OverlayError> {
        if self.explicit_mount_point.is_none() {
            return Ok(());
        }
        if let Some(parent) = self.mount_point.parent() {
            fs::create_dir_all(parent).map_err(|error| OverlayError::io(parent, error))?;
        }
        match fs::symlink_metadata(&self.mount_point) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                // The idle launch root is a symlink; the caller normally
                // removes it first, but be defensive.
                fs::remove_file(&self.mount_point)
                    .map_err(|error| OverlayError::io(&self.mount_point, error))?;
                fs::create_dir_all(&self.mount_point)
                    .map_err(|error| OverlayError::io(&self.mount_point, error))?;
            }
            Ok(metadata) if metadata.is_dir() => {
                let empty = fs::read_dir(&self.mount_point)
                    .map_err(|error| OverlayError::io(&self.mount_point, error))?
                    .next()
                    .is_none();
                if !empty {
                    return Err(OverlayError::MountFailed(format!(
                        "mountpoint is not empty: {}",
                        self.mount_point.display()
                    )));
                }
            }
            Ok(_) => {
                return Err(OverlayError::MountFailed(format!(
                    "mountpoint is not a directory: {}",
                    self.mount_point.display()
                )))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(&self.mount_point)
                    .map_err(|error| OverlayError::io(&self.mount_point, error))?;
            }
            Err(error) => return Err(OverlayError::io(&self.mount_point, error)),
        }
        Ok(())
    }

    pub fn unmount(&mut self) -> Result<(), OverlayError> {
        if self.mounted {
            if is_mountpoint(&self.mount_point) {
                unmount_path(&self.mount_point)?;
            }
            self.mounted = false;
        }
        if self.explicit_mount_point.is_some() {
            let _ = fs::remove_dir(&self.mount_point);
        }
        self.cleanup_runtime();
        Ok(())
    }

    fn cleanup_runtime(&self) {
        if self.layout.runtime.exists() {
            let _ = fs::remove_dir_all(&self.layout.runtime);
        }
        if self.layout.work.exists() {
            let _ = fs::remove_dir_all(&self.layout.work);
        }
    }
}

impl Drop for LinuxOverlay {
    fn drop(&mut self) {
        let _ = self.unmount();
    }
}

fn require_tool(name: &str) -> Result<PathBuf, OverlayError> {
    which(name).ok_or_else(|| OverlayError::ToolMissing(name.into()))
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn is_mountpoint(path: &Path) -> bool {
    let Ok(mountinfo) = fs::read_to_string("/proc/self/mountinfo") else {
        return false;
    };
    let path = path.to_string_lossy();
    mountinfo.lines().any(|line| {
        line.split(" - ")
            .next()
            .and_then(|prefix| prefix.split_whitespace().nth(4))
            .map(unescape_mountinfo)
            .is_some_and(|value| value == path)
    })
}

fn unescape_mountinfo(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let digits = &value[index + 1..index + 4];
            if digits.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
                if let Ok(decoded) = u8::from_str_radix(digits, 8) {
                    result.push(decoded as char);
                    index += 4;
                    continue;
                }
            }
        }
        result.push(bytes[index] as char);
        index += 1;
    }
    result
}

fn unmount_path(path: &Path) -> Result<(), OverlayError> {
    let tools = ["fusermount3", "fusermount"];
    let mut last_error = String::new();
    for tool in tools {
        let Some(binary) = which(tool) else {
            continue;
        };
        let status = Command::new(binary)
            .args(["-u", &path.to_string_lossy()])
            .status()
            .map_err(|error| OverlayError::io(path, error))?;
        if status.success() {
            return Ok(());
        }
        last_error = format!("{tool} exited with {status}");
        let status = Command::new(tool)
            .args(["-uz", &path.to_string_lossy()])
            .status();
        if status.is_ok_and(|status| status.success()) {
            return Ok(());
        }
    }
    Err(OverlayError::UnmountFailed(if last_error.is_empty() {
        format!("no usable fusermount tool for {}", path.display())
    } else {
        last_error
    }))
}

