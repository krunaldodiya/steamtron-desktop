//! Add Steam library entries from a Lua/manifest ZIP or a provider bundle.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::Serialize;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use steamtron_core::steam;
use steamtron_register::{RegistrationBackend, RegistrationRequest};

pub mod updates;
pub mod managed;
pub mod cache;
pub mod backup;

pub(crate) const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("invalid App ID")]
    InvalidAppId,
    #[error("manifest download failed: {0}")]
    Download(String),
    #[error("manifest archive is invalid: {0}")]
    Archive(String),
    #[error("Steam library was not found")]
    SteamNotFound,
    #[error("App {0} already has an appmanifest")]
    AlreadyInstalled(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("update error: {0}")]
    Update(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{0}")]
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

#[derive(Debug, Clone)]
pub struct Bundle {
    pub lua: String,
    pub manifests: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Serialize)]
pub struct AddedGame {
    pub appid: String,
    pub title: String,
    pub library: String,
    pub appmanifest: String,
    pub lua: String,
    pub manifests: usize,
    pub pinned: Option<String>,
    pub pin_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RepairedGame {
    pub appid: String,
    pub title: String,
    pub appmanifest: String,
}

#[derive(Debug, Serialize)]
pub struct RemovedGame {
    pub appid: String,
    pub removed: Vec<String>,
    pub steam_restart_required: bool,
}

pub fn title_for_appid(appid: &str) -> Option<String> {
    search(appid)
        .ok()?
        .into_iter()
        .next()
        .map(|result| result.name)
        .filter(|name| !name.trim().is_empty())
}

pub fn normalize_appid(value: &str) -> Option<String> {
    let value = value.trim();
    let value = value
        .split_once("/app/")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(rest))
        .unwrap_or(value);
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(value.to_owned())
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub appid: String,
    pub name: String,
    pub kind: String,
    pub image: String,
    pub hero: String,
    pub installed: bool,
}

/// Resolve a Steam App ID/URL or search the public Steam store catalogue.
pub fn search(query: &str) -> Result<Vec<SearchResult>, LibraryError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(appid) = normalize_appid(query) {
        let client = reqwest::blocking::Client::new();
        let response = client
            .get("https://store.steampowered.com/api/appdetails")
            .query(&[("appids", appid.as_str()), ("l", "english"), ("cc", "US")])
            .header("User-Agent", "steamtron-desktop")
            .send()
            .map_err(|error| LibraryError::Download(error.to_string()))?;
        let payload: serde_json::Value = response
            .json()
            .map_err(|error| LibraryError::Download(error.to_string()))?;
        let data = payload.get(&appid).and_then(|value| value.get("data"));
        if let Some(data) = data {
            let name = data
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            let kind = data
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("app")
                .to_owned();
            let image = data
                .get("header_image")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            let hero = data
                .get("background_raw")
                .and_then(|value| value.as_str())
                .or_else(|| data.get("background").and_then(|value| value.as_str()))
                .unwrap_or("")
                .to_owned();
            let name = if name.is_empty() {
                format!("Steam App {appid}")
            } else {
                name
            };
            return Ok(vec![SearchResult {
                appid,
                name,
                kind,
                image,
                hero,
                installed: false,
            }]);
        }
    }
    let client = reqwest::blocking::Client::new();
    let response = client
        .get("https://store.steampowered.com/api/storesearch/")
        .query(&[("term", query), ("l", "english"), ("cc", "US")])
        .header("User-Agent", "steamtron-desktop")
        .send()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    let payload: serde_json::Value = response
        .json()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    let items = payload
        .get("items")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(items
        .into_iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_u64()?.to_string();
            let name = item.get("name")?.as_str()?.to_owned();
            let kind = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("app")
                .to_owned();
            let image = item
                .get("tiny_image")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned();
            Some(SearchResult { appid: id, name, kind, image, hero: String::new(), installed: false })
        })
        .collect())
}

/// One global ZIP cache entry per provider and App ID. The raw archive is
/// kept so an add can reuse it without contacting the provider again.
fn manifest_cache_path_in(cache_root: &Path, provider: &str, appid: &str) -> PathBuf {
    cache_root
        .join("manifests")
        .join(provider)
        .join(format!("{appid}.zip"))
}

pub fn manifest_cache_path(appid: &str, provider: &str) -> PathBuf {
    manifest_cache_path_in(&steamtron_core::paths::layout().cache_dir(), provider, appid)
}

fn read_cached_manifest_from(cache_root: &Path, appid: &str, provider: &str) -> Option<Bundle> {
    let path = manifest_cache_path_in(cache_root, provider, appid);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => return None,
    };
    if bytes.len() as u64 > MAX_ARCHIVE {
        let _ = fs::remove_file(&path);
        return None;
    }
    match parse_bundle(&bytes) {
        Ok(mut bundle) => {
            bundle.lua = canonical_lua(&bundle.lua);
            Some(bundle)
        }
        Err(_) => {
            // A half-written or malformed cache must never block a download.
            let _ = fs::remove_file(&path);
            None
        }
    }
}

pub(crate) fn read_cached_manifest(appid: &str, provider: &str) -> Option<Bundle> {
    read_cached_manifest_from(
        &steamtron_core::paths::layout().cache_dir(),
        appid,
        provider,
    )
}

pub(crate) fn invalidate_cached_manifest(appid: &str, provider: &str) {
    let path = manifest_cache_path(appid, provider);
    let _ = fs::remove_file(path);
}

fn store_cached_manifest_in(
    cache_root: &Path,
    appid: &str,
    provider: &str,
    bytes: &[u8],
) {
    let path = manifest_cache_path_in(cache_root, provider, appid);
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension(format!("zip.{}.tmp", std::process::id()));
    if fs::write(&temporary, bytes).is_ok() && fs::rename(&temporary, &path).is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

pub(crate) fn store_cached_manifest(appid: &str, provider: &str, bytes: &[u8]) {
    store_cached_manifest_in(
        &steamtron_core::paths::layout().cache_dir(),
        appid,
        provider,
        bytes,
    );
}

pub(crate) fn persistent_bundle_path(appid: &str) -> PathBuf {
    steamtron_core::paths::layout()
        .home()
        .join("manifest-bundles")
        .join(format!("{appid}.zip"))
}

fn read_bundle_file(path: &Path) -> Option<Bundle> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() as u64 > MAX_ARCHIVE { return None; }
    match parse_bundle(&bytes) {
        Ok(mut bundle) => {
            bundle.lua = canonical_lua(&bundle.lua);
            Some(bundle)
        }
        Err(_) => None,
    }
}

pub(crate) fn store_persistent_bundle(appid: &str, bytes: &[u8]) {
    let path = persistent_bundle_path(appid);
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_err() { return; }
    let temporary = path.with_extension(format!("zip.{}.tmp", std::process::id()));
    if fs::write(&temporary, bytes).is_ok() && fs::rename(&temporary, &path).is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

/// Move bundles from the disposable cache into SteamTron's persistent data
/// directory. Runs at startup and on demand so existing caches are not lost.
pub fn migrate_manifest_cache() {
    let old = steamtron_core::paths::layout()
        .cache_dir()
        .join("manifests")
        .join("api");
    let Ok(entries) = fs::read_dir(&old) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("zip") { continue; }
        let Some(appid) = path.file_stem().and_then(|value| value.to_str()) else { continue };
        if normalize_appid(appid).as_deref() != Some(appid) { continue; }
        let destination = persistent_bundle_path(appid);
        if destination.is_file() { continue; }
        if let Some(parent) = destination.parent() { let _ = fs::create_dir_all(parent); }
        if fs::rename(&path, &destination).is_err() {
            let _ = fs::copy(&path, &destination);
        }
    }
}

/// Write a persistent bundle from imported Lua and manifest bytes.
pub fn write_persistent_bundle(
    appid: &str,
    lua: &str,
    manifests: &BTreeMap<String, Vec<u8>>,
) -> Result<(), LibraryError> {
    let path = persistent_bundle_path(appid);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| LibraryError::io(parent, error))?;
    }
    let temporary = path.with_extension("zip.tmp");
    let file = fs::File::create(&temporary).map_err(|error| LibraryError::io(&temporary, error))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(format!("{appid}.lua"), options).map_err(LibraryError::Zip)?;
    use std::io::Write;
    zip.write_all(lua.as_bytes()).map_err(|error| LibraryError::io(&temporary, error))?;
    for (name, data) in manifests {
        let safe = Path::new(name).file_name().and_then(|value| value.to_str()).unwrap_or("manifest");
        zip.start_file(safe.to_owned(), options).map_err(LibraryError::Zip)?;
        zip.write_all(data).map_err(|error| LibraryError::io(&temporary, error))?;
    }
    zip.finish().map_err(LibraryError::Zip)?;
    fs::rename(&temporary, &path).map_err(|error| LibraryError::io(&path, error))?;
    Ok(())
}

pub(crate) fn read_persistent_bundle(appid: &str) -> Option<Bundle> {
    read_bundle_file(&persistent_bundle_path(appid))
}

/// Read the globally persisted Lua/manifest bundle for a game.
pub fn persistent_bundle(appid: &str) -> Option<Bundle> {
    read_persistent_bundle(appid)
}

pub fn download_bundle(appid: &str) -> Result<Bundle, LibraryError> {
    if normalize_appid(appid).as_deref() != Some(appid) {
        return Err(LibraryError::InvalidAppId);
    }

    // Persistent store first: adding/removing a library entry never needs to
    // contact the manifest API twice for the same App ID.
    let persistent = persistent_bundle_path(appid);
    if let Some(bundle) = read_bundle_file(&persistent) {
        return Ok(bundle);
    }
    if let Some(bundle) = read_cached_manifest(appid, "api") {
        // Migrate an existing disposable-cache bundle into the persistent store.
        if let Ok(bytes) = fs::read(manifest_cache_path(appid, "api")) {
            store_persistent_bundle(appid, &bytes);
        }
        return Ok(bundle);
    }

    // The signed-in UMUTron API owns provider priority: Ryuu first, then
    // Luie, then SteamML. Do not silently use the old public SteamML archive
    // here; it can point at a different game build than the selected fix.
    let bytes = match download_from_api(appid) {
        Ok(bytes) => bytes,
        Err(api_error) => {
            let fallback = download_from_public(appid).map_err(|fallback_error| {
                LibraryError::Download(format!(
                    "{api_error}; public manifest fallback failed: {fallback_error}"
                ))
            })?;
            fallback
        }
    };
    if bytes.len() as u64 > MAX_ARCHIVE {
        return Err(LibraryError::Archive("archive is too large".into()));
    }
    let mut bundle = parse_bundle(&bytes)?;
    bundle.lua = canonical_lua(&bundle.lua);
    store_persistent_bundle(appid, &bytes);
    store_cached_manifest(appid, "api", &bytes);
    Ok(bundle)
}

fn download_from_public(appid: &str) -> Result<Vec<u8>, LibraryError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    let url = format!(
        "https://pub-5b6d3b7c03fd4ac1afb5bd3017850e20.r2.dev/{appid}.zip"
    );
    let response = client
        .get(url)
        .header("Accept", "application/zip")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    if !response.status().is_success() {
        return Err(LibraryError::Download(format!("HTTP {}", response.status())));
    }
    let bytes = response
        .bytes()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    Ok(bytes.to_vec())
}

fn download_from_api(appid: &str) -> Result<Vec<u8>, LibraryError> {
    let token = steamtron_auth::access_token();
    if token.is_empty() {
        return Err(LibraryError::Download(
            "sign in to use the UMUTron manifest providers".into(),
        ));
    }
    let base = steamtron_auth::api_base();
    // Vercel can take tens of seconds to assemble a full manifest archive.
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    let response = client
        .get(format!("{base}/api/download/{appid}"))
        .query(&[("file_type", "manifest")])
        .bearer_auth(token)
        .header("Accept", "application/zip")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    if !response.status().is_success() {
        return Err(LibraryError::Download(format!("HTTP {}", response.status())));
    }
    let bytes = response
        .bytes()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    Ok(bytes.to_vec())
}

fn choose_library(libraries: &[PathBuf]) -> Result<PathBuf, LibraryError> {
    if let Some(override_path) = std::env::var_os("STEAMTRON_LIBRARY").map(PathBuf::from) {
        let path = if override_path.file_name().and_then(|name| name.to_str()) == Some("steamapps") {
            override_path
        } else {
            override_path.join("steamapps")
        };
        let path = path.canonicalize().map_err(|error| LibraryError::io(&path, error))?;
        if libraries.contains(&path) {
            return Ok(path);
        }
        return Err(LibraryError::Update(
            "STEAMTRON_LIBRARY is not a configured Steam library".into(),
        ));
    }
    if libraries.is_empty() {
        return Err(LibraryError::SteamNotFound);
    }
    // Steam installs the game into the library that owns the most visible games.
    // This keeps the Steam root library for runtimes when the user's games live
    // in a separate library, while still falling back to the root for new users.
    let mut best = libraries[0].clone();
    let mut best_count = 0usize;
    for library in libraries {
        let count = steam::discover_games(std::slice::from_ref(library)).len();
        if count > best_count {
            best = library.clone();
            best_count = count;
        }
    }
    Ok(best)
}

pub fn repair_missing_appmanifests() -> Result<Vec<RepairedGame>, LibraryError> {
    let root = steam::steam_root().map_err(|_| LibraryError::SteamNotFound)?;
    let libraries = steam::libraries().map_err(|_| LibraryError::SteamNotFound)?;
    let library = choose_library(&libraries)?;
    let installed = steam::installed_appids().map_err(|_| LibraryError::SteamNotFound)?;
    let managed = steam::managed_appids().map_err(|_| LibraryError::SteamNotFound)?;
    let mut repaired = Vec::new();
    for appid in managed {
        if installed.contains(&appid) {
            continue;
        }
        let lua_path = root.join("config/stplug-in").join(format!("{appid}.lua"));
        let Ok(lua) = fs::read_to_string(lua_path) else {
            continue;
        };
        let depots = parse_depots(&lua)?;
        let title = lua
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("-- "))
            .map(|line| line.trim_start_matches("-- ").trim())
            .find(|line| {
                !line.contains("Lua and Manifest")
                    && !line.starts_with("Created:")
                    && !line.starts_with("Website:")
                    && !line.starts_with("Total ")
                    && !line.starts_with("MAIN ")
                    && !line.starts_with("SHARED ")
                    && !line.starts_with("DLCS ")
            })
            .map(str::to_owned)
            .or_else(|| title_for_appid(&appid))
            .unwrap_or_else(|| format!("Steam App {appid}"));
        let acf = library.join(format!("appmanifest_{appid}.acf"));
        if acf.exists() {
            continue;
        }
        let text = acf_text(
            &appid,
            &sanitize_title(&title),
            &depots,
            &library.join("common").join(sanitize_title(&title)),
        );
        let temporary = acf.with_extension("acf.steamtron.tmp");
        fs::write(&temporary, text).map_err(|error| LibraryError::io(&temporary, error))?;
        fs::rename(&temporary, &acf).map_err(|error| LibraryError::io(&acf, error))?;
        repaired.push(RepairedGame {
            appid,
            title,
            appmanifest: acf.display().to_string(),
        });
    }
    Ok(repaired)
}

pub fn remove_game(appid: &str, backend: &dyn RegistrationBackend) -> Result<RemovedGame, LibraryError> {
    if normalize_appid(appid).as_deref() != Some(appid) {
        return Err(LibraryError::InvalidAppId);
    }
    let root = steam::steam_root().map_err(|_| LibraryError::SteamNotFound)?;
    let libraries = steam::libraries().map_err(|_| LibraryError::SteamNotFound)?;
    let mut manifest_names = BTreeSet::new();
    let mut removed = Vec::new();

    let lua_path = root.join("config/stplug-in").join(format!("{appid}.lua"));
    if let Ok(text) = fs::read_to_string(&lua_path) {
        let set_manifest = Regex::new(r#"setManifestid\s*\(\s*(\d+)\s*,\s*["'](\d+)["']"#).unwrap();
        for captures in set_manifest.captures_iter(&text) {
            manifest_names.insert(format!("{}_{}.manifest", &captures[1], &captures[2]));
        }
    }

    let mut install_dir = String::new();
    for library in &libraries {
        let acf = library.join(format!("appmanifest_{appid}.acf"));
        if !acf.is_file() {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&acf) {
            let install = Regex::new(r#"(?m)^\s*"installdir"\s+"([^"]+)""#).unwrap();
            if install_dir.is_empty() {
                install_dir = install
                    .captures(&text)
                    .map(|captures| captures[1].to_owned())
                    .unwrap_or_default();
            }
            let depots = Regex::new(r#"(?m)^\s*"(\d+)"\s*\{\s*"manifest"\s+"(\d+)""#).unwrap();
            for captures in depots.captures_iter(&text) {
                manifest_names.insert(format!("{}_{}.manifest", &captures[1], &captures[2]));
            }
        }
        fs::remove_file(&acf).map_err(|error| LibraryError::io(&acf, error))?;
        removed.push(acf.display().to_string());
    }

    if !install_dir.is_empty() {
        for library in &libraries {
            let common = library.join("common").join(&install_dir);
            if common.symlink_metadata().map(|meta| meta.file_type().is_symlink()).unwrap_or(false)
                || common.is_symlink()
            {
                fs::remove_file(&common).map_err(|error| LibraryError::io(&common, error))?;
                removed.push(common.display().to_string());
            }
        }
    }

    if lua_path.exists() {
        backend.unregister(appid).map_err(|error| LibraryError::Backend(error.to_string()))?;
        removed.push(lua_path.display().to_string());
    }

    for library in &libraries {
        remove_manifests(&library.join("depotcache"), appid, &manifest_names, &mut removed);
    }
    for store in manifest_stores() {
        remove_manifests(&store, appid, &manifest_names, &mut removed);
    }

    Ok(RemovedGame {
        appid: appid.to_owned(),
        steam_restart_required: !removed.is_empty(),
        removed,
    })
}

fn remove_manifests(root: &Path, appid: &str, names: &BTreeSet<String>, removed: &mut Vec<String>) {
    let mut depots = BTreeSet::new();
    for name in names {
        for path in [root.join(appid).join(name), root.join(name)] {
            if path.is_file() && fs::remove_file(&path).is_ok() {
                removed.push(path.display().to_string());
            }
        }
        if let Some((depot, _)) = name.split_once('_') {
            depots.insert(depot.to_owned());
        }
    }
    for depot in depots {
        let preferred = root.join(format!(".preferred_{depot}"));
        if preferred.is_file() {
            if fs::remove_file(&preferred).is_ok() {
                removed.push(preferred.display().to_string());
            }
        }
    }
}

fn manifest_stores() -> Vec<PathBuf> {
    let mut stores = Vec::new();
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".config"))
    }) {
        stores.push(config.join("SLSsteam/manifests"));
    }
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".local/share"))
    }) {
        stores.push(data.join("SLSsteam/manifests"));
    }
    stores.push(steamtron_core::paths::layout().manifests());
    stores
}

pub fn bundle_from_file(path: &Path) -> Result<Bundle, LibraryError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "lua" {
        let bytes = fs::read(path).map_err(|error| LibraryError::io(path, error))?;
        if bytes.len() as u64 > MAX_ARCHIVE {
            return Err(LibraryError::Archive("Lua file is too large".into()));
        }
        let lua = String::from_utf8(bytes)
            .map_err(|error| LibraryError::Archive(format!("Lua file is not UTF-8: {error}")))?
            .trim_start_matches('\u{feff}')
            .to_owned();
        if lua.trim().is_empty() {
            return Err(LibraryError::Archive("Lua file is empty".into()));
        }
        return Ok(Bundle {
            lua,
            manifests: BTreeMap::new(),
        });
    }
    if extension == "zip" {
        return bundle_from_zip(path);
    }
    Err(LibraryError::Archive(
        "Choose a .lua file or a ZIP containing Lua and manifest files".into(),
    ))
}

pub fn bundle_from_zip(path: &Path) -> Result<Bundle, LibraryError> {
    let bytes = fs::read(path).map_err(|error| LibraryError::io(path, error))?;
    if bytes.len() as u64 > MAX_ARCHIVE {
        return Err(LibraryError::Archive("archive is too large".into()));
    }
    parse_bundle(&bytes)
}

/// SteamTools expects keyed addappid entries to use index 1. Ryuu's
/// archive keeps its raw `addappid(app, 0, key)` form, while lua.tools and
/// the canonical Lua endpoint use `addappid(app, 1, key)`.
pub fn canonical_lua(content: &str) -> String {
    let keyed = Regex::new(r"addappid\(\s*(\d+)\s*,\s*0\s*,").unwrap();
    let manifest = Regex::new(
        r#"^(\s*)(--\s*)?setManifestid\(\s*(\d+)\s*,\s*["'](\d+)["'](?:\s*,\s*(\d+))?\s*\)"#,
    )
    .unwrap();
    let mut out = String::with_capacity(content.len());
    for line in content.lines() {
        if let Some(captures) = manifest.captures(line) {
            let indent = &captures[1];
            let depot = &captures[3];
            let manifest_id = &captures[4];
            let size = captures.get(5).map(|value| value.as_str());
            out.push_str(&format!(
                "{indent}setManifestid({depot}, \"{manifest_id}\"{})\n",
                size.map(|size| format!(", {size}")).unwrap_or_default()
            ));
            continue;
        }
        out.push_str(&keyed.replace(line, "addappid(${1}, 1,"));
        out.push('\n');
    }
    out
}

pub fn parse_bundle(bytes: &[u8]) -> Result<Bundle, LibraryError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| LibraryError::Archive(error.to_string()))?;
    let manifest = Regex::new(r"^\d+_\d+\.manifest$").unwrap();
    let mut lua = None;
    let mut manifests = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| LibraryError::Archive(error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        if name.contains('/') || name.contains('\\') || name.starts_with('.') {
            continue;
        }
        if name.to_ascii_lowercase().ends_with(".lua") {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|error| LibraryError::Archive(error.to_string()))?;
            lua = Some(text.trim_start_matches('\u{feff}').to_owned());
        } else if manifest.is_match(&name) {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|error| LibraryError::Archive(error.to_string()))?;
            manifests.insert(name, data);
        }
    }
    let lua = lua.ok_or_else(|| LibraryError::Archive("archive contains no Lua file".into()))?;
    if lua.trim().is_empty() {
        return Err(LibraryError::Archive("Lua file is empty".into()));
    }
    Ok(Bundle { lua, manifests })
}

fn non_windows_depots(appid: &str) -> Result<BTreeSet<String>, LibraryError> {
    let url = format!("https://api.steamcmd.net/v1/info/{appid}");
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| LibraryError::Download(error.to_string()))?
        .get(url)
        .send()
        .map_err(|error| LibraryError::Download(error.to_string()))?;
    if !response.status().is_success() {
        return Err(LibraryError::Download(format!(
            "Steam app info returned HTTP {}",
            response.status()
        )));
    }
    let payload: serde_json::Value = response
        .json()
        .map_err(|error| LibraryError::Download(format!("Steam app info is invalid: {error}")))?;
    let depots = payload
        .get("data")
        .and_then(|data| data.get(appid))
        .and_then(|info| info.get("depots"))
        .and_then(|depots| depots.as_object())
        .ok_or_else(|| LibraryError::Download(format!("No depot metadata for App {appid}")))?;
    let mut result = BTreeSet::new();
    for (depot, meta) in depots {
        let oslist = meta
            .get("config")
            .and_then(|config| config.get("oslist"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !oslist.is_empty() && !oslist.split(',').any(|part| part.trim() == "windows") {
            result.insert(depot.clone());
        }
    }
    Ok(result)
}

pub(crate) fn windows_only_lua(appid: &str, lua: &str) -> Result<String, LibraryError> {
    let blocked = non_windows_depots(appid)?;
    if blocked.is_empty() {
        return Ok(lua.to_owned());
    }
    let add = Regex::new(r"^\s*(?:--\s*)?addappid\s*\(\s*(\d+)").unwrap();
    let manifest = Regex::new(r"^\s*(?:--\s*)?setManifestid\s*\(\s*(\d+)").unwrap();
    let mut output = String::with_capacity(lua.len());
    for line in lua.lines() {
        let depot = add
            .captures(line)
            .or_else(|| manifest.captures(line))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str());
        if depot.is_some_and(|depot| blocked.contains(depot)) {
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    Ok(output)
}

pub fn add_bundle(
    appid: &str,
    title: &str,
    bundle: Bundle,
    backend: &dyn RegistrationBackend,
) -> Result<AddedGame, LibraryError> {
    if normalize_appid(appid).as_deref() != Some(appid) {
        return Err(LibraryError::InvalidAppId);
    }
    if !lua_declares_appid(&bundle.lua, appid) {
        return Err(LibraryError::Archive(format!(
            "Lua manifest is for a different App ID (expected {appid})"
        )));
    }
    let raw_title = if title.trim().is_empty() {
        title_for_appid(appid).unwrap_or_else(|| format!("Steam App {appid}"))
    } else {
        title.trim().to_owned()
    };
    let title = sanitize_title(&raw_title);
    let libraries = steam::libraries().map_err(|_| LibraryError::SteamNotFound)?;
    let steamapps = choose_library(&libraries)?;
    let acf = steamapps.join(format!("appmanifest_{appid}.acf"));
    if acf.exists() {
        return Err(LibraryError::AlreadyInstalled(appid.to_owned()));
    }
    let existing_pin = updates::has_existing_pin(appid);
    // Steam always installs the host platform; on Linux that is the native
    // build. UmuTron-style Windows-only libraries must strip native Linux and
    // macOS depots from the provider Lua before Steam sees it.
    let lua = windows_only_lua(appid, &bundle.lua)?;
    let request = RegistrationRequest {
        app_id: appid,
        title: &title,
        lua: &lua,
        manifests: &bundle.manifests,
    };
    let registration = backend
        .register(&request)
        .map_err(|error| LibraryError::Archive(error.to_string()))?;
    let state = backend
        .verify(appid)
        .map_err(|error| LibraryError::Archive(error.to_string()))?;
    if !state.registered {
        return Err(LibraryError::Archive(format!(
            "{} did not keep the registration for App {appid}",
            backend.id()
        )));
    }

    let depots = parse_depots(&lua)?;
    let text = acf_text(appid, &title, &depots, &steamapps.join("common").join(&title));
    let temporary = acf.with_extension("acf.steamtron.tmp");
    fs::write(&temporary, text).map_err(|error| LibraryError::io(&temporary, error))?;
    fs::rename(&temporary, &acf).map_err(|error| LibraryError::io(&acf, error))?;

    // Do not pin the archived latest here. The game has not downloaded yet,
    // so there is no installed build to lock. The desktop calls
    // auto_pin_installed after Steam reports complete installed content.
    let pinned = None;
    let pin_error = None;
    let _ = existing_pin;

    Ok(AddedGame {
        appid: appid.to_owned(),
        title,
        library: steamapps.display().to_string(),
        appmanifest: acf.display().to_string(),
        lua: registration.lua_path.display().to_string(),
        manifests: registration.manifests.len(),
        pinned,
        pin_error,
    })
}

fn parse_depots(lua: &str) -> Result<BTreeMap<String, (String, u64)>, LibraryError> {
    let regex = Regex::new(
        r#"setManifestid\s*\(\s*(\d+)\s*,\s*["'](\d+)["'](?:\s*,\s*(\d+))?"#,
    )
    .unwrap();
    let mut depots = BTreeMap::new();
    for captures in regex.captures_iter(lua) {
        let depot = captures[1].to_owned();
        let manifest = captures[2].to_owned();
        let size = captures
            .get(3)
            .and_then(|value| value.as_str().parse().ok())
            .unwrap_or(0);
        depots.insert(depot, (manifest, size));
    }
    if depots.is_empty() {
        return Err(LibraryError::Archive(
            "Lua file contains no setManifestid entries".into(),
        ));
    }
    Ok(depots)
}

pub(crate) fn lua_declares_appid(lua: &str, appid: &str) -> bool {
    Regex::new(r"addappid\s*\(\s*(\d+)")
        .unwrap()
        .captures_iter(lua)
        .any(|captures| &captures[1] == appid)
}

pub(crate) fn sanitize_title(title: &str) -> String {
    let value: String = title
        .chars()
        .map(|character| match character {
            '"' | '\\' | '\n' | '\r' | '\t' => ' ',
            other => other,
        })
        .collect();
    let value = value.trim();
    if value.is_empty() {
        "Steam App".to_owned()
    } else {
        value.chars().take(200).collect()
    }
}

fn content_size(path: &Path) -> u64 {
    if !path.is_dir() {
        return 0;
    }
    let mut total = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

fn acf_text(
    appid: &str,
    title: &str,
    depots: &BTreeMap<String, (String, u64)>,
    common: &Path,
) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let size_on_disk = content_size(common);
    let state = if size_on_disk > 0 { 4 } else { 2 };
    let mut lines = vec![
        "\"AppState\"".to_owned(),
        "{".to_owned(),
        format!("\t\"appid\"\t\t\"{appid}\""),
        "\t\"Universe\"\t\t\"1\"".to_owned(),
        format!("\t\"name\"\t\t\"{title}\""),
        format!("\t\"StateFlags\"\t\t\"{state}\""),
        format!("\t\"installdir\"\t\t\"{title}\""),
        format!("\t\"LastUpdated\"\t\t\"{now}\""),
        format!("\t\"SizeOnDisk\"\t\t\"{size_on_disk}\""),
        "\t\"StagingSize\"\t\t\"0\"".to_owned(),
        "\t\"buildid\"\t\t\"0\"".to_owned(),
        "\t\"LastOwner\"\t\t\"0\"".to_owned(),
        "\t\"DownloadType\"\t\t\"1\"".to_owned(),
        "\t\"UpdateResult\"\t\t\"0\"".to_owned(),
        "\t\"BytesToDownload\"\t\t\"0\"".to_owned(),
        "\t\"BytesDownloaded\"\t\t\"0\"".to_owned(),
        "\t\"BytesToStage\"\t\t\"0\"".to_owned(),
        "\t\"BytesStaged\"\t\t\"0\"".to_owned(),
        "\t\"TargetBuildID\"\t\t\"0\"".to_owned(),
        "\t\"AutoUpdateBehavior\"\t\t\"0\"".to_owned(),
        "\t\"AllowOtherDownloadsWhileRunning\"\t\t\"0\"".to_owned(),
        "\t\"ScheduledAutoUpdate\"\t\t\"0\"".to_owned(),
        "\t\"InstalledDepots\"".to_owned(),
        "\t{".to_owned(),
    ];
    for (depot, (manifest, size)) in depots {
        lines.push(format!("\t\t\"{depot}\""));
        lines.push("\t\t{".to_owned());
        lines.push(format!("\t\t\t\"manifest\"\t\t\"{manifest}\""));
        lines.push(format!("\t\t\t\"size\"\t\t\"{size}\""));
        lines.push("\t\t}".to_owned());
    }
    lines.extend([
        "\t}".to_owned(),
        "\t\"UserConfig\"".to_owned(),
        "\t{".to_owned(),
        "\t\t\"language\"\t\t\"english\"".to_owned(),
        "\t}".to_owned(),
        "\t\"MountedConfig\"".to_owned(),
        "\t{".to_owned(),
        "\t\t\"language\"\t\t\"english\"".to_owned(),
        "\t}".to_owned(),
        "}".to_owned(),
        String::new(),
    ]);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::canonical_lua;

    #[test]
    fn appmanifest_marks_existing_content_installed() {
        let temp = tempfile::tempdir().unwrap();
        let common = temp.path().join("Game");
        std::fs::create_dir_all(&common).unwrap();
        std::fs::write(common.join("data.bin"), b"abc").unwrap();
        let depots = std::collections::BTreeMap::new();
        let text = acf_text("42", "Game", &depots, &common);
        assert!(text.contains("\t\"StateFlags\"\t\t\"4\""));
        assert!(text.contains("\t\"SizeOnDisk\"\t\t\"3\""));
    }

    #[test]
    fn canonicalizes_ryuu_keyed_addappid() {
        let lua = "addappid(204104,0,\"abc\")\naddappid(204100)\n";
        assert_eq!(
            canonical_lua(lua),
            "addappid(204104, 1,\"abc\")\naddappid(204100)\n"
        );
    }

    #[test]
    fn canonicalizes_manifest_lines() {
        let lua = "setManifestid(204104,\"123\")\n--setManifestid(204105, \"456\", 789)\n";
        assert_eq!(
            canonical_lua(lua),
            "setManifestid(204104, \"123\")\nsetManifestid(204105, \"456\", 789)\n"
        );
    }

    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_bundle(lua: &str, manifest: &str) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .start_file("42.lua", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(lua.as_bytes()).unwrap();
            writer
                .start_file("43_7.manifest", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(manifest.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn caches_provider_zip_by_appid_and_provider() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = zip_bundle("addappid(42)\n", "manifest");
        store_cached_manifest_in(temp.path(), "42", "steamml", &bytes);
        let path = manifest_cache_path_in(temp.path(), "steamml", "42");
        assert!(path.is_file());
        let cached = read_cached_manifest_from(temp.path(), "42", "steamml").unwrap();
        assert_eq!(cached.lua, "addappid(42)\n");
        assert_eq!(cached.manifests.get("43_7.manifest").unwrap(), b"manifest");
        assert!(!manifest_cache_path_in(temp.path(), "api", "42").exists());
    }

    #[test]
    fn corrupt_cached_zip_is_removed() {
        let temp = tempfile::tempdir().unwrap();
        let path = manifest_cache_path_in(temp.path(), "steamml", "42");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not a zip").unwrap();
        assert!(read_cached_manifest_from(temp.path(), "42", "steamml").is_none());
        assert!(!path.exists());
    }

    #[test]
    fn normalizes_appids_and_urls() {
        assert_eq!(normalize_appid("3468650").as_deref(), Some("3468650"));
        assert_eq!(
            normalize_appid("https://store.steampowered.com/app/3468650/Game/").as_deref(),
            Some("3468650")
        );
        assert_eq!(normalize_appid("not-an-id"), None);
    }

    #[test]
    fn prefers_library_with_games() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let games = temp.path().join("games");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&games).unwrap();
        std::fs::write(
            games.join("appmanifest_42.acf"),
            "\"AppState\"\n{\n\t\"name\"\t\t\"Game\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"SizeOnDisk\"\t\t\"100\"\n}\n",
        ).unwrap();
        let chosen = choose_library(&[root, games.clone()]).unwrap();
        assert_eq!(chosen, games);
    }

    #[test]
    fn imports_a_raw_lua_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("42.lua");
        std::fs::write(&path, "addappid(42)\nsetManifestid(43, \"7\")\n").unwrap();
        let bundle = bundle_from_file(&path).unwrap();
        assert!(bundle.lua.contains("addappid(42)"));
        assert!(bundle.manifests.is_empty());
    }

    #[test]
    fn removes_manifest_and_preferred_files() {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path();
        std::fs::write(store.join("43_7.manifest"), b"manifest").unwrap();
        std::fs::write(store.join(".preferred_43"), b"7\n").unwrap();
        let mut removed = Vec::new();
        let mut names = BTreeSet::new();
        names.insert("43_7.manifest".to_owned());
        remove_manifests(store, "43", &names, &mut removed);
        assert!(!store.join("43_7.manifest").exists());
        assert!(!store.join(".preferred_43").exists());
        assert_eq!(removed.len(), 2);
    }

    #[test]
    fn rejects_zip_for_another_appid() {
        assert!(lua_declares_appid("addappid(42)", "42"));
        assert!(!lua_declares_appid("addappid(42)", "43"));
    }

    #[test]
    fn parses_lua_and_manifest_zip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("manifest.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        writer
            .start_file("42.lua", SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(b"addappid(42)\nsetManifestid(43,\"7\")\n")
            .unwrap();
        writer
            .start_file("43_7.manifest", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"data").unwrap();
        writer.finish().unwrap();
        let bundle = bundle_from_zip(&path).unwrap();
        assert!(bundle.lua.contains("addappid(42)"));
        assert!(bundle.manifests.contains_key("43_7.manifest"));
    }
}
