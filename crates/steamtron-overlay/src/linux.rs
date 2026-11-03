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
    mounted: bool,
}

impl LinuxOverlay {
    pub fn new(game: OverlayGame) -> Self {
        Self::with_data_home(game, default_data_home())
    }

    pub fn with_data_home(game: OverlayGame, data_home: impl Into<PathBuf>) -> Self {
        let layout = OverlayLayout::for_game(&game, &data_home.into());
        Self {
            game,
            layout,
            mounted: false,
        }
    }

    pub fn game(&self) -> &OverlayGame {
        &self.game
    }

    pub fn layout(&self) -> &OverlayLayout {
        &self.layout
    }

    pub fn merged(&self) -> &Path {
        &self.layout.merged
    }

    pub fn mount(&mut self) -> Result<&Path, OverlayError> {
        if self.mounted {
            return Ok(&self.layout.merged);
        }
        let fuse = require_tool("fuse-overlayfs")?;
        if which("fusermount3").is_none() && which("fusermount").is_none() {
            return Err(OverlayError::ToolMissing(
                "fusermount3 or fusermount".into(),
            ));
        }

        self.layout.prepare()?;
        if self.layout.merged.exists() && is_mountpoint(&self.layout.merged) {
            return Err(OverlayError::AlreadyMounted(
                self.layout.merged.display().to_string(),
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
            .arg(&self.layout.merged)
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
            if is_mountpoint(&self.layout.merged) {
                self.mounted = true;
                return Ok(&self.layout.merged);
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        self.cleanup_runtime();
        Err(OverlayError::MountFailed(
            "the fixes overlay did not become a mountpoint".into(),
        ))
    }

    pub fn unmount(&mut self) -> Result<(), OverlayError> {
        if self.mounted {
            unmount_path(&self.layout.merged)?;
            self.mounted = false;
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
            .is_some_and(|value| value == path)
    })
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

