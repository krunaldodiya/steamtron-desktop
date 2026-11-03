//! Add Steam library entries from a Lua/manifest ZIP or the public SteamML bundle.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::Serialize;
use thiserror::Error;
use zip::ZipArchive;

use steamtron_core::steam;
use steamtron_unlock::UnlockBackend;

pub mod updates;

const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const STEAMML_URL: &str = "https://pub-5b6d3b7c03fd4ac1afb5bd3017850e20.r2.dev/{appid}.zip";

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
pub struct RemovedGame {
    pub appid: String,
    pub removed: Vec<String>,
    pub steam_restart_required: bool,
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
            Some(SearchResult { appid: id, name, kind, image, installed: false })
        })
        .collect())
}

pub fn download_bundle(appid: &str) -> Result<Bundle, LibraryError> {
    if normalize_appid(appid).as_deref() != Some(appid) {
        return Err(LibraryError::InvalidAppId);
    }
    let response = reqwest::blocking::Client::new()
        .get(STEAMML_URL.replace("{appid}", appid))
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
    if bytes.len() as u64 > MAX_ARCHIVE {
        return Err(LibraryError::Archive("archive is too large".into()));
    }
    parse_bundle(&bytes)
}

pub fn remove_game(appid: &str, backend: &dyn UnlockBackend) -> Result<RemovedGame, LibraryError> {
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
        backend.remove_lua(appid).map_err(|error| LibraryError::Backend(error.to_string()))?;
        removed.push(lua_path.display().to_string());
    }

    for library in &libraries {
        remove_manifests(&library.join("depotcache"), &manifest_names, &mut removed);
    }
    for store in manifest_stores() {
        remove_manifests(&store, &manifest_names, &mut removed);
    }

    Ok(RemovedGame {
        appid: appid.to_owned(),
        steam_restart_required: !removed.is_empty(),
        removed,
    })
}

fn remove_manifests(root: &Path, names: &BTreeSet<String>, removed: &mut Vec<String>) {
    let mut depots = BTreeSet::new();
    for name in names {
        let path = root.join(name);
        if path.is_file() {
            if fs::remove_file(&path).is_ok() {
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

pub fn add_bundle(
    appid: &str,
    title: &str,
    bundle: Bundle,
    backend: &dyn UnlockBackend,
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
        format!("Steam App {appid}")
    } else {
        title.trim().to_owned()
    };
    let title = sanitize_title(&raw_title);
    let libraries = steam::libraries().map_err(|_| LibraryError::SteamNotFound)?;
    let steamapps = libraries.into_iter().next().ok_or(LibraryError::SteamNotFound)?;
    let acf = steamapps.join(format!("appmanifest_{appid}.acf"));
    if acf.exists() {
        return Err(LibraryError::AlreadyInstalled(appid.to_owned()));
    }
    let existing_pin = updates::has_existing_pin(appid);
    let installed = backend
        .install_lua(appid, &bundle.lua, &bundle.manifests)
        .map_err(|error| LibraryError::Archive(error.to_string()))?;

    let depots = parse_depots(&bundle.lua)?;
    let text = acf_text(appid, &title, &depots);
    let temporary = acf.with_extension("acf.steamtron.tmp");
    fs::write(&temporary, text).map_err(|error| LibraryError::io(&temporary, error))?;
    fs::rename(&temporary, &acf).map_err(|error| LibraryError::io(&acf, error))?;

    let mut pinned = None;
    let mut pin_error = None;
    if !existing_pin {
        if let Ok(updates) = updates::available_updates(appid) {
            if !updates.latest.is_empty() {
                match updates::pin_update(appid, &updates.latest) {
                    Ok(_) => pinned = Some(updates.latest),
                    Err(error) => pin_error = Some(error.to_string()),
                }
            }
        }
    }

    Ok(AddedGame {
        appid: appid.to_owned(),
        title,
        library: steamapps.display().to_string(),
        appmanifest: acf.display().to_string(),
        lua: installed.lua_path.display().to_string(),
        manifests: installed.manifests.len(),
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

fn lua_declares_appid(lua: &str, appid: &str) -> bool {
    Regex::new(r"addappid\s*\(\s*(\d+)")
        .unwrap()
        .captures_iter(lua)
        .any(|captures| &captures[1] == appid)
}

fn sanitize_title(title: &str) -> String {
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

fn acf_text(appid: &str, title: &str, depots: &BTreeMap<String, (String, u64)>) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut lines = vec![
        "\"AppState\"".to_owned(),
        "{".to_owned(),
        format!("\t\"appid\"\t\t\"{appid}\""),
        "\t\"Universe\"\t\t\"1\"".to_owned(),
        format!("\t\"name\"\t\t\"{title}\""),
        "\t\"StateFlags\"\t\t\"2\"".to_owned(),
        format!("\t\"installdir\"\t\t\"{title}\""),
        format!("\t\"LastUpdated\"\t\t\"{now}\""),
        "\t\"SizeOnDisk\"\t\t\"0\"".to_owned(),
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
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

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
        remove_manifests(store, &names, &mut removed);
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
