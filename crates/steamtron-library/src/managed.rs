//! Managed-library add flow: manifest download, registration, library entry.
//!
//! Adding a game never downloads content. It registers the App ID with the
//! platform backend, writes the managed library entry, and reports the exact
//! registration state so the UI can ask for a Steam restart.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use steamtron_core::library::{
    Artwork, Game, Installation, InstallMode, LaunchMode, LaunchSettings, Library, MetadataSource,
    RegistrationState, SteamRegistration, game_layout, managed_lua_path,
};
use steamtron_register::{RegistrationBackend, RegistrationRequest};

use crate::{
    Bundle, LibraryError, MAX_ARCHIVE, canonical_lua, lua_declares_appid, normalize_appid,
    sanitize_title, windows_only_lua,
};

/// Where the Lua manifest comes from.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestProvider {
    /// Signed-in UMUTron API (Ryuu, then Luie, then SteamML).
    Api,
    /// Public SteamML archive; no account required.
    SteamMl,
}

impl ManifestProvider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::SteamMl => "steamml",
        }
    }
}

const STEAMML_URL: &str = "https://pub-5b6d3b7c03fd4ac1afb5bd3017850e20.r2.dev/{appid}.zip";

/// Download a Lua manifest archive from SteamML, reusing the global ZIP cache.
pub fn download_steamml(appid: &str) -> Result<Bundle, LibraryError> {
    if normalize_appid(appid).as_deref() != Some(appid) {
        return Err(LibraryError::InvalidAppId);
    }
    if let Some(bundle) = crate::read_persistent_bundle(appid) {
        if lua_declares_appid(&bundle.lua, appid) {
            return Ok(bundle);
        }
    }
    if let Some(bundle) = crate::read_cached_manifest(appid, "steamml") {
        if lua_declares_appid(&bundle.lua, appid) {
            if let Ok(bytes) = fs::read(crate::manifest_cache_path(appid, "steamml")) {
                crate::store_persistent_bundle(appid, &bytes);
            }
            return Ok(bundle);
        }
        crate::invalidate_cached_manifest(appid, "steamml");
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    let response = client
        .get(STEAMML_URL.replace("{appid}", appid))
        .header("User-Agent", "steamtron-desktop")
        .send()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    if !response.status().is_success() {
        return Err(LibraryError::Download(format!("HTTP {}", response.status())));
    }
    let bytes = response
        .bytes()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    if bytes.len() as u64 > MAX_ARCHIVE {
        return Err(LibraryError::Archive("archive is too large".into()));
    }
    let mut bundle = crate::parse_bundle(&bytes)?;
    bundle.lua = canonical_lua(&bundle.lua);
    if !lua_declares_appid(&bundle.lua, appid) {
        return Err(LibraryError::Archive(
            "Lua manifest is for a different App ID".into(),
        ));
    }
    crate::store_persistent_bundle(appid, &bytes);
    crate::store_cached_manifest(appid, "steamml", &bytes);
    Ok(bundle)
}

pub fn fetch_manifest_bundle(
    appid: &str,
    provider: ManifestProvider,
) -> Result<Bundle, LibraryError> {
    match provider {
        ManifestProvider::Api => crate::download_bundle(appid),
        ManifestProvider::SteamMl => download_steamml(appid),
    }
}


#[derive(Clone, Debug, Serialize)]
pub struct ReconcileReport {
    pub checked: usize,
    pub registered: usize,
    pub pending: usize,
    pub errors: Vec<String>,
}



#[derive(Clone, Debug, Serialize)]
pub struct FixFile {
    pub name: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct FixesView {
    pub path: String,
    pub files: Vec<FixFile>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FixesRemoved {
    pub path: String,
    pub files: usize,
}

fn fixes_dir_for(game: &Game) -> Result<PathBuf, LibraryError> {
    let drive = steamtron_core::library::drive_root_for(&game.installation.root)
        .ok_or_else(|| LibraryError::Invalid("game is not inside a library folder".into()))?;
    steamtron_core::library::game_layout(&drive, game)
        .map(|layout| layout.fixes)
        .map_err(|error| LibraryError::Invalid(error.to_string()))
}

fn app_id_for(game: &Game) -> String {
    game.metadata_app_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| game.id.clone())
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<FixFile>) -> std::io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if files.len() >= 5000 {
            return Ok(());
        }
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            files.push(FixFile {
                name,
                size: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            });
        }
    }
    Ok(())
}

pub fn fixes_view_at(game_id: &str, library_path: &Path) -> Result<FixesView, LibraryError> {
    let library =
        Library::open(library_path).map_err(|error| LibraryError::Invalid(error.to_string()))?;
    let game = library
        .game(game_id)
        .ok_or_else(|| LibraryError::Invalid(format!("unknown game {game_id}")))?;
    let dir = fixes_dir_for(game)?;
    let mut files = Vec::new();
    if dir.is_dir() {
        collect_files(&dir, &dir, &mut files)
            .map_err(|error| LibraryError::Io { path: dir.clone(), source: error })?;
    }
    files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(FixesView {
        path: dir.display().to_string(),
        files,
    })
}

pub fn install_fixes_at(
    game_id: &str,
    zip_path: &Path,
    library_path: &Path,
) -> Result<FixesView, LibraryError> {
    let library =
        Library::open(library_path).map_err(|error| LibraryError::Invalid(error.to_string()))?;
    let game = library
        .game(game_id)
        .ok_or_else(|| LibraryError::Invalid(format!("unknown game {game_id}")))?;
    let dir = fixes_dir_for(game)?;
    steamtron_core::fixes::install_zip_into(&app_id_for(game), zip_path, &dir)
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    fixes_view_at(game_id, library_path)
}

pub fn remove_fixes_at(game_id: &str, library_path: &Path) -> Result<FixesRemoved, LibraryError> {
    let view = fixes_view_at(game_id, library_path)?;
    let dir = PathBuf::from(&view.path);
    let files = view.files.len();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|error| LibraryError::Io { path: dir.clone(), source: error })?;
    }
    Ok(FixesRemoved {
        path: view.path,
        files,
    })
}

pub fn fixes_view(game_id: &str) -> Result<FixesView, LibraryError> {
    fixes_view_at(game_id, &Library::default_path())
}

pub fn install_fixes(game_id: &str, zip_path: &Path) -> Result<FixesView, LibraryError> {
    install_fixes_at(game_id, zip_path, &Library::default_path())
}

pub fn remove_fixes(game_id: &str) -> Result<FixesRemoved, LibraryError> {
    remove_fixes_at(game_id, &Library::default_path())
}


#[derive(Clone, Debug, Deserialize)]
pub struct SetTargetRequest {
    pub game_id: String,
    pub executable: PathBuf,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub proton: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TargetReport {
    pub game_id: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub proton: String,
    pub name: String,
}

/// Record the primary launch target for a managed game.
pub fn set_launch_target_at(
    request: SetTargetRequest,
    library_path: &Path,
) -> Result<TargetReport, LibraryError> {
    let mut library =
        Library::open(library_path).map_err(|error| LibraryError::Invalid(error.to_string()))?;
    let game = library
        .game(&request.game_id)
        .cloned()
        .ok_or_else(|| LibraryError::Invalid(format!("unknown game {}", request.game_id)))?;
    let root = &game.installation.root;
    let executable = if request.executable.is_absolute() {
        request.executable.clone()
    } else {
        root.join(&request.executable)
    };
    if !executable.is_file() {
        return Err(LibraryError::Invalid(format!(
            "executable does not exist: {}",
            executable.display()
        )));
    }
    if !executable.starts_with(root) {
        return Err(LibraryError::Invalid(
            "executable must be inside the game folder".into(),
        ));
    }
    if !request.proton.trim().is_empty() && !Path::new(request.proton.trim()).exists() {
        return Err(LibraryError::Invalid(format!(
            "Proton runner does not exist: {}",
            request.proton.trim()
        )));
    }
    let name = if request.name.trim().is_empty() {
        executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Game")
            .to_owned()
    } else {
        request.name.trim().to_owned()
    };
    let mut updated = game;
    updated.launch_targets = vec![steamtron_core::library::LaunchTarget {
        id: "t1".into(),
        name: name.clone(),
        executable: executable.clone(),
        arguments: request.arguments.clone(),
        primary: true,
    }];
    if !request.proton.trim().is_empty() {
        updated.launch.proton = request.proton.trim().to_owned();
        updated.installation.proton = request.proton.trim().to_owned();
    }
    steamtron_core::library::mark_setup_completed(&mut updated);
    library
        .upsert_game(updated)
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    library
        .save()
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    Ok(TargetReport {
        game_id: request.game_id,
        executable: executable.display().to_string(),
        arguments: request.arguments,
        proton: request.proton.trim().to_owned(),
        name,
    })
}

pub fn set_launch_target(request: SetTargetRequest) -> Result<TargetReport, LibraryError> {
    set_launch_target_at(request, &Library::default_path())
}
#[derive(Clone, Debug, Deserialize)]
pub struct LinkLauncher {
    #[serde(default)]
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LinkGameRequest {
    pub game_id: String,
    pub root: String,
    pub launchers: Vec<LinkLauncher>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkReport {
    pub game_id: String,
    pub root: String,
    pub launchers: usize,
    pub primary: String,
}

/// Link an installed folder and its launchers, mirroring UmuTron Game Link.
pub fn link_game(request: LinkGameRequest) -> Result<LinkReport, LibraryError> {
    link_game_at(request, &Library::default_path())
}

pub fn link_game_at(
    request: LinkGameRequest,
    library_path: &Path,
) -> Result<LinkReport, LibraryError> {
    use steamtron_core::library::LaunchTarget;

    let game_id = request.game_id.clone();
    let mut library =
        Library::open(library_path).map_err(|error| LibraryError::Invalid(error.to_string()))?;
    let previous = library
        .game(&game_id)
        .cloned()
        .ok_or_else(|| LibraryError::Invalid(format!("unknown game {}", request.game_id)))?;
    let raw_root = request.root.trim();
    if raw_root.is_empty() {
        return Err(LibraryError::Invalid("Choose the installed game folder.".into()));
    }
    let root_path = Path::new(raw_root);
    if !root_path.is_absolute() || !root_path.is_dir() {
        return Err(LibraryError::Invalid(
            "The selected game folder does not exist.".into(),
        ));
    }
    let root = root_path
        .canonicalize()
        .map_err(|error| LibraryError::Invalid(format!("{}: {error}", root_path.display())))?;
    if request.launchers.is_empty() {
        return Err(LibraryError::Invalid(
            "Add at least one launcher executable.".into(),
        ));
    }
    let mut targets = Vec::with_capacity(request.launchers.len());
    for (index, launcher) in request.launchers.into_iter().enumerate() {
        let candidate = if Path::new(&launcher.executable).is_absolute() {
            PathBuf::from(&launcher.executable)
        } else {
            root.join(&launcher.executable)
        };
        let executable = candidate.canonicalize().map_err(|error| {
            LibraryError::Invalid(format!("{}: {error}", candidate.display()))
        })?;
        if !executable.is_file() {
            return Err(LibraryError::Invalid(format!(
                "executable does not exist: {}",
                executable.display()
            )));
        }
        if !executable.starts_with(&root) {
            return Err(LibraryError::Invalid(
                "Choose a launcher inside the linked game folder.".into(),
            ));
        }
        let name = if launcher.name.trim().is_empty() {
            executable
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Launcher")
                .to_owned()
        } else {
            launcher.name.trim().to_owned()
        };
        let id = previous
            .launch_targets
            .iter()
            .find(|target| target.executable == executable)
            .map(|target| target.id.clone())
            .unwrap_or_else(|| format!("t{}", index + 1));
        targets.push(LaunchTarget {
            id,
            name,
            executable,
            arguments: launcher.arguments,
            primary: launcher.primary,
        });
    }
    if !targets.iter().any(|target| target.primary) {
        if let Some(first) = targets.first_mut() {
            first.primary = true;
        }
    }
    let mut primary = true;
    for target in &mut targets {
        if target.primary && primary {
            primary = false;
        } else if target.primary {
            target.primary = false;
        }
    }
    let primary_name = targets
        .iter()
        .find(|target| target.primary)
        .map(|target| target.name.clone())
        .unwrap_or_default();
    let mut updated = previous;
    let changed_root = updated.installation.root != root;
    updated.installation.root = root.clone();
    updated.installation.mode = if changed_root {
        InstallMode::Linked
    } else {
        updated.installation.mode
    };
    updated.installation.confirmed = true;
    updated.launch_targets = targets;
    steamtron_core::library::mark_setup_completed(&mut updated);
    library
        .upsert_game(updated)
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    library
        .save()
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    let launcher_count = library
        .game(&game_id)
        .map(|game| game.launch_targets.len())
        .unwrap_or_default();
    Ok(LinkReport {
        game_id,
        root: root.display().to_string(),
        launchers: launcher_count,
        primary: primary_name,
    })
}


/// Steam client route: the game needs an App ID and a verified registration.
pub fn steam_ready(game: &Game, backend: &dyn RegistrationBackend) -> Result<u32, LibraryError> {
    let app_id = game
        .metadata_app_id
        .ok_or_else(|| LibraryError::Invalid("this game has no Steam App ID".into()))?;
    let state = backend
        .verify(&app_id.to_string())
        .map_err(|error| LibraryError::Backend(error.to_string()))?;
    if !state.registered {
        return Err(LibraryError::Backend(format!(
            "App {app_id} is not registered with {}. Re-add the game or open Settings → Doctor.",
            backend.id()
        )));
    }
    Ok(app_id)
}

/// Verify every stored Steam registration and update pending states.
pub fn reconcile_registrations(
    backend: &dyn RegistrationBackend,
) -> Result<ReconcileReport, LibraryError> {
    reconcile_registrations_at(&Library::default_path(), backend)
}

pub fn reconcile_registrations_at(
    library_path: &Path,
    backend: &dyn RegistrationBackend,
) -> Result<ReconcileReport, LibraryError> {
    let mut library = Library::open(library_path)
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    let mut report = ReconcileReport {
        checked: 0,
        registered: 0,
        pending: 0,
        errors: Vec::new(),
    };
    let mut changed = false;
    for game in library.games().to_vec() {
        if game.steam.is_none() {
            continue;
        }
        report.checked += 1;
        let app_id = game
            .steam
            .as_ref()
            .map(|registration| registration.app_id)
            .unwrap_or_default();
        match backend.verify(&app_id.to_string()) {
            Ok(state) if state.registered => {
                report.registered += 1;
                if game
                    .steam
                    .as_ref()
                    .is_some_and(|registration| registration.state != RegistrationState::Registered)
                {
                    let mut updated = game.clone();
                    if let Some(registration) = updated.steam.as_mut() {
                        registration.registered = true;
                        registration.state = RegistrationState::Registered;
                    }
                    library
                        .upsert_game(updated)
                        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
                    changed = true;
                }
            }
            Ok(_) => {
                report.pending += 1;
            }
            Err(error) => report.errors.push(format!("{app_id}: {error}")),
        }
    }
    if changed {
        library
            .save()
            .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    }
    Ok(report)
}

#[derive(Clone, Debug, Deserialize)]
pub struct ManagedAddRequest {
    pub app_id: u32,
    #[serde(default)]
    pub title: String,
    /// Drive root (`/mnt/games`), not the library folder.
    pub drive_root: PathBuf,
    #[serde(default)]
    pub drive_label: String,
    pub provider: ManifestProvider,
    /// Optional Proton runner path for the first Play.
    #[serde(default)]
    pub proton: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ManagedAdded {
    pub game_id: String,
    pub title: String,
    pub drive: String,
    pub root: String,
    pub prefix: String,
    pub manifest: String,
    pub fixes: String,
    pub lua: String,
    pub backend: String,
    pub provider: &'static str,
    pub registered: bool,
    pub pending_restart: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Remove legacy per-library Lua files once their global bundle exists.
pub fn cleanup_legacy_lua_files() {
    let Ok(library) = Library::load() else { return };
    let mut directories = std::collections::BTreeSet::new();
    for game in library.games() {
        let Some(appid) = game.metadata_app_id else { continue };
        let Some(path) = managed_lua_path(game) else { continue };
        if !path.is_file() { continue; }
        if crate::read_persistent_bundle(&appid.to_string()).is_some() {
            let _ = fs::remove_file(&path);
            if let Some(parent) = path.parent() {
                directories.insert(parent.to_path_buf());
            }
        }
    }
    for directory in directories {
        let _ = fs::remove_dir(directory);
    }
}

/// Rebuild the per-game manifest folders from the persistent bundles and
/// remove flat copies that now live inside a game folder.
pub fn organize_manifest_store() {
    let root = steamtron_core::paths::layout().manifests();
    if !root.is_dir() { return; }
    let bundles = steamtron_core::paths::layout().home().join("manifest-bundles");
    if let Ok(entries) = fs::read_dir(&bundles) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(appid) = path.file_stem().and_then(|value| value.to_str()) else { continue };
            let Some(bundle) = crate::read_bundle_file(&path) else { continue };
            let directory = root.join(appid);
            let _ = fs::create_dir_all(&directory);
            for (name, data) in &bundle.manifests {
                let safe = Path::new(name).file_name().and_then(|value| value.to_str()).unwrap_or("manifest");
                let _ = fs::write(directory.join(safe), data);
            }
        }
    }
    let Ok(entries) = fs::read_dir(&root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() { continue; }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else { continue };
        if let Some(appid) = name.split('.').next().filter(|value| value.chars().all(|c| c.is_ascii_digit())) {
            let directory = root.join(appid);
            let _ = fs::create_dir_all(&directory);
            let _ = fs::rename(&path, directory.join(name));
            continue;
        }
        let organized = fs::read_dir(&root).ok().into_iter().flatten().flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .any(|directory| directory.join(name).is_file());
        if organized { let _ = fs::remove_file(&path); }
    }
}

/// Store provider depot manifests in SteamTron's Steam-independent store.
pub fn write_managed_manifests(
    appid: &str,
    manifests: &BTreeMap<String, Vec<u8>>,
) -> Result<(), LibraryError> {
    if manifests.is_empty() {
        return Ok(());
    }
    let directory = steamtron_core::paths::layout().manifests_for(appid);
    fs::create_dir_all(&directory).map_err(|error| LibraryError::io(&directory, error))?;
    for (name, data) in manifests {
        let safe = Path::new(name)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("manifest");
        let path = directory.join(safe);
        fs::write(&path, data).map_err(|error| LibraryError::io(&path, error))?;
    }
    Ok(())
}

/// Write the managed appmanifest JSON when it is missing.
pub fn ensure_appmanifest(game: &Game) -> Result<bool, LibraryError> {
    let path = &game.installation.manifest;
    if path.as_os_str().is_empty() || path.is_file() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| LibraryError::io(parent, error))?;
    }
    let data = serde_json::to_vec_pretty(&crate::backup::game_appmanifest(game))
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    let temporary = path.with_extension("json.steamtron.tmp");
    fs::write(&temporary, data).map_err(|error| LibraryError::io(&temporary, error))?;
    fs::rename(&temporary, path).map_err(|error| LibraryError::io(path, error))?;
    Ok(true)
}

/// Populate appmanifest JSON for every existing game that lacks it.
pub fn populate_missing_appmanifests() -> usize {
    let Ok(library) = Library::load() else { return 0 };
    let mut written = 0;
    for game in library.games() {
        if ensure_appmanifest(game).unwrap_or(false) {
            written += 1;
        }
    }
    written
}

/// Add one game to the managed library. Steam registration is attempted only
/// when a ready backend is supplied.
pub fn add_managed_game(
    request: ManagedAddRequest,
    backend: Option<&dyn RegistrationBackend>,
) -> Result<ManagedAdded, LibraryError> {
    let bundle = fetch_manifest_bundle(&request.app_id.to_string(), request.provider)?;
    add_managed_game_with_bundle(request, bundle, backend)
}

/// Same as [`add_managed_game`] with an already downloaded bundle.
pub fn add_managed_game_with_bundle(
    request: ManagedAddRequest,
    bundle: Bundle,
    backend: Option<&dyn RegistrationBackend>,
) -> Result<ManagedAdded, LibraryError> {
    add_managed_game_at(request, bundle, backend, &Library::default_path())
}

/// Same as [`add_managed_game_with_bundle`] against an explicit library file.
pub fn add_managed_game_at(
    request: ManagedAddRequest,
    bundle: Bundle,
    backend: Option<&dyn RegistrationBackend>,
    library_path: &Path,
) -> Result<ManagedAdded, LibraryError> {
    let appid = request.app_id.to_string();
    if !request.drive_root.is_absolute() {
        return Err(LibraryError::Invalid(
            "drive root must be an absolute path".into(),
        ));
    }
    let backend = backend.filter(|backend| {
        backend
            .status()
            .map(|status| status.ready)
            .unwrap_or(false)
    });
    let mut library = Library::open(library_path)
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    if library.game(&appid).is_some() || library.game_by_app_id(request.app_id).is_some() {
        return Err(LibraryError::Invalid(format!(
            "App {} is already in the library",
            request.app_id
        )));
    }
    if !lua_declares_appid(&bundle.lua, &appid) {
        return Err(LibraryError::Archive(
            "Lua manifest is for a different App ID".into(),
        ));
    }
    let raw_title = if request.title.trim().is_empty() {
        crate::title_for_appid(&appid).unwrap_or_else(|| format!("Steam App {appid}"))
    } else {
        request.title.clone()
    };
    let title = sanitize_title(&raw_title);

    let drive_root = request.drive_root.clone();
    if !library.drives().iter().any(|drive| drive.root == drive_root) {
        let label = if request.drive_label.trim().is_empty() {
            drive_root.display().to_string()
        } else {
            request.drive_label.trim().to_owned()
        };
        library
            .add_drive(drive_root.clone(), label, library.drives().is_empty())
            .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    }

    let mut game = Game {
        id: appid.clone(),
        title,
        metadata_app_id: Some(request.app_id),
        metadata_source: Some(MetadataSource {
            provider: "steam".into(),
            id: request.app_id,
        }),
        installation: Installation {
            mode: InstallMode::Managed,
            root: PathBuf::new(),
            prefix: PathBuf::new(),
            proton: request.proton.clone(),
            manifest: PathBuf::new(),
            confirmed: false,
        },
        steam: None,
        launch: LaunchSettings {
            prefix_mode: library.settings().default_prefix_mode.clone(),
            proton: request.proton.clone(),
            ..LaunchSettings::default()
        },
        launch_targets: Vec::new(),
        fixes_enabled: true,
        artwork: Artwork::default(),
        setup_completed_at: None,
    };
    let layout = game_layout(&drive_root, &game)
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    game.installation.root = layout.content.clone();
    game.installation.prefix = layout.prefix.clone();
    game.installation.manifest = layout.manifest.clone();
    ensure_appmanifest(&game)?;

    // Depot filtering is best-effort: a Steam metadata outage must not block
    // registration, so the provider Lua is kept unchanged in that case.
    let lua = windows_only_lua(&appid, &bundle.lua)
        .unwrap_or_else(|_| canonical_lua(&bundle.lua));
    let manifest_dir = steamtron_core::paths::layout().manifests_for(&appid);
    fs::create_dir_all(&manifest_dir).map_err(|error| LibraryError::io(&manifest_dir, error))?;
    for (name, data) in &bundle.manifests {
        let safe = Path::new(name)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("manifest");
        let path = manifest_dir.join(safe);
        fs::write(&path, data).map_err(|error| LibraryError::io(&path, error))?;
    }

    let mut registered = false;
    let mut pending_restart = false;
    let mut backend_id = String::new();
    let mut lua_result = format!("manifest-bundles/{appid}.zip");
    let mut warning = None;
    if let Some(backend) = backend {
        match backend.register(&RegistrationRequest {
            app_id: &appid,
            title: &game.title,
            lua: &lua,
            manifests: &bundle.manifests,
        }) {
            Ok(registration) => match backend.verify(&appid) {
                Ok(state) if state.registered => {
                    game.steam = Some(SteamRegistration {
                        app_id: request.app_id,
                        registered: true,
                        backend: backend.id().to_owned(),
                        state: RegistrationState::PendingRestart,
                        registered_at: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|elapsed| elapsed.as_secs())
                            .unwrap_or(0),
                        launch_mode: LaunchMode::Emulator,
                    });
                    registered = true;
                    pending_restart = true;
                    backend_id = registration.backend;
                    lua_result = registration.lua_path.display().to_string();
                }
                Ok(_) => {
                    warning = Some(
                        "Steam did not keep the registration; added to the SteamTron library only."
                            .into(),
                    );
                }
                Err(error) => {
                    warning = Some(format!(
                        "Steam registration could not be verified; added to the SteamTron library only: {error}"
                    ));
                }
            },
            Err(error) => {
                warning = Some(format!(
                    "Steam registration failed; added to the SteamTron library only: {error}"
                ));
            }
        }
    } else {
        warning = Some("Steam was not detected; added to the SteamTron library only.".into());
    }

    library
        .upsert_game(game.clone())
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;
    library
        .save()
        .map_err(|error| LibraryError::Invalid(error.to_string()))?;

    Ok(ManagedAdded {
        game_id: game.id,
        title: game.title,
        drive: drive_root.display().to_string(),
        root: layout.content.display().to_string(),
        prefix: layout.prefix.display().to_string(),
        manifest: layout.manifest.display().to_string(),
        fixes: layout.fixes.display().to_string(),
        lua: lua_result,
        backend: backend_id,
        provider: request.provider.id(),
        registered,
        pending_restart,
        warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use steamtron_register::{
        BackendStatus, Registration, RegistrationError, RegistrationState as BackendRegistrationState,
    };

    struct MemoryBackend {
        root: PathBuf,
    }

    impl RegistrationBackend for MemoryBackend {
        fn id(&self) -> &'static str {
            "memory"
        }
        fn status(&self) -> Result<BackendStatus, RegistrationError> {
            Ok(BackendStatus {
                platform: "test",
                backend: self.id(),
                installed: true,
                ready: true,
                steam_root: self.root.display().to_string(),
                lua_dir: self.root.display().to_string(),
                details: String::new(),
            })
        }
        fn register(
            &self,
            request: &RegistrationRequest<'_>,
        ) -> Result<Registration, RegistrationError> {
            fs::create_dir_all(&self.root).unwrap();
            let path = self.root.join(format!("{}.lua", request.app_id));
            fs::write(&path, request.lua).unwrap();
            Ok(Registration {
                app_id: request.app_id.to_owned(),
                title: request.title.to_owned(),
                backend: self.id().to_owned(),
                lua_path: path,
                manifests: Vec::new(),
            })
        }
        fn verify(&self, app_id: &str) -> Result<BackendRegistrationState, RegistrationError> {
            let path = self.root.join(format!("{app_id}.lua"));
            Ok(BackendRegistrationState {
                app_id: app_id.to_owned(),
                backend: self.id().to_owned(),
                registered: path.is_file(),
                lua_path: path.display().to_string(),
            })
        }
        fn unregister(&self, app_id: &str) -> Result<(), RegistrationError> {
            let path = self.root.join(format!("{app_id}.lua"));
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
            Ok(())
        }
    }

    fn sample_bundle(appid: &str) -> Bundle {
        Bundle {
            lua: format!(
                "addappid({appid}, 1, \"key\")\nsetManifestid(1, \"2\", 3)\n"
            ),
            manifests: BTreeMap::new(),
        }
    }

    #[test]
    fn adds_a_managed_game_and_registers_it() {
        let temp = tempfile::tempdir().unwrap();
        let library_path = temp.path().join("library.json");
        let backend = MemoryBackend {
            root: temp.path().join("steam"),
        };
        let bundle = sample_bundle("42");
        let report = add_managed_game_at(
            ManagedAddRequest {
                app_id: 42,
                title: "Sample Game".into(),
                drive_root: temp.path().join("drive"),
                drive_label: "Drive".into(),
                provider: ManifestProvider::SteamMl,
                proton: String::new(),
            },
            bundle,
            Some(&backend),
            &library_path,
        )
        .unwrap();
        assert_eq!(report.game_id, "42");
        assert!(backend.verify("42").unwrap().registered);
        assert!(report.root.ends_with("SteamTronLibrary/steamtronapps/common/Sample Game"));
        assert!(report.prefix.ends_with("SteamTronLibrary/steamtronapps/compatdata/Sample Game"));

        let library = Library::open(&library_path).unwrap();
        let game = library.game("42").unwrap();
        assert_eq!(game.installation.mode, InstallMode::Managed);
        assert_eq!(game.steam.as_ref().unwrap().state, RegistrationState::PendingRestart);

        let duplicate = add_managed_game_at(
            ManagedAddRequest {
                app_id: 42,
                title: "Sample Game".into(),
                drive_root: temp.path().join("drive"),
                drive_label: "Drive".into(),
                provider: ManifestProvider::SteamMl,
                proton: String::new(),
            },
            sample_bundle("42"),
            Some(&backend),
            &library_path,
        );
        assert!(duplicate.is_err());
    }





    #[test]
    fn records_a_launch_target_inside_the_game_folder() {
        let temp = tempfile::tempdir().unwrap();
        let library_path = temp.path().join("library.json");
        let backend = MemoryBackend {
            root: temp.path().join("steam"),
        };
        add_managed_game_at(
            ManagedAddRequest {
                app_id: 42,
                title: "Sample".into(),
                drive_root: temp.path().join("drive"),
                drive_label: String::new(),
                provider: ManifestProvider::SteamMl,
                proton: String::new(),
            },
            sample_bundle("42"),
            Some(&backend),
            &library_path,
        )
        .unwrap();
        let game = Library::open(&library_path).unwrap().game("42").unwrap().clone();
        std::fs::create_dir_all(&game.installation.root).unwrap();
        std::fs::write(game.installation.root.join("game.exe"), b"MZ").unwrap();

        let report = set_launch_target_at(
            SetTargetRequest {
                game_id: "42".into(),
                executable: PathBuf::from("game.exe"),
                arguments: vec!["-windowed".into()],
                proton: String::new(),
                name: String::new(),
            },
            &library_path,
        )
        .unwrap();
        assert!(report.executable.ends_with("game.exe"));
        let stored = Library::open(&library_path).unwrap().game("42").unwrap().clone();
        assert_eq!(stored.launch_targets.len(), 1);
        assert!(stored.launch_targets[0].primary);
        assert_eq!(stored.launch_targets[0].arguments, vec!["-windowed"]);

        let outside = set_launch_target_at(
            SetTargetRequest {
                game_id: "42".into(),
                executable: temp.path().join("outside.exe"),
                arguments: Vec::new(),
                proton: String::new(),
                name: String::new(),
            },
            &library_path,
        );
        assert!(outside.is_err());
    }

    #[test]
    fn installs_lists_and_removes_managed_fixes() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let temp = tempfile::tempdir().unwrap();
        let library_path = temp.path().join("library.json");
        let backend = MemoryBackend {
            root: temp.path().join("steam"),
        };
        add_managed_game_at(
            ManagedAddRequest {
                app_id: 42,
                title: "Sample".into(),
                drive_root: temp.path().join("drive"),
                drive_label: String::new(),
                provider: ManifestProvider::SteamMl,
                proton: String::new(),
            },
            sample_bundle("42"),
            Some(&backend),
            &library_path,
        )
        .unwrap();
        let zip_path = temp.path().join("fixes.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("fix.dll", SimpleFileOptions::default()).unwrap();
            zip.write_all(b"dll").unwrap();
            zip.start_file("nested/readme.txt", SimpleFileOptions::default()).unwrap();
            zip.write_all(b"readme").unwrap();
            zip.finish().unwrap();
        }
        let view = install_fixes_at("42", &zip_path, &library_path).unwrap();
        assert_eq!(view.files.len(), 2);
        assert!(view.path.ends_with("fixes/Sample"));
        assert!(view.files.iter().any(|file| file.name == "fix.dll"));

        let removed = remove_fixes_at("42", &library_path).unwrap();
        assert_eq!(removed.files, 2);
        assert!(!Path::new(&view.path).exists());
    }

    #[test]
    fn steam_ready_requires_a_verified_registration() {
        let temp = tempfile::tempdir().unwrap();
        let backend = MemoryBackend {
            root: temp.path().join("steam"),
        };
        let mut game = Game {
            id: "42".into(),
            title: "Sample".into(),
            metadata_app_id: Some(42),
            metadata_source: None,
            installation: Installation {
                mode: InstallMode::Managed,
                root: temp.path().to_path_buf(),
                prefix: PathBuf::new(),
                proton: String::new(),
                manifest: PathBuf::new(),
                confirmed: false,
            },
            steam: None,
            launch: LaunchSettings::default(),
            launch_targets: Vec::new(),
            fixes_enabled: true,
            artwork: Artwork::default(),
            setup_completed_at: None,
        };
        assert!(steam_ready(&game, &backend).is_err());
        let manifests = BTreeMap::new();
        backend
            .register(&RegistrationRequest {
                app_id: "42",
                title: "Sample",
                lua: "addappid(42)\n",
                manifests: &manifests,
            })
            .unwrap();
        assert_eq!(steam_ready(&game, &backend).unwrap(), 42);
        game.metadata_app_id = None;
        assert!(steam_ready(&game, &backend).is_err());
    }

    #[test]
    fn links_a_folder_with_multiple_launchers() {
        let temp = tempfile::tempdir().unwrap();
        let library_path = temp.path().join("library.json");
        let backend = MemoryBackend {
            root: temp.path().join("steam"),
        };
        add_managed_game_at(
            ManagedAddRequest {
                app_id: 42,
                title: "Sample Game".into(),
                drive_root: temp.path().join("drive"),
                drive_label: String::new(),
                proton: String::new(),
                provider: ManifestProvider::SteamMl,
            },
            sample_bundle("42"),
            Some(&backend),
            &library_path,
        )
        .unwrap();
        let root = temp.path().join("installed");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("game.exe"), b"game").unwrap();
        fs::write(root.join("launcher.exe"), b"launcher").unwrap();
        let report = link_game_at(
            LinkGameRequest {
                game_id: "42".into(),
                root: root.display().to_string(),
                launchers: vec![
                    LinkLauncher {
                        name: String::new(),
                        executable: root.join("game.exe").display().to_string(),
                        arguments: Vec::new(),
                        primary: false,
                    },
                    LinkLauncher {
                        name: "Launcher".into(),
                        executable: root.join("launcher.exe").display().to_string(),
                        arguments: vec!["-nolauncher".into()],
                        primary: false,
                    },
                ],
            },
            &library_path,
        )
        .unwrap();
        assert_eq!(report.launchers, 2);
        assert_eq!(report.primary, "game");
        let library = Library::open(&library_path).unwrap();
        let game = library.game("42").unwrap();
        assert_eq!(game.installation.mode, InstallMode::Linked);
        assert!(game.setup_completed_at.is_some());
        assert_eq!(game.launch_targets.len(), 2);
        assert!(game.launch_targets[0].primary);
        assert!(!game.launch_targets[1].primary);
    }

    #[test]
    fn reconcile_marks_registered_games_and_keeps_pending_ones() {
        let temp = tempfile::tempdir().unwrap();
        let library_path = temp.path().join("library.json");
        let backend = MemoryBackend {
            root: temp.path().join("steam"),
        };
        add_managed_game_at(
            ManagedAddRequest {
                app_id: 42,
                title: "Sample".into(),
                drive_root: temp.path().join("drive"),
                drive_label: String::new(),
                provider: ManifestProvider::SteamMl,
                proton: String::new(),
            },
            sample_bundle("42"),
            Some(&backend),
            &library_path,
        )
        .unwrap();
        // The test backend already has the Lua, so reconcile clears the pending state.
        let report = reconcile_registrations_at(&library_path, &backend).unwrap();
        assert_eq!(report.checked, 1);
        assert_eq!(report.registered, 1);
        assert_eq!(report.pending, 0);
        let game = Library::open(&library_path).unwrap().game("42").unwrap().clone();
        assert_eq!(game.steam.unwrap().state, RegistrationState::Registered);

        backend.unregister("42").unwrap();
        let report = reconcile_registrations_at(&library_path, &backend).unwrap();
        assert_eq!(report.registered, 0);
        assert_eq!(report.pending, 1);
    }

    #[test]
    fn adds_without_a_steam_backend_and_writes_appmanifest() {
        let temp = tempfile::tempdir().unwrap();
        let library_path = temp.path().join("library.json");
        let report = add_managed_game_at(
            ManagedAddRequest {
                app_id: 42,
                title: "Sample Game".into(),
                drive_root: temp.path().join("drive"),
                drive_label: String::new(),
                provider: ManifestProvider::SteamMl,
                proton: String::new(),
            },
            sample_bundle("42"),
            None,
            &library_path,
        )
        .unwrap();
        assert!(!report.registered);
        assert!(!report.pending_restart);
        assert!(report.warning.is_some());
        assert!(report.lua.starts_with("manifest-bundles/"));
        let game = Library::open(&library_path).unwrap().game("42").unwrap().clone();
        assert!(game.installation.manifest.is_file());
        assert!(super::managed_lua_path(&game).is_none_or(|path| !path.exists()));
        assert!(game.steam.is_none());
        assert_eq!(game.launch.prefix_mode, "umu");
    }

    #[test]
    fn rejects_a_bundle_for_another_app() {
        let temp = tempfile::tempdir().unwrap();
        let backend = MemoryBackend {
            root: temp.path().join("steam"),
        };
        let error = add_managed_game_at(
            ManagedAddRequest {
                app_id: 42,
                title: "Sample".into(),
                drive_root: temp.path().join("drive"),
                drive_label: String::new(),
                provider: ManifestProvider::SteamMl,
                proton: String::new(),
            },
            sample_bundle("99"),
            Some(&backend),
            &temp.path().join("library.json"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("different App ID"));
    }
}
