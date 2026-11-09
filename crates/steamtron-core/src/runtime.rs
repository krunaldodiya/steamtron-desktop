//! Launch planning for SteamTron-managed games.
//!
//! The plan is a plain process description: Proton runner, target executable,
//! arguments, environment and working directory. Nothing here mounts an
//! overlay; fixes overlays remain a separate phase.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::library::{Game, LaunchTarget};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("game has no launch target")]
    NoTarget,
    #[error("launch target does not exist: {0}")]
    MissingExecutable(PathBuf),
    #[error("game content folder does not exist: {0}")]
    MissingContent(PathBuf),
    #[error("{0} is not configured")]
    MissingRuntime(&'static str),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub cwd: PathBuf,
}

impl LaunchPlan {
    /// Build a std process command; stdout/stderr handling stays with the caller.
    pub fn command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.arguments).current_dir(&self.cwd);
        command.envs(&self.environment);
        command
    }
}

pub fn primary_target(game: &Game) -> Option<&LaunchTarget> {
    game.launch_targets
        .iter()
        .find(|target| target.primary)
        .or_else(|| game.launch_targets.first())
}

fn proton_runner(game: &Game) -> Option<&Path> {
    for value in [&game.launch.proton, &game.installation.proton] {
        if !value.is_empty() {
            return Some(Path::new(value));
        }
    }
    None
}

/// Plan a Steam-free Proton launch for one managed game.
pub fn managed_launch_plan(
    game: &Game,
    target: &LaunchTarget,
    steam_root: &Path,
) -> Result<LaunchPlan, RuntimeError> {
    let root = &game.installation.root;
    if !root.is_dir() {
        return Err(RuntimeError::MissingContent(root.clone()));
    }
    let executable = if target.executable.is_absolute() {
        target.executable.clone()
    } else {
        root.join(&target.executable)
    };
    if !executable.is_file() {
        return Err(RuntimeError::MissingExecutable(executable));
    }
    let proton = proton_runner(game).ok_or(RuntimeError::MissingRuntime("Proton runner"))?;
    if !proton.exists() {
        return Err(RuntimeError::MissingRuntime("Proton runner"));
    }
    let prefix = &game.installation.prefix;
    if prefix.as_os_str().is_empty() {
        return Err(RuntimeError::MissingRuntime("game prefix"));
    }
    fs::create_dir_all(prefix).map_err(|error| RuntimeError::Io {
        path: prefix.clone(),
        source: error,
    })?;

    let mut arguments = vec![
        "waitforexitandrun".to_owned(),
        executable.display().to_string(),
    ];
    arguments.extend(target.arguments.iter().cloned());

    let mut environment = BTreeMap::new();
    environment.insert(
        "STEAM_COMPAT_DATA_PATH".into(),
        prefix.display().to_string(),
    );
    environment.insert(
        "STEAM_COMPAT_CLIENT_INSTALL_PATH".into(),
        steam_root.display().to_string(),
    );
    if let Some(app_id) = game.metadata_app_id {
        environment.insert("SteamAppId".into(), app_id.to_string());
        environment.insert("SteamGameId".into(), app_id.to_string());
    }
    Ok(LaunchPlan {
        program: proton.to_path_buf(),
        arguments,
        environment,
        cwd: root.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Artwork, Installation, InstallMode, LaunchSettings};

    fn game(root: &Path, proton: &Path, prefix: &Path) -> Game {
        Game {
            id: "42".into(),
            title: "Sample".into(),
            metadata_app_id: Some(42),
            metadata_source: None,
            installation: Installation {
                mode: InstallMode::Managed,
                root: root.to_path_buf(),
                prefix: prefix.to_path_buf(),
                proton: proton.display().to_string(),
                manifest: PathBuf::new(),
                confirmed: true,
            },
            steam: None,
            launch: LaunchSettings::default(),
            launch_targets: vec![LaunchTarget {
                id: "t1".into(),
                name: "Sample".into(),
                executable: PathBuf::from("Sample.exe"),
                arguments: vec!["-windowed".into()],
                primary: true,
            }],
            fixes_enabled: true,
            artwork: Artwork::default(),
        }
    }

    #[test]
    fn plans_proton_with_prefix_and_app_id() {
        let temp = tempfile::tempdir().unwrap();
        let proton = temp.path().join("proton");
        fs::write(&proton, b"#!/bin/sh\n").unwrap();
        let root = temp.path().join("common/Sample");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Sample.exe"), b"MZ").unwrap();
        let prefix = temp.path().join("compatdata/Sample");
        let steam_root = temp.path().join("Steam");
        let game = game(&root, &proton, &prefix);
        let plan = managed_launch_plan(&game, primary_target(&game).unwrap(), &steam_root).unwrap();
        assert_eq!(plan.program, proton);
        assert_eq!(
            plan.arguments,
            vec![
                "waitforexitandrun".to_owned(),
                root.join("Sample.exe").display().to_string(),
                "-windowed".to_owned()
            ]
        );
        assert_eq!(plan.cwd, root);
        assert_eq!(
            plan.environment.get("STEAM_COMPAT_DATA_PATH").unwrap(),
            &prefix.display().to_string()
        );
        assert_eq!(plan.environment.get("SteamAppId").unwrap(), "42");
        assert!(prefix.is_dir(), "the prefix is created before launch");
    }

    #[test]
    fn reports_missing_executable_and_content() {
        let temp = tempfile::tempdir().unwrap();
        let proton = temp.path().join("proton");
        fs::write(&proton, b"#!/bin/sh\n").unwrap();
        let root = temp.path().join("common/Sample");
        fs::create_dir_all(&root).unwrap();
        let mut game = game(&root, &proton, &temp.path().join("prefix"));
        let target = primary_target(&game).unwrap().clone();
        assert!(matches!(
            managed_launch_plan(&game, &target, temp.path()),
            Err(RuntimeError::MissingExecutable(_))
        ));
        fs::write(root.join("Sample.exe"), b"MZ").unwrap();
        fs::remove_dir_all(&root).unwrap();
        assert!(matches!(
            managed_launch_plan(&game, &target, temp.path()),
            Err(RuntimeError::MissingContent(_))
        ));
        game.launch.proton.clear();
        game.installation.proton.clear();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Sample.exe"), b"MZ").unwrap();
        assert!(matches!(
            managed_launch_plan(&game, &target, temp.path()),
            Err(RuntimeError::MissingRuntime("Proton runner"))
        ));
    }

    #[test]
    fn primary_target_wins() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("common/Sample");
        fs::create_dir_all(&root).unwrap();
        let proton = temp.path().join("proton");
        fs::write(&proton, b"#!/bin/sh\n").unwrap();
        let mut game = game(&root, &proton, &temp.path().join("prefix"));
        game.launch_targets = vec![
            LaunchTarget {
                id: "t1".into(),
                name: "Launcher".into(),
                executable: PathBuf::from("launcher.exe"),
                arguments: Vec::new(),
                primary: false,
            },
            LaunchTarget {
                id: "t2".into(),
                name: "Game".into(),
                executable: PathBuf::from("game.exe"),
                arguments: Vec::new(),
                primary: true,
            },
        ];
        assert_eq!(primary_target(&game).unwrap().id, "t2");
        game.launch_targets[1].primary = false;
        assert_eq!(primary_target(&game).unwrap().id, "t1");
    }
}
