//! Managed library v2: drives, games, launch settings, and Steam registration.
//!
//! This replaces the App-ID-keyed `config.json` for the Steam-free path. The
//! document is validated on load, mutated through typed setters, and written
//! atomically. Missing paths (an unplugged drive, a not-yet-created prefix) are
//! valid states; only unsafe or ambiguous data is rejected.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LIBRARY_VERSION: u32 = 2;
pub const MAX_DRIVES: usize = 64;
pub const MAX_GAMES: usize = 10_000;
pub const MAX_TARGETS: usize = 16;

#[derive(Debug, Error)]
pub enum LibraryError {
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
    #[error("invalid library: {0}")]
    Invalid(String),
}

impl LibraryError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    Emulator,
    SteamClient,
    Native,
}

impl Default for LaunchMode {
    fn default() -> Self {
        Self::Emulator
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMode {
    Managed,
    Linked,
    Shortcut,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataSource {
    pub provider: String,
    pub id: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artwork {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cover: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hero: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drive {
    pub id: String,
    pub root: PathBuf,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default)]
    pub default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installation {
    pub mode: InstallMode,
    pub root: PathBuf,
    #[serde(default, skip_serializing_if = "path_is_empty")]
    pub prefix: PathBuf,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub proton: String,
    #[serde(default, skip_serializing_if = "path_is_empty")]
    pub manifest: PathBuf,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistrationState {
    #[serde(rename = "registered")]
    Registered,
    #[serde(rename = "pending_restart")]
    PendingRestart,
    #[serde(rename = "client_stopped")]
    ClientStopped,
    #[serde(rename = "backend_missing")]
    BackendMissing,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteamRegistration {
    pub app_id: u32,
    #[serde(default)]
    pub registered: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub backend: String,
    #[serde(default = "default_registration_state")]
    pub state: RegistrationState,
    #[serde(default)]
    pub registered_at: u64,
    #[serde(default)]
    pub launch_mode: LaunchMode,
}

fn default_registration_state() -> RegistrationState {
    RegistrationState::BackendMissing
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSettings {
    #[serde(default = "default_prefix_mode")]
    pub prefix_mode: String,
    #[serde(default)]
    pub requires_steam: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub proton: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prelaunch: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prelaunch_sha256: String,
}

fn default_prefix_mode() -> String {
    "umu".into()
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self {
            prefix_mode: default_prefix_mode(),
            requires_steam: false,
            environment: BTreeMap::new(),
            arguments: Vec::new(),
            proton: String::new(),
            prelaunch: String::new(),
            prelaunch_sha256: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchTarget {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub executable: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_app_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_source: Option<MetadataSource>,
    pub installation: Installation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam: Option<SteamRegistration>,
    #[serde(default)]
    pub launch: LaunchSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launch_targets: Vec<LaunchTarget>,
    #[serde(default = "default_true")]
    pub fixes_enabled: bool,
    #[serde(default, skip_serializing_if = "Artwork::is_empty")]
    pub artwork: Artwork,
    /// Set once the game first becomes playable; adding metadata never sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_completed_at: Option<i64>,
}

/// A library folder with at least one entry is downloaded content.
pub fn installed_content(root: &Path) -> bool {
    root.is_dir()
        && std::fs::read_dir(root)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
}

/// Playable means downloaded content plus a configured primary executable.
pub fn game_ready(game: &Game) -> bool {
    if !installed_content(&game.installation.root) {
        return false;
    }
    game.launch_targets
        .iter()
        .find(|target| target.primary)
        .or_else(|| game.launch_targets.first())
        .is_some_and(|target| target.executable.is_file())
}

impl Game {
    /// Build the per-game settings manifest (the game's config file).
    ///
    /// This is the authoritative source for launch targets, Proton, prefix
    /// mode, environment, arguments, prelaunch, and fixes. `library.json`
    /// keeps the index and fields that are not per-game settings.
    pub fn appmanifest(&self) -> serde_json::Value {
        let root = &self.installation.root;
        let folder = root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("Game");
        let prefix_name = self
            .installation
            .prefix
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(folder);
        let primary = self
            .launch_targets
            .iter()
            .find(|target| target.primary)
            .or_else(|| self.launch_targets.first());
        let launcher_relative = primary
            .and_then(|target| target.executable.strip_prefix(root).ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let targets = self
            .launch_targets
            .iter()
            .map(|target| {
                serde_json::json!({
                    "id": target.id,
                    "name": target.name,
                    "executable": target
                        .executable
                        .strip_prefix(root)
                        .map(|path| path.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| target.executable.to_string_lossy().into_owned()),
                    "arguments": target.arguments,
                    "primary": target.primary,
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "format": 1,
            "game_id": self.id,
            "title": self.title,
            "steam_app_id": self.metadata_app_id,
            "content_directory": format!("common/{folder}"),
            "fixes_directory": format!("fixes/{folder}"),
            "launcher_relative_path": launcher_relative,
            "compatdata_directory": format!("compatdata/{prefix_name}"),
            "proton": self.launch.proton,
            "launch_options": self.launch.arguments,
            "launch_settings": {
                "environment": self.launch.environment,
                "prefix_mode": self.launch.prefix_mode,
                "proton": self.launch.proton,
                "requires_steam": self.launch.requires_steam,
            },
            "prelaunch": {
                "path": self.launch.prelaunch,
                "sha256": self.launch.prelaunch_sha256,
            },
            "fixes_enabled": self.fixes_enabled,
            "launcher_targets": targets,
        })
    }

    /// Apply every manifest setting except launcher targets.
    pub fn apply_appmanifest_settings(&mut self, manifest: &serde_json::Value) {
        let Some(base) = self.installation.manifest.parent().map(Path::to_path_buf) else {
            return;
        };
        self.apply_appmanifest_settings_at(manifest, &base);
    }

    /// Apply settings using an explicit library root for relative paths.
    ///
    /// Restore passes the local library root because the source manifest lives
    /// in the backup folder, not next to the installed game.
    pub fn apply_appmanifest_settings_at(
        &mut self,
        manifest: &serde_json::Value,
        base: &Path,
    ) {
        if let Some(title) = manifest
            .get("title")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            self.title = title.to_owned();
        }
        if let Some(app_id) = manifest
            .get("steam_app_id")
            .and_then(serde_json::Value::as_u64)
        {
            self.metadata_app_id = Some(app_id as u32);
        }
        if let Some(content) = manifest
            .get("content_directory")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
        {
            let root = rebase_with_prefix(&self.installation.root, base.join(content));
            if root != self.installation.root {
                self.installation.root = root;
            }
        }
        if let Some(prefix) = manifest
            .get("compatdata_directory")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
        {
            self.installation.prefix = rebase_with_prefix(
                &self.installation.prefix,
                base.join(prefix),
            );
        }

        let mut proton_applied = false;
        if let Some(settings) = manifest
            .get("launch_settings")
            .and_then(serde_json::Value::as_object)
        {
            if let Some(environment) = settings
                .get("environment")
                .and_then(serde_json::Value::as_object)
            {
                self.launch.environment = environment
                    .iter()
                    .filter_map(|(name, value)| {
                        value.as_str().map(|value| (name.clone(), value.to_owned()))
                    })
                    .collect();
            }
            if let Some(mode) = settings
                .get("prefix_mode")
                .and_then(serde_json::Value::as_str)
            {
                self.launch.prefix_mode = mode.to_owned();
            }
            if let Some(proton) = settings
                .get("proton")
                .and_then(serde_json::Value::as_str)
            {
                self.launch.proton = proton.to_owned();
                self.installation.proton = proton.to_owned();
                proton_applied = true;
            }
            if let Some(requires) = settings
                .get("requires_steam")
                .and_then(serde_json::Value::as_bool)
            {
                self.launch.requires_steam = requires;
            }
        }
        if !proton_applied {
            if let Some(proton) = manifest
                .get("proton")
                .and_then(serde_json::Value::as_str)
            {
                self.launch.proton = proton.to_owned();
                self.installation.proton = proton.to_owned();
            }
        }
        if let Some(options) = manifest
            .get("launch_options")
            .and_then(serde_json::Value::as_array)
        {
            self.launch.arguments = options
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect();
        }
        if let Some(prelaunch) = manifest
            .get("prelaunch")
            .and_then(serde_json::Value::as_object)
        {
            self.launch.prelaunch = prelaunch
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            self.launch.prelaunch_sha256 = prelaunch
                .get("sha256")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
        }
        if let Some(enabled) = manifest
            .get("fixes_enabled")
            .and_then(serde_json::Value::as_bool)
        {
            self.fixes_enabled = enabled;
        }
    }

    /// Apply the per-game settings manifest, including launcher targets.
    ///
    /// Only fields present in the manifest override the library copy, so an
    /// older partial manifest still loads. `launcher_targets` is canonical;
    /// an empty legacy `launch_targets` array is ignored so an old metadata
    /// snapshot cannot erase configured launchers.
    pub fn apply_appmanifest(&mut self, manifest: &serde_json::Value) {
        self.apply_appmanifest_settings(manifest);
        let canonical = manifest
            .get("launcher_targets")
            .and_then(serde_json::Value::as_array);
        let legacy = manifest
            .get("launch_targets")
            .and_then(serde_json::Value::as_array)
            .filter(|items| !items.is_empty());
        if let Some(items) = canonical.or(legacy) {
            let mut targets = Vec::new();
            for (index, item) in items.iter().enumerate() {
                let Some(relative) = item
                    .get("executable")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                let path = Path::new(relative);
                let executable = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    self.installation.root.join(path)
                };
                let name = item
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        executable
                            .file_stem()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Game")
                            .to_owned()
                    });
                targets.push(LaunchTarget {
                    id: item
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("t{}", index + 1)),
                    name,
                    executable,
                    arguments: item
                        .get("arguments")
                        .and_then(serde_json::Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(|value| value.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default(),
                    primary: item
                        .get("primary")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(index == 0),
                });
            }
            let mut claimed = false;
            for target in &mut targets {
                if target.primary && !claimed {
                    claimed = true;
                } else {
                    target.primary = false;
                }
            }
            if !claimed {
                if let Some(first) = targets.first_mut() {
                    first.primary = true;
                }
            }
            self.launch_targets = targets;
        } else if self.launch_targets.is_empty() {
            if let Some(relative) = manifest
                .get("launcher_relative_path")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
            {
                let path = Path::new(relative);
                let executable = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    self.installation.root.join(path)
                };
                let name = executable
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Game")
                    .to_owned();
                self.launch_targets = vec![LaunchTarget {
                    id: "t1".into(),
                    name,
                    executable,
                    arguments: Vec::new(),
                    primary: true,
                }];
            }
        }
    }
}

/// A SteamTron per-game config file, not a Steam `.acf` manifest.
fn is_managed_appmanifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("appmanifest_") && name.ends_with(".json"))
}

/// Keep the Windows extended-length prefix when the library already used one.
fn rebase_with_prefix(existing: &Path, candidate: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let existing_text = existing.to_string_lossy();
        let candidate_text = candidate.to_string_lossy();
        if existing_text.starts_with(r"\\?\") && !candidate_text.starts_with(r"\\?\") {
            return PathBuf::from(format!(r"\\?\{}", candidate_text));
        }
    }
    let _ = existing;
    candidate
}

/// Overlay the per-game config file, which is authoritative for settings.
fn apply_appmanifest_from_disk(game: &mut Game) {
    if !is_managed_appmanifest(&game.installation.manifest) {
        return;
    }
    let Ok(bytes) = fs::read(&game.installation.manifest) else {
        return;
    };
    let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    game.apply_appmanifest(&manifest);
}

/// Stamp the first ready transition; later edits keep the original time.
pub fn mark_setup_completed(game: &mut Game) {
    if game.setup_completed_at.is_none() && game_ready(game) {
        game.setup_completed_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs() as i64)
                .unwrap_or(0),
        );
    }
}

fn path_is_empty(path: &Path) -> bool {
    path.as_os_str().is_empty()
}

fn default_true() -> bool {
    true
}

impl Artwork {
    pub fn is_empty(&self) -> bool {
        self.cover.is_empty() && self.hero.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibrarySettings {
    #[serde(default = "default_display_mode")]
    pub default_display_mode: String,
    #[serde(default = "default_prefix_mode")]
    pub default_prefix_mode: String,
    #[serde(default = "default_true")]
    pub gamemode: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proton_runners: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_proton: String,
    #[serde(default)]
    pub picker_folders: BTreeMap<String, String>,
}

fn default_display_mode() -> String {
    "desktop".into()
}

impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            default_display_mode: default_display_mode(),
            default_prefix_mode: default_prefix_mode(),
            gamemode: true,
            proton_runners: Vec::new(),
            default_proton: String::new(),
            picker_folders: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    version: u32,
    #[serde(default)]
    settings: LibrarySettings,
    #[serde(default)]
    drives: Vec<Drive>,
    #[serde(default)]
    games: Vec<Game>,
}

pub struct Library {
    path: PathBuf,
    settings: LibrarySettings,
    drives: Vec<Drive>,
    games: Vec<Game>,
}

impl Library {
    pub fn default_path() -> PathBuf {
        crate::paths::layout().library()
    }

    /// Open a library file; a missing file is an empty library, not an error.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, LibraryError> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self {
                path,
                settings: LibrarySettings::default(),
                drives: Vec::new(),
                games: Vec::new(),
            });
        }
        let text = fs::read_to_string(&path).map_err(|error| LibraryError::io(&path, error))?;
        let document: Document = serde_json::from_str(&text).map_err(|source| LibraryError::Json {
            path: path.clone(),
            source,
        })?;
        if document.version != LIBRARY_VERSION {
            return Err(LibraryError::Invalid(format!(
                "unsupported library version {} (expected {LIBRARY_VERSION})",
                document.version
            )));
        }
        validate(&document.settings, &document.drives, &document.games)?;
        let mut games = document.games;
        for game in &mut games {
            normalize_install_mode(&document.drives, game);
            // The per-game config file wins over the library cache.
            apply_appmanifest_from_disk(game);
        }
        Ok(Self {
            path,
            settings: document.settings,
            drives: document.drives,
            games,
        })
    }

    pub fn load() -> Result<Self, LibraryError> {
        Self::open(Self::default_path())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn settings(&self) -> &LibrarySettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut LibrarySettings {
        &mut self.settings
    }

    pub fn drives(&self) -> &[Drive] {
        &self.drives
    }

    pub fn games(&self) -> &[Game] {
        &self.games
    }

    pub fn game(&self, id: &str) -> Option<&Game> {
        self.games.iter().find(|game| game.id == id)
    }

    pub fn game_by_app_id(&self, app_id: u32) -> Option<&Game> {
        self.games
            .iter()
            .filter(|game| game.metadata_app_id == Some(app_id))
            .min_by_key(|game| game.id.clone())
    }

    pub fn drive(&self, id: &str) -> Option<&Drive> {
        self.drives.iter().find(|drive| drive.id == id)
    }

    pub fn default_drive(&self) -> Option<&Drive> {
        self.drives
            .iter()
            .find(|drive| drive.default)
            .or_else(|| self.drives.first())
    }

    pub fn add_drive(&mut self, root: impl Into<PathBuf>, label: impl Into<String>, default: bool) -> Result<&Drive, LibraryError> {
        let root = root.into();
        if !root.is_absolute() || root.components().any(|part| part == std::path::Component::ParentDir) {
            return Err(LibraryError::Invalid("drive root must be an absolute, normalized path".into()));
        }
        if self.drives.iter().any(|drive| drive.root == root) {
            return Err(LibraryError::Invalid(format!("drive already registered: {}", root.display())));
        }
        if self.drives.len() >= MAX_DRIVES {
            return Err(LibraryError::Invalid("too many drives".into()));
        }
        if default {
            for drive in &mut self.drives {
                drive.default = false;
            }
        }
        let mut index = self.drives.len() + 1;
        while self.drives.iter().any(|drive| drive.id == format!("d{index}")) {
            index += 1;
        }
        self.drives.push(Drive {
            id: format!("d{index}"),
            root,
            label: label.into(),
            default,
        });
        Ok(self.drives.last().expect("drive was just pushed"))
    }

    pub fn upsert_game(&mut self, game: Game) -> Result<(), LibraryError> {
        let mut candidate = self.games.clone();
        match candidate.iter_mut().find(|existing| existing.id == game.id) {
            Some(existing) => *existing = game,
            None => {
                if candidate.len() >= MAX_GAMES {
                    return Err(LibraryError::Invalid("too many games".into()));
                }
                candidate.push(game);
            }
        }
        validate(&self.settings, &self.drives, &candidate)?;
        self.games = candidate;
        Ok(())
    }

    pub fn remove_game(&mut self, id: &str) -> Option<Game> {
        let index = self.games.iter().position(|game| game.id == id)?;
        Some(self.games.remove(index))
    }

    pub fn set_picker_folder(&mut self, key: &str, folder: impl Into<String>) -> Result<(), LibraryError> {
        if !valid_key(key) {
            return Err(LibraryError::Invalid("invalid picker key".into()));
        }
        let folder = folder.into();
        if folder.len() > 4096 || folder.contains('\0') || folder.contains('\n') {
            return Err(LibraryError::Invalid("invalid picker folder".into()));
        }
        if !Path::new(&folder).is_dir() {
            return Ok(());
        }
        self.settings.picker_folders.insert(key.to_owned(), folder);
        while self.settings.picker_folders.len() > 64 {
            if let Some(first) = self.settings.picker_folders.keys().next().cloned() {
                self.settings.picker_folders.remove(&first);
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Rewrite entries that still point inside UmuTron's library to SteamTron's
    /// own layout. Content is not moved: the entries become uninstalled and can
    /// be downloaded into `SteamTronLibrary/steamtronapps`.
    pub fn detach_umutron_paths(&mut self) -> usize {
        let mut changed = 0usize;
        for mut game in self.games().to_vec() {
        let root = game.installation.root.to_string_lossy().to_string();
        let needs = root.contains("/UmuLibrary/")
            || root.contains("/umutron/")
            || game.installation.proton.contains("/umutron/")
            || game.launch.proton.contains("/umutron/")
            || game
                .launch_targets
                .iter()
                .any(|target| target.executable.to_string_lossy().contains("/UmuLibrary/"));
        if !needs {
            continue;
        }
        let Some(drive) = drive_root_for(&game.installation.root) else { continue };
        let base = steamtronapps_root(&drive);
        let folder = safe_folder(&game.title).unwrap_or_else(|_| game.id.clone());
        game.installation.mode = InstallMode::Managed;
        game.installation.root = base.join("common").join(&folder);
        game.installation.prefix = base.join("compatdata").join(&folder);
        game.installation.manifest = base.join(format!(
            "appmanifest_{}.json",
            game.metadata_app_id
                .map(|app_id| app_id.to_string())
                .unwrap_or_else(|| game.id.clone())
        ));
        game.installation.proton.clear();
        game.installation.confirmed = false;
        game.launch.proton.clear();
        game.launch_targets.clear();
            if self.upsert_game(game).is_ok() {
                changed += 1;
            }
        }
        changed
    }

pub fn remove_drive(&mut self, id: &str) -> Result<Option<Drive>, LibraryError> {
        let Some(index) = self.drives.iter().position(|drive| drive.id == id) else {
            return Err(LibraryError::Invalid(format!("unknown drive: {id}")));
        };
        let was_default = self.drives[index].default;
        let removed = self.drives.remove(index);
        if was_default {
            if let Some(next) = self.drives.first_mut() {
                next.default = true;
            }
        }
        Ok(Some(removed))
    }

pub fn set_default_drive(&mut self, id: &str) -> Result<(), LibraryError> {
        if !self.drives.iter().any(|drive| drive.id == id) {
            return Err(LibraryError::Invalid(format!("unknown drive: {id}")));
        }
        for drive in &mut self.drives {
            drive.default = drive.id == id;
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), LibraryError> {
        // Write every game's config file first: it is authoritative, so a
        // failure here must not leave library.json claiming a new value.
        for game in &self.games {
            if !is_managed_appmanifest(&game.installation.manifest) {
                continue;
            }
            let path = &game.installation.manifest;
            let mut payload = fs::read(path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .filter(serde_json::Value::is_object)
                .unwrap_or_else(|| serde_json::json!({}));
            let generated = game.appmanifest();
            if let (Some(target), Some(source)) =
                (payload.as_object_mut(), generated.as_object())
            {
                for (key, value) in source {
                    target.insert(key.clone(), value.clone());
                }
            }
            let content = serde_json::to_string_pretty(&payload).map_err(|source| {
                LibraryError::Json {
                    path: path.clone(),
                    source,
                }
            })?;
            write_atomic(path, content.as_bytes())?;
        }
        self.save_index()
    }

    /// Write only the library index, leaving per-game config files untouched.
    ///
    /// Metadata import uses this so an imported appmanifest keeps its exact
    /// bytes; the next `open` overlays it as the authoritative config.
    pub fn save_index(&self) -> Result<(), LibraryError> {
        let document = Document {
            version: LIBRARY_VERSION,
            settings: self.settings.clone(),
            drives: self.drives.clone(),
            games: self.games.clone(),
        };
        let content = serde_json::to_string_pretty(&document).map_err(|source| LibraryError::Json {
            path: self.path.clone(),
            source,
        })?;
        write_atomic(&self.path, content.as_bytes())?;
        Ok(())
    }
}

/// Replace a file atomically; falls back to remove-then-rename on platforms
/// where renaming over an existing file is refused.
pub fn write_atomic(path: &Path, content: &[u8]) -> Result<(), LibraryError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| LibraryError::io(parent, error))?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, content).map_err(|error| LibraryError::io(&temporary, error))?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(first) => {
            if path.exists() {
                fs::remove_file(path).map_err(|error| LibraryError::io(path, error))?;
                fs::rename(&temporary, path).map_err(|error| LibraryError::io(path, error))
            } else {
                let _ = fs::remove_file(&temporary);
                Err(LibraryError::io(path, first))
            }
        }
    }
}

const MAX_LIBRARY_FILE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct LibraryFileSummary {
    pub path: String,
    pub games: usize,
    pub drives: usize,
    pub bytes: u64,
}

/// Copy the whole library document to `destination` without changing it.
pub fn export_library_file(destination: &Path) -> Result<LibraryFileSummary, LibraryError> {
    export_library_file_from(&Library::default_path(), destination)
}

fn export_library_file_from(source: &Path, destination: &Path) -> Result<LibraryFileSummary, LibraryError> {
    let library = Library::open(source)?;
    if !source.is_file() {
        return Err(LibraryError::Invalid("No library file to export yet.".into()));
    }
    let data = fs::read(source).map_err(|error| LibraryError::io(source, error))?;
    write_atomic(destination, &data)?;
    Ok(LibraryFileSummary {
        path: destination.display().to_string(),
        games: library.games().len(),
        drives: library.drives().len(),
        bytes: data.len() as u64,
    })
}

/// Validate a library document and make it the active library. The current
/// document is kept next to it as `library.json.bak` before the replacement.
pub fn import_library_file(source: &Path) -> Result<LibraryFileSummary, LibraryError> {
    import_library_file_to(source, &Library::default_path())
}

fn import_library_file_to(source: &Path, destination: &Path) -> Result<LibraryFileSummary, LibraryError> {
    if !source.is_file() {
        return Err(LibraryError::Invalid(
            "Choose a SteamTron library JSON file.".into(),
        ));
    }
    let size = fs::metadata(source)
        .map_err(|error| LibraryError::io(source, error))?
        .len();
    if size > MAX_LIBRARY_FILE {
        return Err(LibraryError::Invalid("Library file is too large.".into()));
    }
    let data = fs::read(source).map_err(|error| LibraryError::io(source, error))?;
    let document: Document = serde_json::from_slice(&data).map_err(|error| LibraryError::Json {
        path: source.to_path_buf(),
        source: error,
    })?;
    if document.version != LIBRARY_VERSION {
        return Err(LibraryError::Invalid(format!(
            "unsupported library version {} (expected {LIBRARY_VERSION})",
            document.version
        )));
    }
    validate(&document.settings, &document.drives, &document.games)?;
    if destination.is_file() {
        let backup = destination.with_extension("json.bak");
        fs::copy(destination, &backup).map_err(|error| LibraryError::io(&backup, error))?;
    }
    write_atomic(destination, &data)?;
    Ok(LibraryFileSummary {
        path: destination.display().to_string(),
        games: document.games.len(),
        drives: document.drives.len(),
        bytes: data.len() as u64,
    })
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 40
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Canonical managed folder for a game title (invalid characters become spaces).
pub fn safe_folder(title: &str) -> Result<String, LibraryError> {
    let mut cleaned = String::with_capacity(title.len());
    for character in title.chars() {
        let unsafe_char = character.is_control()
            || matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*');
        cleaned.push(if unsafe_char { ' ' } else { character });
    }
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|character| character == '.' || character == ' ');
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return Err(LibraryError::Invalid("game title has no safe folder name".into()));
    }
    Ok(trimmed.chars().take(120).collect())
}

/// `<drive>/SteamTronLibrary/steamtronapps` — SteamTron's managed layout.
///
/// It intentionally mirrors UmuTron's `UmuLibrary/umuapps` structure without
/// sharing the folders, so both applications can own the same drive.
pub fn steamtronapps_root(drive_root: &Path) -> PathBuf {
    drive_root.join("SteamTronLibrary").join("steamtronapps")
}

/// Derive managed vs linked from where the content actually lives.
///
/// A restored or moved game has its content under
/// `<drive>/SteamTronLibrary/steamtronapps/common`, so a stale `linked` mode
/// must not hide Uninstall or block the uninstall guard.
fn normalize_install_mode(drives: &[Drive], game: &mut Game) {
    if game.installation.root.as_os_str().is_empty() {
        return;
    }
    let managed = drives.iter().any(|drive| {
        crate::paths::path_within(
            &game.installation.root,
            &steamtronapps_root(&drive.root).join("common"),
        )
    });
    game.installation.mode = if managed {
        InstallMode::Managed
    } else {
        InstallMode::Linked
    };
}

/// Drive root that owns a managed or imported library folder.
///
/// Walks up from a game root until it finds `SteamTronLibrary` or UmuTron's
/// `UmuLibrary`, then returns the folder that contains it.
pub fn drive_root_for(root: &Path) -> Option<PathBuf> {
    root.ancestors().find_map(|ancestor| {
        matches!(
            ancestor.file_name().and_then(|name| name.to_str()),
            Some("SteamTronLibrary" | "UmuLibrary")
        )
        .then(|| ancestor.parent().map(Path::to_path_buf))
        .flatten()
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameLayout {
    pub base: PathBuf,
    pub content: PathBuf,
    pub prefix: PathBuf,
    pub fixes: PathBuf,
    pub manifest: PathBuf,
}

/// Fixes directory for a game.
///
/// Managed games keep fixes beside their content. Linked games also use that
/// folder when their prefix or manifest already lives in a managed library, so
/// a Steam-installed game catalogued by SteamTron shares the same
/// `SteamTronLibrary` fixes tree as managed games. Only games with no managed
/// footprint fall back to the app-data folder keyed by App ID.
pub fn fixes_dir(game: &Game) -> Result<PathBuf, LibraryError> {
    let drive = [
        &game.installation.root,
        &game.installation.prefix,
        &game.installation.manifest,
    ]
    .into_iter()
    .find_map(|path| drive_root_for(path));
    if let Some(drive) = drive {
        return game_layout(&drive, game).map(|layout| layout.fixes);
    }
    Ok(legacy_fixes_dir(game))
}

/// App-data fixes folder used when a game has no managed library footprint.
pub fn legacy_fixes_dir(game: &Game) -> PathBuf {
    let app_id = game
        .metadata_app_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| game.id.clone());
    crate::paths::layout().home().join("fixes").join(app_id)
}

/// Resolve the fixes folder and adopt fixes left in the app-data fallback once
/// the game maps to a managed library. Best effort: I/O problems leave the
/// legacy folder untouched and still return the resolved target.
pub fn migrate_legacy_fixes(game: &Game) -> Result<PathBuf, LibraryError> {
    let target = fixes_dir(game)?;
    let legacy = legacy_fixes_dir(game);
    if target != legacy && !target.exists() && legacy.is_dir() {
        adopt_fixes(&legacy, &target);
    }
    Ok(target)
}

fn adopt_fixes(legacy: &Path, target: &Path) {
    let Some(parent) = target.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() || fs::rename(legacy, target).is_ok() {
        return;
    }
    let Some(folder) = target.file_name() else {
        return;
    };
    let mut staging_name = std::ffi::OsString::from(".");
    staging_name.push(folder);
    staging_name.push(format!(".adopt-{}", std::process::id()));
    let staging = parent.join(staging_name);
    let _ = fs::remove_dir_all(&staging);
    if copy_fixes_tree(legacy, &staging).is_err() {
        let _ = fs::remove_dir_all(&staging);
        return;
    }
    if fs::rename(&staging, target).is_ok() {
        let _ = fs::remove_dir_all(legacy);
    } else {
        let _ = fs::remove_dir_all(&staging);
    }
}

fn copy_fixes_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let metadata = fs::symlink_metadata(&source)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let destination = to.join(entry.file_name());
        if metadata.is_dir() {
            copy_fixes_tree(&source, &destination)?;
        } else if metadata.is_file() {
            fs::copy(&source, &destination)?;
        }
    }
    Ok(())
}

pub fn game_layout(drive_root: &Path, game: &Game) -> Result<GameLayout, LibraryError> {
    let base = steamtronapps_root(drive_root);
    let folder = safe_folder(&game.title)?;
    let manifest_name = game
        .metadata_app_id
        .map(|app_id| format!("appmanifest_{app_id}.json"))
        .unwrap_or_else(|| format!("appmanifest_{}.json", game.id));
    Ok(GameLayout {
        content: base.join("common").join(&folder),
        prefix: base.join("compatdata").join(&folder),
        fixes: base.join("fixes").join(&folder),
        manifest: base.join(manifest_name),
        base,
    })
}

/// Local SteamTron copy of the game's Lua manifest.
///
/// Added games keep this next to their managed content so downloads,
/// verification, updates, and DLC detection work without a Steam install.
pub fn managed_lua_path(game: &Game) -> Option<PathBuf> {
    let app_id = game.metadata_app_id?;
    let base = game.installation.manifest.parent()?;
    Some(base.join("lua").join(format!("{app_id}.lua")))
}

fn validate(settings: &LibrarySettings, drives: &[Drive], games: &[Game]) -> Result<(), LibraryError> {
    if settings.default_display_mode != "desktop" && settings.default_display_mode != "fullscreen" {
        return Err(LibraryError::Invalid("invalid default display mode".into()));
    }
    if settings.default_prefix_mode != "umu" && settings.default_prefix_mode != "steam" {
        return Err(LibraryError::Invalid("invalid default prefix source".into()));
    }
    if settings.proton_runners.len() > 32 {
        return Err(LibraryError::Invalid("too many Proton runners".into()));
    }
    for runner in &settings.proton_runners {
        let path = Path::new(runner);
        if runner.len() > 4096 || !path.is_absolute() || has_parent(path) {
            return Err(LibraryError::Invalid(format!(
                "invalid Proton runner path: {runner}"
            )));
        }
    }
    if !settings.default_proton.is_empty() {
        let path = Path::new(&settings.default_proton);
        if !path.is_absolute() || has_parent(path) {
            return Err(LibraryError::Invalid(
                "default Proton runner must be an absolute path".into(),
            ));
        }
    }
    if settings.picker_folders.len() > 64 {
        return Err(LibraryError::Invalid("too many remembered picker folders".into()));
    }
    for (key, value) in &settings.picker_folders {
        if !valid_key(key) || value.len() > 4096 || value.contains('\0') {
            return Err(LibraryError::Invalid("invalid remembered picker folder".into()));
        }
    }
    if drives.len() > MAX_DRIVES {
        return Err(LibraryError::Invalid("too many drives".into()));
    }
    let mut drive_ids = BTreeSet::new();
    let mut roots = BTreeSet::new();
    let mut defaults = 0;
    for drive in drives {
        if !valid_id(&drive.id) || !drive_ids.insert(drive.id.clone()) {
            return Err(LibraryError::Invalid(format!("invalid or duplicate drive id: {}", drive.id)));
        }
        if !drive.root.is_absolute() || has_parent(&drive.root) {
            return Err(LibraryError::Invalid(format!(
                "drive root must be absolute: {}",
                drive.root.display()
            )));
        }
        if !roots.insert(drive.root.clone()) {
            return Err(LibraryError::Invalid(format!(
                "duplicate drive root: {}",
                drive.root.display()
            )));
        }
        if drive.default {
            defaults += 1;
        }
    }
    if defaults > 1 {
        return Err(LibraryError::Invalid("more than one default drive".into()));
    }
    if games.len() > MAX_GAMES {
        return Err(LibraryError::Invalid("too many games".into()));
    }
    let mut game_ids = BTreeSet::new();
    let mut app_ids = BTreeSet::new();
    for game in games {
        if !valid_id(&game.id) || !game_ids.insert(game.id.clone()) {
            return Err(LibraryError::Invalid(format!("invalid or duplicate game id: {}", game.id)));
        }
        if game.title.trim().is_empty() || game.title.chars().count() > 200 || game.title.contains('\0') {
            return Err(LibraryError::Invalid(format!("invalid title for game {}", game.id)));
        }
        if let Some(app_id) = game.metadata_app_id {
            if app_id == 0 || !app_ids.insert(app_id) {
                return Err(LibraryError::Invalid(format!("invalid or duplicate App ID: {app_id}")));
            }
        }
        validate_installation(&game.installation)?;
        validate_launch(&game.launch)?;
        validate_targets(&game.launch_targets)?;
        if let Some(steam) = &game.steam {
            if game.metadata_app_id.is_some() && game.metadata_app_id != Some(steam.app_id) {
                return Err(LibraryError::Invalid(format!(
                    "game {} has two different App IDs",
                    game.id
                )));
            }
            if steam.app_id == 0 {
                return Err(LibraryError::Invalid("invalid Steam App ID".into()));
            }
        }
    }
    Ok(())
}

fn validate_installation(installation: &Installation) -> Result<(), LibraryError> {
    if !installation.root.is_absolute() || has_parent(&installation.root) {
        return Err(LibraryError::Invalid(format!(
            "installation root must be absolute: {}",
            installation.root.display()
        )));
    }
    if !installation.prefix.as_os_str().is_empty()
        && (!installation.prefix.is_absolute() || has_parent(&installation.prefix))
    {
        return Err(LibraryError::Invalid(format!(
            "prefix must be absolute: {}",
            installation.prefix.display()
        )));
    }
    Ok(())
}

fn validate_launch(launch: &LaunchSettings) -> Result<(), LibraryError> {
    if launch.prefix_mode != "umu" && launch.prefix_mode != "steam" {
        return Err(LibraryError::Invalid("prefix mode must be umu or steam".into()));
    }
    if launch.prelaunch.is_empty() != launch.prelaunch_sha256.is_empty() {
        return Err(LibraryError::Invalid(
            "pre-launch script and its approved SHA-256 must be set together".into(),
        ));
    }
    if !launch.prelaunch.is_empty()
        && (!std::path::Path::new(&launch.prelaunch).is_absolute()
            || launch.prelaunch.len() > 4096
            || launch.prelaunch.contains('\0'))
    {
        return Err(LibraryError::Invalid("invalid pre-launch script path".into()));
    }
    if launch.environment.len() > 32 {
        return Err(LibraryError::Invalid("too many environment variables".into()));
    }
    let mut total = 0usize;
    for (key, value) in &launch.environment {
        if !valid_env_name(key)
            || value.len() > 1024
            || value.contains('\0')
            || value.contains('\n')
            || value.contains('\r')
        {
            return Err(LibraryError::Invalid(format!("invalid environment entry: {key}")));
        }
        total += key.len() + value.len();
    }
    if total > 8192 {
        return Err(LibraryError::Invalid("environment exceeds 8 KiB".into()));
    }
    if launch.arguments.len() > 128 {
        return Err(LibraryError::Invalid("too many launch arguments".into()));
    }
    Ok(())
}

fn validate_targets(targets: &[LaunchTarget]) -> Result<(), LibraryError> {
    if targets.len() > MAX_TARGETS {
        return Err(LibraryError::Invalid("too many launch targets".into()));
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut primaries = 0;
    for target in targets {
        if !valid_id(&target.id) || !ids.insert(target.id.clone()) {
            return Err(LibraryError::Invalid(format!("invalid or duplicate target id: {}", target.id)));
        }
        if !target.executable.is_absolute() || has_parent(&target.executable) {
            return Err(LibraryError::Invalid(format!(
                "launch target must be absolute: {}",
                target.executable.display()
            )));
        }
        if !paths.insert(target.executable.clone()) {
            return Err(LibraryError::Invalid(format!(
                "duplicate launch target: {}",
                target.executable.display()
            )));
        }
        if target.primary {
            primaries += 1;
        }
    }
    if primaries > 1 {
        return Err(LibraryError::Invalid("more than one primary launch target".into()));
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_env_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn has_parent(path: &Path) -> bool {
    path.components().any(|part| part == std::path::Component::ParentDir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_game(id: &str, title: &str, root: PathBuf) -> Game {
        Game {
            id: id.into(),
            title: title.into(),
            metadata_app_id: None,
            metadata_source: None,
            installation: Installation {
                mode: InstallMode::Managed,
                root,
                prefix: PathBuf::new(),
                proton: String::new(),
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

    #[test]
    fn derives_managed_mode_from_the_content_root() {
        let drive = Drive {
            id: "d1".into(),
            root: PathBuf::from("/mnt/drive"),
            label: String::new(),
            default: true,
        };
        let managed_root = steamtronapps_root(&drive.root)
            .join("common")
            .join("Game");
        let mut game = sample_game("g1", "Game", managed_root);
        game.installation.mode = InstallMode::Linked;
        normalize_install_mode(std::slice::from_ref(&drive), &mut game);
        assert_eq!(game.installation.mode, InstallMode::Managed);

        let mut external = sample_game("g2", "External", PathBuf::from("/mnt/external/Game"));
        external.installation.mode = InstallMode::Managed;
        normalize_install_mode(std::slice::from_ref(&drive), &mut external);
        assert_eq!(external.installation.mode, InstallMode::Linked);
    }

    #[test]
    fn setup_completion_requires_downloaded_content_and_a_target() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("game");
        let mut game = sample_game("42", "Game", root.clone());
        mark_setup_completed(&mut game);
        assert!(game.setup_completed_at.is_none());

        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("game.exe"), b"exe").unwrap();
        game.launch_targets = vec![LaunchTarget {
            id: "t1".into(),
            name: "Game".into(),
            executable: root.join("game.exe"),
            arguments: Vec::new(),
            primary: true,
        }];
        mark_setup_completed(&mut game);
        let first = game.setup_completed_at.expect("ready transition is stamped");
        assert!(first > 0);
        mark_setup_completed(&mut game);
        assert_eq!(game.setup_completed_at, Some(first));
    }

    #[test]
    fn missing_file_is_an_empty_library() {
        let temp = tempfile::tempdir().unwrap();
        let library = Library::open(temp.path().join("library.json")).unwrap();
        assert!(library.games().is_empty());
        assert!(library.drives().is_empty());
    }

    #[test]
    fn round_trips_drives_and_games_through_an_atomic_write() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library.json");
        let mut library = Library::open(&path).unwrap();
        library.add_drive(temp.path(), "Temp", true).unwrap();
        let game = sample_game("game-1", "Sample", temp.path().join("game"));
        library.upsert_game(game).unwrap();
        library.save().unwrap();

        let reopened = Library::open(&path).unwrap();
        assert_eq!(reopened.drives().len(), 1);
        assert_eq!(reopened.games()[0].title, "Sample");
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn exports_and_imports_the_library_document() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("backup/library.json");
        let destination = temp.path().join("data/library.json");
        let mut library = Library::open(&source).unwrap();
        library.add_drive(temp.path(), "Temp", true).unwrap();
        library
            .upsert_game(sample_game("game-1", "Sample", temp.path().join("game")))
            .unwrap();
        library.save().unwrap();

        let exported = export_library_file_from(&source, &destination).unwrap();
        assert_eq!(exported.games, 1);
        assert_eq!(exported.drives, 1);
        assert_eq!(fs::read(&source).unwrap(), fs::read(&destination).unwrap());

        // Change the active document, then restore the exported bytes.
        let mut changed = Library::open(&destination).unwrap();
        changed
            .upsert_game(sample_game("game-2", "Other", temp.path().join("other")))
            .unwrap();
        changed.save().unwrap();
        let changed_bytes = fs::read(&destination).unwrap();

        let imported = import_library_file_to(&source, &destination).unwrap();
        assert_eq!(imported.games, 1);
        assert_eq!(fs::read(&source).unwrap(), fs::read(&destination).unwrap());
        assert_eq!(
            fs::read(destination.with_extension("json.bak")).unwrap(),
            changed_bytes
        );
    }

    #[test]
    fn invalid_import_leaves_the_active_library_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("bad.json");
        let destination = temp.path().join("data/library.json");
        let mut library = Library::open(&destination).unwrap();
        library.add_drive(temp.path(), "Temp", true).unwrap();
        library.save().unwrap();
        let before = fs::read(&destination).unwrap();

        fs::write(&source, b"{ not json").unwrap();
        assert!(import_library_file_to(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), before);

        fs::write(&source, br#"{"version":999,"settings":{},"drives":[],"games":[]}"#).unwrap();
        assert!(import_library_file_to(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), before);
    }

    #[test]
    fn rejects_duplicate_app_ids_and_unsafe_paths() {
        let temp = tempfile::tempdir().unwrap();
        let mut library = Library::open(temp.path().join("library.json")).unwrap();
        let mut first = sample_game("game-1", "One", temp.path().join("one"));
        first.metadata_app_id = Some(42);
        library.upsert_game(first).unwrap();

        let mut second = sample_game("game-2", "Two", temp.path().join("two"));
        second.metadata_app_id = Some(42);
        assert!(library.upsert_game(second).is_err());

        let mut escaping = sample_game("game-3", "Three", PathBuf::from("../escape"));
        assert!(library.upsert_game(escaping.clone()).is_err());
        escaping.installation.root = temp.path().join("three");
        library.upsert_game(escaping).unwrap();
    }

    #[test]
    fn safe_folder_sanitizes_titles() {
        assert_eq!(safe_folder("Lies of P").unwrap(), "Lies of P");
        assert_eq!(safe_folder("A/B: C?").unwrap(), "A B C");
        assert!(safe_folder("...").is_err());
        assert!(safe_folder("   ").is_err());
    }



    #[test]
    fn default_drive_is_unique() {
        let temp = tempfile::tempdir().unwrap();
        let mut library = Library::open(temp.path().join("library.json")).unwrap();
        assert!(library.add_drive("/mnt/one", "one", true).is_ok());
        assert!(library.add_drive("/mnt/two", "two", false).is_ok());
        let second = library.drives()[1].id.clone();
        library.set_default_drive(&second).unwrap();
        assert_eq!(
            library.drives().iter().filter(|drive| drive.default).count(),
            1
        );
        assert_eq!(library.default_drive().unwrap().id, second);
        assert!(library.set_default_drive("missing").is_err());
    }

    #[test]
    fn drive_root_is_found_for_both_library_names() {
        assert_eq!(
            drive_root_for(Path::new("/mnt/games/SteamTronLibrary/steamtronapps/common/Sample")),
            Some(PathBuf::from("/mnt/games"))
        );
        assert_eq!(
            drive_root_for(Path::new("/mnt/games/UmuLibrary/umuapps/common/Sample")),
            Some(PathBuf::from("/mnt/games"))
        );
        assert_eq!(drive_root_for(Path::new("/mnt/games/common/Sample")), None);
    }

    #[test]
    fn game_layout_uses_the_shared_umuapps_tree() {
        let temp = tempfile::tempdir().unwrap();
        let mut game = sample_game("game-1", "Sample", temp.path().join("game"));
        game.metadata_app_id = Some(42);
        let layout = game_layout(temp.path(), &game).unwrap();
        assert_eq!(layout.base, steamtronapps_root(temp.path()));
        assert_eq!(layout.content, layout.base.join("common").join("Sample"));
        assert_eq!(layout.fixes, layout.base.join("fixes").join("Sample"));
        assert_eq!(layout.manifest, layout.base.join("appmanifest_42.json"));
    }

    #[test]
    fn linked_game_with_managed_footprint_uses_the_library_fixes_folder() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp
            .path()
            .join("SteamLibrary/steamapps/common/Red Dead Redemption 2");
        let base = temp.path().join("SteamTronLibrary/steamtronapps");
        let mut game = sample_game("1174180", "Red Dead Redemption 2", root);
        game.installation.mode = InstallMode::Linked;
        game.installation.prefix = base.join("compatdata/Red Dead Redemption 2");
        game.installation.manifest = base.join("appmanifest_1174180.json");
        assert_eq!(
            fixes_dir(&game).unwrap(),
            base.join("fixes/Red Dead Redemption 2")
        );
        assert_eq!(
            legacy_fixes_dir(&game),
            crate::paths::layout().home().join("fixes/1174180")
        );
    }

    #[test]
    fn linked_game_without_a_managed_footprint_uses_app_data() {
        let temp = tempfile::tempdir().unwrap();
        let mut game = sample_game(
            "1174180",
            "Red Dead Redemption 2",
            temp.path().join("SteamLibrary/steamapps/common/Red Dead Redemption 2"),
        );
        game.installation.mode = InstallMode::Linked;
        game.installation.manifest = temp
            .path()
            .join("SteamLibrary/steamapps/appmanifest_1174180.acf");
        assert_eq!(fixes_dir(&game).unwrap(), legacy_fixes_dir(&game));
    }

    #[test]
    fn legacy_fixes_are_adopted_into_the_managed_folder() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("appdata/fixes/42");
        let target = temp
            .path()
            .join("SteamTronLibrary/steamtronapps/fixes/Game");
        fs::create_dir_all(legacy.join("sub")).unwrap();
        fs::write(legacy.join("sub/a.dll"), b"alpha").unwrap();
        adopt_fixes(&legacy, &target);
        assert_eq!(fs::read(target.join("sub/a.dll")).unwrap(), b"alpha");
        assert!(!legacy.exists());
    }

    #[test]
    fn settings_and_picker_folders_persist() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library.json");
        let picker = temp.path().join("exports");
        fs::create_dir_all(&picker).unwrap();
        {
            let mut library = Library::open(&path).unwrap();
            library.settings_mut().default_display_mode = "fullscreen".into();
            library
                .set_picker_folder("fixes_export", picker.to_string_lossy())
                .unwrap();
            library.save().unwrap();
        }
        let reopened = Library::open(&path).unwrap();
        assert_eq!(reopened.settings().default_display_mode, "fullscreen");
        assert_eq!(
            reopened.settings().picker_folders.get("fixes_export").map(String::as_str),
            Some(picker.to_string_lossy().as_ref())
        );
    }

    fn configured_game(root: &Path, manifest: &Path) -> Game {
        let mut game = sample_game("42", "Configured", root.to_path_buf());
        game.metadata_app_id = Some(42);
        game.installation.manifest = manifest.to_path_buf();
        game.installation.prefix = manifest.parent().unwrap().join("compatdata/Configured");
        game.launch.proton = "/opt/proton/GE".into();
        game.launch.arguments = vec!["-windowed".into()];
        game.launch.environment.insert("WINEDLLOVERRIDES".into(), "x=n".into());
        game.launch.prefix_mode = "steam".into();
        game.launch.requires_steam = true;
        game.launch.prelaunch = "/tmp/prelaunch.sh".into();
        game.launch.prelaunch_sha256 = "abc".into();
        game.fixes_enabled = false;
        game.launch_targets = vec![
            LaunchTarget {
                id: "play".into(),
                name: "Play".into(),
                executable: root.join("Play.exe"),
                arguments: vec!["-nolauncher".into()],
                primary: true,
            },
            LaunchTarget {
                id: "game".into(),
                name: "Game".into(),
                executable: root.join("Game.exe"),
                arguments: Vec::new(),
                primary: false,
            },
        ];
        game
    }

    #[test]
    fn appmanifest_round_trips_every_game_setting() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("common/Configured");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Play.exe"), b"MZ").unwrap();
        fs::write(root.join("Game.exe"), b"MZ").unwrap();
        let manifest_path = temp.path().join("appmanifest_42.json");
        let game = configured_game(&root, &manifest_path);

        let manifest = game.appmanifest();
        assert_eq!(manifest["launcher_targets"][0]["executable"], "Play.exe");
        assert_eq!(manifest["launch_settings"]["prefix_mode"], "steam");
        assert_eq!(manifest["prelaunch"]["path"], "/tmp/prelaunch.sh");
        assert_eq!(manifest["fixes_enabled"], false);

        let mut restored = sample_game("42", "Placeholder", root.clone());
        restored.installation.manifest = manifest_path.clone();
        restored.apply_appmanifest(&manifest);
        assert_eq!(restored.title, "Configured");
        assert_eq!(restored.launch.proton, "/opt/proton/GE");
        assert_eq!(restored.launch.arguments, vec!["-windowed"]);
        assert_eq!(restored.launch.prefix_mode, "steam");
        assert!(restored.launch.requires_steam);
        assert_eq!(restored.launch.prelaunch, "/tmp/prelaunch.sh");
        assert!(!restored.fixes_enabled);
        assert_eq!(restored.launch_targets.len(), 2);
        assert_eq!(restored.launch_targets[0].executable, root.join("Play.exe"));
        assert!(restored.launch_targets[0].primary);
    }

    #[test]
    fn library_open_overlays_the_per_game_config() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("SteamTronLibrary/steamtronapps/common/Configured");
        let base = temp.path().join("SteamTronLibrary/steamtronapps");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Play.exe"), b"MZ").unwrap();
        let manifest_path = base.join("appmanifest_42.json");
        let library_path = temp.path().join("library.json");

        {
            let mut library = Library::open(&library_path).unwrap();
            library.add_drive(temp.path(), "test", true).unwrap();
            let mut cached = sample_game("42", "Library Cache", root.clone());
            cached.installation.manifest = manifest_path.clone();
            library.upsert_game(cached).unwrap();
            library.save_index().unwrap();
        }
        let manifest = serde_json::json!({
            "format": 1,
            "game_id": "42",
            "title": "Manifest Config",
            "steam_app_id": 42,
            "content_directory": "common/Configured",
            "launch_options": ["-manifest"],
            "launch_settings": {
                "environment": {"WINEDLLOVERRIDES": "manifest=n"},
                "prefix_mode": "steam",
                "proton": "/manifest/proton",
                "requires_steam": true
            },
            "prelaunch": {"path": "/manifest/prelaunch.sh", "sha256": "def"},
            "fixes_enabled": false,
            "launcher_targets": [
                {"id": "play", "name": "Play", "executable": "Play.exe", "primary": true}
            ]
        });
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();

        let library = Library::open(&library_path).unwrap();
        let game = library.game("42").unwrap();
        assert_eq!(game.title, "Manifest Config");
        assert_eq!(game.launch.arguments, vec!["-manifest"]);
        assert_eq!(game.launch.prefix_mode, "steam");
        assert_eq!(game.launch.proton, "/manifest/proton");
        assert!(game.launch.requires_steam);
        assert_eq!(game.launch.prelaunch, "/manifest/prelaunch.sh");
        assert!(!game.fixes_enabled);
        assert_eq!(game.launch_targets[0].executable, root.join("Play.exe"));
    }

    #[test]
    fn save_leaves_a_steam_acf_alone() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("SteamLibrary/steamapps/common/Sample");
        fs::create_dir_all(&root).unwrap();
        let acf = temp.path().join("SteamLibrary/steamapps/appmanifest_42.acf");
        let original = b"\"AppState\"\n{\n\t\"appid\"\t\t\"42\"\n}\n";
        fs::write(&acf, original).unwrap();
        let mut library = Library::open(temp.path().join("library.json")).unwrap();
        library.add_drive(temp.path(), "test", true).unwrap();
        let mut game = sample_game("42", "Sample", root);
        game.installation.mode = InstallMode::Linked;
        game.installation.manifest = acf.clone();
        library.upsert_game(game).unwrap();
        library.save().unwrap();

        assert_eq!(fs::read(&acf).unwrap(), original);
    }

    #[test]
    fn library_save_writes_the_per_game_config() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("SteamTronLibrary/steamtronapps/common/Configured");
        let base = temp.path().join("SteamTronLibrary/steamtronapps");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Play.exe"), b"MZ").unwrap();
        let manifest_path = base.join("appmanifest_42.json");
        let mut library = Library::open(temp.path().join("library.json")).unwrap();
        library.add_drive(temp.path(), "test", true).unwrap();
        let game = configured_game(&root, &manifest_path);
        library.upsert_game(game).unwrap();
        library.save().unwrap();

        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["launch_settings"]["prefix_mode"], "steam");
        assert_eq!(manifest["launch_options"][0], "-windowed");
        assert_eq!(manifest["prelaunch"]["path"], "/tmp/prelaunch.sh");
        assert_eq!(manifest["launcher_targets"][0]["name"], "Play");
        assert_eq!(manifest["launcher_targets"][0]["executable"], "Play.exe");
    }
}
