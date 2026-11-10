//! Local artwork cache for managed games.
//!
//! UmuTron downloads cover/hero art once, validates the image bytes, stores the
//! files in its data directory and renders from disk. SteamTron does the same:
//! local UmuTron artwork is reused when present, Steam's modern StoreBrowse
//! assets are used next, and legacy CDN paths are the last fallback.

use std::path::{Path, PathBuf};


const MAX_IMAGE: u64 = 12 * 1024 * 1024;
const LEGACY_BASE: &str = "https://cdn.akamai.steamstatic.com/steam/apps";
const ASSET_BASE: &str = "https://shared.akamai.steamstatic.com/store_item_assets/";

pub struct ArtworkPaths {
    pub cover: Option<PathBuf>,
    pub hero: Option<PathBuf>,
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 512 {
        return None;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some("png");
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    None
}

fn find_cached(directory: &Path, appid: u32, kind: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(directory).ok()?;
    let prefix = format!("{appid}_{kind}.");
    entries
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
                && path.metadata().map(|meta| meta.len() > 512).unwrap_or(false)
        })
}

fn write_cached(directory: &Path, appid: u32, kind: &str, bytes: &[u8]) -> Option<PathBuf> {
    let extension = image_extension(bytes)?;
    let target = directory.join(format!("{appid}_{kind}.{extension}"));
    let temporary = target.with_extension("tmp");
    std::fs::write(&temporary, bytes).ok()?;
    std::fs::rename(&temporary, &target).ok()?;
    Some(target)
}


async fn download(
    client: &reqwest::Client,
    urls: &[String],
    directory: &Path,
    appid: u32,
    kind: &str,
) -> Option<PathBuf> {
    for url in urls {
        let Ok(response) = client.get(url).send().await else { continue };
        if !response.status().is_success() {
            continue;
        }
        let Ok(bytes) = response.bytes().await else { continue };
        if bytes.len() as u64 > MAX_IMAGE || image_extension(&bytes).is_none() {
            continue;
        }
        if let Some(path) = write_cached(directory, appid, kind, &bytes) {
            return Some(path);
        }
    }
    None
}

async fn store_asset_urls(client: &reqwest::Client, appid: u32) -> Vec<(String, String)> {
    let input = serde_json::json!({
        "ids": [{ "appid": appid }],
        "context": { "language": "english", "country_code": "US" },
        "data_request": { "include_assets": true },
    });
    let Ok(response) = client
        .get("https://api.steampowered.com/IStoreBrowseService/GetItems/v1/")
        .query(&[("input_json", input.to_string())])
        .send()
        .await
    else {
        return Vec::new();
    };
    let Ok(payload) = response.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    let Some(item) = payload["response"]["store_items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["appid"] == appid))
    else {
        return Vec::new();
    };
    let assets = &item["assets"];
    let Some(format) = assets["asset_url_format"].as_str() else {
        return Vec::new();
    };
    let prefix = format!("steam/apps/{appid}/");
    if !format.starts_with(&prefix) || !format.contains("${FILENAME}") {
        return Vec::new();
    }
    let mut urls = Vec::new();
    for (kind, keys) in [
        ("cover", &["library_capsule_2x", "library_capsule"][..]),
        ("hero", &["library_hero_2x", "library_hero"][..]),
    ] {
        if let Some(name) = keys.iter().find_map(|key| assets[*key].as_str()) {
            if !name.is_empty()
                && !name.starts_with('/')
                && !name.contains("..")
                && !name.contains('\\')
            {
                urls.push((kind.to_owned(), format!("{ASSET_BASE}{}", format.replace("${FILENAME}", name))));
            }
        }
    }
    urls
}

fn legacy_urls(appid: u32) -> Vec<(String, String)> {
    let base = format!("{LEGACY_BASE}/{appid}/");
    vec![
        ("cover".to_owned(), format!("{base}library_600x900_2x.jpg")),
        ("cover".to_owned(), format!("{base}library_600x900.jpg")),
        ("cover".to_owned(), format!("{base}header.jpg")),
        ("hero".to_owned(), format!("{base}library_hero.jpg")),
        ("hero".to_owned(), format!("{base}header.jpg")),
    ]
}

pub async fn ensure(game_id: &str) -> Result<ArtworkPaths, String> {
    use steamtron_core::library::Library;

    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(game_id)
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    let appid = game
        .metadata_app_id
        .ok_or_else(|| "game has no Steam App ID".to_string())?;
    let directory = steamtron_core::paths::layout().artwork();
    std::fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;

    let mut cover = find_cached(&directory, appid, "cover");
    let mut hero = find_cached(&directory, appid, "hero");
    if cover.is_none() || hero.is_none() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(25))
            .build()
            .map_err(|error| error.to_string())?;
        let mut candidates = legacy_urls(appid);
        let assets = store_asset_urls(&client, appid).await;
        candidates.extend(assets);
        if cover.is_none() {
            let urls: Vec<String> = candidates
                .iter()
                .filter(|(kind, _)| kind == "cover")
                .map(|(_, url)| url.clone())
                .collect();
            cover = download(&client, &urls, &directory, appid, "cover").await;
        }
        if hero.is_none() {
            let urls: Vec<String> = candidates
                .iter()
                .filter(|(kind, _)| kind == "hero")
                .map(|(_, url)| url.clone())
                .collect();
            hero = download(&client, &urls, &directory, appid, "hero").await;
        }
    }
    Ok(ArtworkPaths { cover, hero })
}
