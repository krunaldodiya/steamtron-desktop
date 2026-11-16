//! Per-game, explicitly approved pre-launch scripts.
//!
//! A script runs on the host as the current user after the prefix exists and
//! before the game starts. SteamTron only verifies, executes and reports it;
//! all game-specific fix logic belongs to the script and must be idempotent.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::library::Game;

use crate::process::hidden_command;
pub const MAX_SCRIPT_BYTES: u64 = 1024 * 1024;
pub const TIMEOUT_SECONDS: u64 = 120;

/// Validate a user-selected script and return its approved digest.
pub fn approve(path: &Path) -> Result<String, String> {
    let script = validate_script(path)?;
    checksum(&script)
}

/// Re-verify an approved script before execution.
pub fn verify(path: &str, expected: &str) -> Result<PathBuf, String> {
    let script = validate_script(Path::new(path))?;
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("The pre-launch script has no approved SHA-256 digest.".into());
    }
    if checksum(&script)? != expected.to_ascii_lowercase() {
        return Err("The pre-launch script changed since it was approved. Review it and approve it again.".into());
    }
    Ok(script)
}

fn validate_script(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("The pre-launch script path must be absolute.".into());
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err("The pre-launch script must be a regular file, not a link.".into());
    }
    if !metadata.is_file() {
        return Err("The pre-launch script must be a regular file.".into());
    }
    if metadata.len() > MAX_SCRIPT_BYTES {
        return Err("The pre-launch script is too large.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o002 != 0 {
            return Err("The pre-launch script must not be world-writable.".into());
        }
        if metadata.mode() & 0o100 == 0 {
            return Err("The pre-launch script must be executable. Run chmod +x on it first.".into());
        }
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err("The pre-launch script must be owned by you.".into());
        }
    }
    Ok(path.to_path_buf())
}

fn checksum(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Run the approved hook before starting the game. Output is appended to the
/// play log; a non-zero exit, timeout, or changed digest blocks the launch.
pub fn run(game: &Game, proton: &Path, log: &std::fs::File) -> Result<(), String> {
    if game.launch.prelaunch.is_empty() {
        return Ok(());
    }
    let script = verify(&game.launch.prelaunch, &game.launch.prelaunch_sha256)?;
    let prefix = game.installation.prefix.display().to_string();
    let drive_c = game.installation.prefix.join("drive_c");
    let mut command = hidden_command(&script);
    command
        .current_dir(&game.installation.root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(|error| error.to_string())?))
        .stderr(Stdio::from(log.try_clone().map_err(|error| error.to_string())?))
        .env("UMU_GAME_DIR", game.installation.root.display().to_string())
        .env("UMU_GAME_ID", &game.id)
        .env("UMU_TITLE", &game.title)
        .env("UMU_PREFIX", &prefix)
        .env("UMU_DRIVE_C", drive_c.display().to_string())
        .env("UMU_PROTON", proton.display().to_string())
        .env("STEAMTRON_GAME_DIR", game.installation.root.display().to_string())
        .env("STEAMTRON_GAME_ID", &game.id)
        .env("STEAMTRON_TITLE", &game.title)
        .env("STEAMTRON_PREFIX", &prefix)
        .env("STEAMTRON_DRIVE_C", drive_c.display().to_string())
        .env("STEAMTRON_PROTON", proton.display().to_string());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|error| format!("Could not start the pre-launch script: {error}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => {
                if status.success() {
                    return Ok(());
                }
                return Err(format!(
                    "Pre-launch script failed with exit code {}. The game was not started.",
                    status.code().unwrap_or(-1)
                ));
            }
            None if started.elapsed() > Duration::from_secs(TIMEOUT_SECONDS) => {
                #[cfg(unix)]
                {
                    let _ = hidden_command("kill").args(["-KILL", &format!("-{}", child.id())]).status();
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Pre-launch script did not finish within {TIMEOUT_SECONDS} seconds. The game was not started."
                ));
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::library::{Artwork, Game, Installation, InstallMode, LaunchSettings};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    fn sample_game(root: &Path, prefix: &Path) -> Game {
        Game {
            id: "42".into(),
            title: "Sample".into(),
            metadata_app_id: None,
            metadata_source: None,
            installation: Installation {
                mode: InstallMode::Linked,
                root: root.to_path_buf(),
                prefix: prefix.to_path_buf(),
                proton: "/tmp/proton".into(),
                manifest: PathBuf::new(),
                confirmed: true,
            },
            steam: None,
            launch: LaunchSettings::default(),
            launch_targets: Vec::new(),
            fixes_enabled: true,
            artwork: Artwork::default(),
            setup_completed_at: None,
        }
    }

    fn script(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn rejects_a_changed_script() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("hook.sh");
        script(&path, "#!/bin/sh\nexit 0\n");
        let digest = approve(&path).unwrap();
        verify(path.to_str().unwrap(), &digest).unwrap();
        script(&path, "#!/bin/sh\nexit 1\n");
        assert!(verify(path.to_str().unwrap(), &digest).is_err());
    }

    #[test]
    fn runs_an_approved_script_with_game_environment() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("game");
        let prefix = temp.path().join("prefix");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&prefix).unwrap();
        let marker = root.join("ran");
        let path = temp.path().join("hook.sh");
        script(
            &path,
            &format!("#!/bin/sh\nprintf '%s' \"$UMU_GAME_ID\" > '{}'\n", marker.display()),
        );
        let digest = approve(&path).unwrap();
        let mut game = sample_game(&root, &prefix);
        game.launch.prelaunch = path.display().to_string();
        game.launch.prelaunch_sha256 = digest;
        let log = temp.path().join("play.log");
        let log = fs::File::create(&log).unwrap();
        run(&game, Path::new("/tmp/proton"), &log).unwrap();
        assert_eq!(fs::read_to_string(&marker).unwrap(), "42");
    }
}
