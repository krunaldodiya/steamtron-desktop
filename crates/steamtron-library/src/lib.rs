//! Add Steam library entries from a Lua/manifest ZIP or the authenticated API.

use std::collections::BTreeMap;
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

const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("invalid App ID")]
    InvalidAppId,
    #[error("sign in to SteamTron first")]
    AuthRequired,
    #[error("manifest download failed: {0}")]
    Download(String),
    #[error("manifest archive is invalid: {0}")]
    Archive(String),
    #[error("Steam library was not found")]
    SteamNotFound,
    #[error("App {0} already has an appmanifest")]
    AlreadyInstalled(String),
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
}

pub fn normalize_appid(value: &str) -> Option<String> {
    let value = value.trim();
    let value = value
        .split_once("/app/")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(rest))
        .unwrap_or(value);
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(value.to_owned())
    } else {
        None
    }
}

pub fn download_bundle(appid: &str) -> Result<Bundle, LibraryError> {
    if normalize_appid(appid).as_deref() != Some(appid) {
        return Err(LibraryError::InvalidAppId);
    }
    let token = steamtron_auth::access_token();
    if token.is_empty() {
        return Err(LibraryError::AuthRequired);
    }
    let base = steamtron_auth::api_base();
    let response = reqwest::blocking::Client::new()
        .get(format!("{base}/api/download/{appid}?file_type=manifest"))
        .header("Authorization", format!("Bearer {token}"))
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
    let raw_title = if title.trim().is_empty() {
        format!("Steam App {appid}")
    } else {
        title.trim().to_owned()
    };
    let title = sanitize_title(&raw_title);
    let libraries = steam::libraries().map_err(|_| LibraryError::SteamNotFound)?;
    let library = libraries.into_iter().next().ok_or(LibraryError::SteamNotFound)?;
    let steamapps = library;
    let acf = steamapps.join(format!("appmanifest_{appid}.acf"));
    if acf.exists() {
        return Err(LibraryError::AlreadyInstalled(appid.to_owned()));
    }
    let installed = backend
        .install_lua(appid, &bundle.lua, &bundle.manifests)
        .map_err(|error| LibraryError::Archive(error.to_string()))?;

    let depots = parse_depots(&bundle.lua)?;
    let text = acf_text(appid, &title, &depots);
    let temporary = acf.with_extension("acf.steamtron.tmp");
    fs::write(&temporary, text).map_err(|error| LibraryError::io(&temporary, error))?;
    fs::rename(&temporary, &acf).map_err(|error| LibraryError::io(&acf, error))?;

    Ok(AddedGame {
        appid: appid.to_owned(),
        title,
        library: steamapps.display().to_string(),
        appmanifest: acf.display().to_string(),
        lua: installed.lua_path.display().to_string(),
        manifests: installed.manifests.len(),
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
        "\t\"StateFlags\"\t\t\"4\"".to_owned(),
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
