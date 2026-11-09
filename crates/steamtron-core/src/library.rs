//! Managed library v2: drives, games, launch settings, and Steam registration.
//!
//! This replaces the App-ID-keyed `config.json` for the Steam-free path. The
//! document is validated on load, mutated through typed setters, and written
//! atomically. Missing paths (an unplugged drive, a not-yet-created prefix) are
//! valid states; only unsafe or ambiguous data is rejected.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

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
    #[serde(default = "default_true")]
    pub gamemode: bool,
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
            gamemode: true,
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
        Ok(Self {
            path,
            settings: document.settings,
            drives: document.drives,
            games: document.games,
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

    pub fn save(&self) -> Result<(), LibraryError> {
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

fn validate(settings: &LibrarySettings, drives: &[Drive], games: &[Game]) -> Result<(), LibraryError> {
    if settings.default_display_mode != "desktop" && settings.default_display_mode != "fullscreen" {
        return Err(LibraryError::Invalid("invalid default display mode".into()));
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
        }
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
}
