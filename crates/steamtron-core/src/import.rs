//! Dry-run importers for existing installations.
//!
//! Nothing in this module mutates the filesystem. Each planner reads a source
//! document and returns the actions an import *would* take, so the UI can show
//! a preview before anything moves.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::library::{
    Artwork, Game, InstallMode, Installation, LaunchMode, LaunchSettings, LaunchTarget, LibrarySettings,
    MetadataSource, RegistrationState, SteamRegistration,
};

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported source format: {0}")]
    Format(String),
}

impl ImportError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportSource {
    SteamTronConfig,
    UmuTronLibrary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedGame {
    pub game: Game,
    pub source: ImportSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportAction {
    AddDrive { root: PathBuf, label: String },
    AddGame { planned: PlannedGame },
    RegisterSteam { app_id: u32, title: String },
    MoveFixes { from: PathBuf, to: PathBuf },
    /// A source entry that cannot become a game without a user decision.
    NeedsInput { app_id: Option<u32>, title: String, reason: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportPlan {
    pub source: Option<ImportSource>,
    pub actions: Vec<ImportAction>,
    pub warnings: Vec<String>,
}

impl ImportPlan {
    pub fn games(&self) -> impl Iterator<Item = &PlannedGame> {
        self.actions.iter().filter_map(|action| match action {
            ImportAction::AddGame { planned } => Some(planned),
            _ => None,
        })
    }

    pub fn registers(&self) -> impl Iterator<Item = (u32, &str)> {
        self.actions.iter().filter_map(|action| match action {
            ImportAction::RegisterSteam { app_id, title } => Some((*app_id, title.as_str())),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------------------
// Legacy SteamTron config.json (App-ID keyed)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LegacyDocument {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    games: BTreeMap<String, LegacyGame>,
}

#[derive(Debug, Default, Deserialize)]
struct LegacyGame {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    fixes: PathBuf,
    #[serde(default)]
    launch_targets: Vec<LegacyTarget>,
    #[serde(default)]
    launch_target: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct LegacyTarget {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    executable: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    primary: bool,
}

pub fn plan_legacy_config(path: impl AsRef<Path>) -> Result<ImportPlan, ImportError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|error| ImportError::io(path, error))?;
    let document: LegacyDocument = serde_json::from_str(&text).map_err(|source| ImportError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    if document.version != 0 && document.version != 1 {
        return Err(ImportError::Format(format!(
            "unsupported config version {}",
            document.version
        )));
    }
    let mut plan = ImportPlan {
        source: Some(ImportSource::SteamTronConfig),
        ..ImportPlan::default()
    };
    let mut drives = BTreeSet::new();
    for (app_id, entry) in document.games {
        let app_id: u32 = match app_id.parse() {
            Ok(value) if value > 0 => value,
            _ => {
                plan.warnings.push(format!("ignoring non-numeric App ID {app_id}"));
                continue;
            }
        };
        let mut targets = Vec::new();
        for (index, target) in entry.launch_targets.iter().enumerate() {
            targets.push(LaunchTarget {
                id: if target.id.is_empty() {
                    format!("t{}", index + 1)
                } else {
                    target.id.clone()
                },
                name: target.name.clone(),
                executable: target.executable.clone(),
                arguments: target.arguments.clone(),
                primary: target.primary || index == 0,
            });
        }
        if targets.is_empty() {
            if let Some(executable) = entry.launch_target {
                targets.push(LaunchTarget {
                    id: "t1".into(),
                    name: String::new(),
                    executable,
                    arguments: Vec::new(),
                    primary: true,
                });
            }
        }
        let Some(target) = targets.iter().find(|target| target.primary).or_else(|| targets.first()) else {
            plan.actions.push(ImportAction::NeedsInput {
                app_id: Some(app_id),
                title: format!("App {app_id}"),
                reason: "no launch target recorded; choose the game executable".into(),
            });
            continue;
        };
        let Some((root, steamapps)) = derive_steam_root(&target.executable) else {
            plan.actions.push(ImportAction::NeedsInput {
                app_id: Some(app_id),
                title: format!("App {app_id}"),
                reason: format!(
                    "launch target is not inside a Steam library: {}",
                    target.executable.display()
                ),
            });
            continue;
        };
        if let Some(drive) = steamapps.parent() {
            if drives.insert(drive.to_path_buf()) {
                plan.actions.push(ImportAction::AddDrive {
                    root: drive.to_path_buf(),
                    label: drive
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| drive.display().to_string()),
                });
            }
        }
        let title = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("App {app_id}"));
        let game = Game {
            id: format!("steam-{app_id}"),
            title: title.clone(),
            metadata_app_id: Some(app_id),
            metadata_source: Some(MetadataSource {
                provider: "steam".into(),
                id: app_id,
            }),
            installation: Installation {
                mode: InstallMode::Linked,
                root,
                prefix: PathBuf::new(),
                proton: String::new(),
                manifest: PathBuf::new(),
                confirmed: true,
            },
            steam: Some(SteamRegistration {
                app_id,
                registered: true,
                backend: String::new(),
                state: RegistrationState::Registered,
                registered_at: 0,
                launch_mode: LaunchMode::Emulator,
            }),
            launch: LaunchSettings::default(),
            launch_targets: targets,
            fixes_enabled: entry.enabled,
            artwork: Artwork::default(),
        };
        plan.actions.push(ImportAction::AddGame {
            planned: PlannedGame {
                game,
                source: ImportSource::SteamTronConfig,
            },
        });
        plan.actions.push(ImportAction::RegisterSteam {
            app_id,
            title: title.clone(),
        });
        if !entry.fixes.as_os_str().is_empty() && entry.fixes.is_dir() {
            // The managed fixes folder is decided by the importer UI once a
            // drive is chosen; report the source so nothing is lost.
            plan.warnings.push(format!(
                "fixes for App {app_id} need a destination: {}",
                entry.fixes.display()
            ));
        }
    }
    Ok(plan)
}

fn derive_steam_root(executable: &Path) -> Option<(PathBuf, PathBuf)> {
    for ancestor in executable.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) == Some("common") {
            let steamapps = ancestor.parent()?;
            if steamapps.file_name().and_then(|name| name.to_str()) == Some("steamapps") {
                let game_root = ancestor.join(executable.strip_prefix(ancestor).ok()?.iter().next()?);
                return Some((game_root, steamapps.to_path_buf()));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// UmuTron library.json
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UmuDocument {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    settings: UmuSettings,
    #[serde(default)]
    games: Vec<UmuGame>,
}

#[derive(Debug, Default, Deserialize)]
struct UmuSettings {
    #[serde(default)]
    default_display_mode: Option<String>,
    #[serde(default)]
    gamemode: Option<bool>,
    #[serde(default)]
    picker_folders: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct UmuGame {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    metadata_app_id: Option<u32>,
    #[serde(default)]
    installation: Option<UmuInstallation>,
    #[serde(default)]
    launch: Option<UmuLaunch>,
    #[serde(default)]
    launch_targets: Vec<UmuTarget>,
    #[serde(default)]
    fixes_enabled: Option<bool>,
    #[serde(default)]
    fixes_path: Option<PathBuf>,
    #[serde(default)]
    artwork: Option<UmuArtwork>,
}

#[derive(Debug, Default, Deserialize)]
struct UmuInstallation {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    root: Option<PathBuf>,
    #[serde(default)]
    prefix: Option<PathBuf>,
    #[serde(default)]
    proton: Option<String>,
    #[serde(default)]
    manifest: Option<PathBuf>,
    #[serde(default)]
    confirmed: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct UmuLaunch {
    #[serde(default)]
    prefix_mode: Option<String>,
    #[serde(default)]
    requires_steam: Option<bool>,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    proton: Option<String>,
    #[serde(default)]
    prefix: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct UmuTarget {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    executable: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Default, Deserialize)]
struct UmuArtwork {
    #[serde(default)]
    cover: String,
    #[serde(default)]
    hero: String,
}

pub fn plan_umutron_library(path: impl AsRef<Path>) -> Result<ImportPlan, ImportError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|error| ImportError::io(path, error))?;
    let document: UmuDocument = serde_json::from_str(&text).map_err(|source| ImportError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    let mut plan = ImportPlan {
        source: Some(ImportSource::UmuTronLibrary),
        warnings: Vec::new(),
        ..ImportPlan::default()
    };
    let mut drives = BTreeSet::new();
    for source in document.games {
        if source.id.is_empty() || source.title.trim().is_empty() {
            plan.warnings.push("ignoring an entry without id/title".into());
            continue;
        }
        let mut installation = source.installation.unwrap_or_default();
        let Some(root) = installation.root.clone() else {
            plan.actions.push(ImportAction::NeedsInput {
                app_id: source.metadata_app_id,
                title: source.title.clone(),
                reason: "no installation folder recorded".into(),
            });
            continue;
        };
        if let Some((drive_root, _folder)) = derive_umu_layout(&root) {
            if drives.insert(drive_root.clone()) {
                plan.actions.push(ImportAction::AddDrive {
                    root: drive_root.clone(),
                    label: drive_root
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| drive_root.display().to_string()),
                });
            }
        } else {
            plan.warnings.push(format!(
                "{} is not inside an UmuLibrary layout; importing as linked",
                source.title
            ));
        }
        if installation.root.is_none() {
            installation.root = Some(root.clone());
        }
        let launch = source.launch.unwrap_or_default();
        let prefix = launch
            .prefix
            .clone()
            .or_else(|| installation.prefix.clone())
            .unwrap_or_default();
        let mut targets = Vec::new();
        for (index, target) in source.launch_targets.iter().enumerate() {
            targets.push(LaunchTarget {
                id: if target.id.is_empty() {
                    format!("t{}", index + 1)
                } else {
                    target.id.clone()
                },
                name: target.name.clone(),
                executable: target.executable.clone(),
                arguments: target.arguments.clone(),
                primary: target.primary || index == 0,
            });
        }
        let mode = match installation.mode.as_deref() {
            Some("managed") | None => InstallMode::Managed,
            Some("shortcut") => InstallMode::Shortcut,
            _ => InstallMode::Linked,
        };
        let game = Game {
            id: source.id.clone(),
            title: source.title.clone(),
            metadata_app_id: source.metadata_app_id,
            metadata_source: source.metadata_app_id.map(|id| MetadataSource {
                provider: "steam".into(),
                id,
            }),
            installation: Installation {
                mode,
                root,
                prefix,
                proton: installation
                    .proton
                    .or_else(|| launch.proton.clone())
                    .unwrap_or_default(),
                manifest: installation.manifest.unwrap_or_default(),
                confirmed: installation.confirmed.unwrap_or(true),
            },
            steam: source.metadata_app_id.map(|app_id| SteamRegistration {
                app_id,
                registered: false,
                backend: String::new(),
                state: RegistrationState::BackendMissing,
                registered_at: 0,
                launch_mode: if launch.requires_steam.unwrap_or(false) {
                    LaunchMode::SteamClient
                } else {
                    LaunchMode::Emulator
                },
            }),
            launch: LaunchSettings {
                prefix_mode: launch.prefix_mode.unwrap_or_else(|| "umu".into()),
                requires_steam: launch.requires_steam.unwrap_or(false),
                environment: launch.environment,
                arguments: launch.arguments,
                proton: launch.proton.unwrap_or_default(),
            },
            launch_targets: targets,
            fixes_enabled: source.fixes_enabled.unwrap_or(true),
            artwork: source
                .artwork
                .map(|art| Artwork {
                    cover: art.cover,
                    hero: art.hero,
                })
                .unwrap_or_default(),
        };
        let planned_layout = crate::library::game_layout(
            &game
                .installation
                .root
                .ancestors()
                .find(|ancestor| ancestor.file_name().and_then(|name| name.to_str()) == Some("UmuLibrary"))
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new("/")),
            &game,
        )
        .ok();
        if let Some(fixes) = source.fixes_path.filter(|path| path.is_dir()) {
            if let Some(layout) = planned_layout.filter(|layout| layout.fixes != fixes) {
                if !layout.fixes.exists() {
                    plan.actions.push(ImportAction::MoveFixes {
                        from: fixes,
                        to: layout.fixes,
                    });
                } else {
                    plan.warnings.push(format!(
                        "fixes for {} exist in two places; the canonical folder is kept",
                        game.title
                    ));
                }
            }
        }
        if let Some(app_id) = game.metadata_app_id {
            plan.actions.push(ImportAction::RegisterSteam {
                app_id,
                title: game.title.clone(),
            });
        }
        plan.actions.push(ImportAction::AddGame {
            planned: PlannedGame {
                game,
                source: ImportSource::UmuTronLibrary,
            },
        });
    }
    Ok(plan)
}

/// Return `(drive_root, game_folder)` for `<drive>/UmuLibrary/umuapps/common/<Game>`.
fn derive_umu_layout(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let umuapps = root.parent()?.parent()?;
    if umuapps.file_name().and_then(|name| name.to_str()) != Some("umuapps") {
        return None;
    }
    if umuapps.parent()?.file_name().and_then(|name| name.to_str()) != Some("UmuLibrary") {
        return None;
    }
    let drive = umuapps.parent()?.parent()?.to_path_buf();
    Some((drive, root.to_path_buf()))
}

/// Convert a plan into the library settings an import would apply.
pub fn imported_settings(settings: &UmuSettings) -> LibrarySettings {
    let mut result = LibrarySettings::default();
    if let Some(mode) = &settings.default_display_mode {
        if mode == "desktop" || mode == "fullscreen" {
            result.default_display_mode = mode.clone();
        }
    }
    if let Some(gamemode) = settings.gamemode {
        result.gamemode = gamemode;
    }
    result.picker_folders = settings
        .picker_folders
        .iter()
        .filter(|(key, _)| !key.is_empty() && key.len() <= 40)
        .take(64)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn legacy_config_plan_derives_root_and_registration() {
        let temp = tempfile::tempdir().unwrap();
        let game_root = temp.path().join("SteamLibrary/steamapps/common/Sample");
        fs::create_dir_all(&game_root).unwrap();
        let executable = game_root.join("Sample.exe");
        fs::write(&executable, b"exe").unwrap();
        let fixes = temp.path().join("fixes/42");
        fs::create_dir_all(&fixes).unwrap();
        let config = temp.path().join("config.json");
        fs::write(
            &config,
            serde_json::json!({
                "version": 1,
                "games": {
                    "42": {
                        "enabled": true,
                        "fixes": fixes,
                        "launch_targets": [
                            { "id": "t1", "executable": executable, "primary": true }
                        ]
                    }
                }
            })
            .to_string(),
        )
        .unwrap();
        let before = fs::read(&config).unwrap();

        let plan = plan_legacy_config(&config).unwrap();

        let game = plan.games().next().expect("one planned game");
        assert_eq!(game.game.metadata_app_id, Some(42));
        assert_eq!(game.game.title, "Sample");
        assert_eq!(game.game.installation.root, game_root);
        assert!(matches!(
            plan.actions.iter().find(|action| matches!(action, ImportAction::AddDrive { .. })),
            Some(ImportAction::AddDrive { .. })
        ));
        assert!(plan.registers().any(|(app_id, _)| app_id == 42));
        assert_eq!(fs::read(&config).unwrap(), before);
    }

    #[test]
    fn legacy_entry_without_a_library_root_needs_input() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("elsewhere/Sample.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"exe").unwrap();
        let config = temp.path().join("config.json");
        fs::write(
            &config,
            serde_json::json!({
                "version": 1,
                "games": { "7": { "enabled": true, "launch_targets": [
                    { "id": "t1", "executable": executable, "primary": true } ] } }
            })
            .to_string(),
        )
        .unwrap();

        let plan = plan_legacy_config(&config).unwrap();

        assert!(matches!(plan.actions[0], ImportAction::NeedsInput { .. }));
        assert!(plan.games().next().is_none());
    }

    #[test]
    fn umutron_library_plan_maps_layout_and_registration() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp
            .path()
            .join("UmuLibrary/umuapps/common/Sample Game");
        fs::create_dir_all(root.join("Bin")).unwrap();
        let executable = root.join("Bin/Sample.exe");
        fs::write(&executable, b"exe").unwrap();
        let prefix = temp.path().join("UmuLibrary/umuapps/compatdata/Sample Game/pfx");
        fs::create_dir_all(&prefix).unwrap();
        let legacy_fixes = temp.path().join("appdata/fixes/99");
        fs::create_dir_all(&legacy_fixes).unwrap();
        let library = temp.path().join("library.json");
        fs::write(
            &library,
            serde_json::json!({
                "version": 1,
                "settings": { "default_display_mode": "fullscreen", "gamemode": false },
                "games": [ {
                    "id": "game-1",
                    "title": "Sample Game",
                    "metadata_app_id": 99,
                    "installation": {
                        "mode": "linked",
                        "root": root,
                        "prefix": prefix,
                        "proton": "/runners/proton"
                    },
                    "launch": { "prefix_mode": "steam", "requires_steam": true },
                    "launch_targets": [ { "id": "t1", "executable": executable, "primary": true } ],
                    "fixes_path": legacy_fixes,
                    "fixes_enabled": true
                } ]
            })
            .to_string(),
        )
        .unwrap();
        let before = fs::read(&library).unwrap();

        let plan = plan_umutron_library(&library).unwrap();

        let game = plan.games().next().expect("one planned game");
        assert_eq!(game.game.metadata_app_id, Some(99));
        assert_eq!(game.game.launch.prefix_mode, "steam");
        assert!(game.game.launch.requires_steam);
        assert!(matches!(
            plan.actions.iter().find(|action| matches!(action, ImportAction::AddDrive { .. })),
            Some(ImportAction::AddDrive { .. })
        ));
        assert!(plan.actions.iter().any(|action| matches!(
            action,
            ImportAction::MoveFixes { from, .. } if from == &legacy_fixes
        )));
        assert!(plan.registers().any(|(app_id, _)| app_id == 99));
        assert_eq!(fs::read(&library).unwrap(), before);
        assert!(!root.join("Bin/Sample.exe").with_extension("tmp").exists());
    }

    #[test]
    fn umutron_entry_without_root_needs_input() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("library.json");
        fs::write(
            &library,
            serde_json::json!({
                "version": 1,
                "games": [ { "id": "g1", "title": "No Root", "metadata_app_id": 5 } ]
            })
            .to_string(),
        )
        .unwrap();

        let plan = plan_umutron_library(&library).unwrap();

        assert!(matches!(plan.actions[0], ImportAction::NeedsInput { .. }));
    }
}
