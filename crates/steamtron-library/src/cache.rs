//! Per-game cache sidecar that travels with appmanifest backups/exports.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use steamtron_core::library::{Library, Game};

pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GameCache {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub steam_app_id: Option<u32>,
    #[serde(default)]
    pub dlc: Option<serde_json::Value>,
    #[serde(default)]
    pub updates: Option<serde_json::Value>,
}

pub fn path_for_manifest(manifest: &Path) -> PathBuf {
    manifest.with_extension("cache.json")
}

pub fn path(game: &Game) -> Option<PathBuf> {
    let manifest = &game.installation.manifest;
    (!manifest.as_os_str().is_empty()).then(|| path_for_manifest(manifest))
}

pub fn load_for_game(game: &Game) -> GameCache {
    let Some(path) = path(game) else { return GameCache::default() };
    fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

pub fn save_for_game(game: &Game, cache: &GameCache) -> Result<(), String> {
    let Some(path) = path(game) else { return Ok(()) };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let mut cache = cache.clone();
    cache.schema = SCHEMA;
    cache.game_id = game.id.clone();
    cache.steam_app_id = game.metadata_app_id;
    let temporary = path.with_extension("cache.json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(&cache).map_err(|error| error.to_string())?)
        .map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, &path).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn cache_path_for_appid(appid: u32) -> Option<PathBuf> {
    let library = Library::load().ok()?;
    let game = library.game_by_app_id(appid)?;
    path(game)
}

pub fn clear_updates_for_appid(appid: u32) {
    let Some(path) = cache_path_for_appid(appid) else { return };
    let Ok(data) = fs::read(&path) else { return };
    let Ok(mut cache) = serde_json::from_slice::<GameCache>(&data) else { return };
    cache.updates = None;
    let _ = fs::write(path, serde_json::to_vec_pretty(&cache).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use steamtron_core::library::{Artwork, InstallMode, Installation, LaunchSettings};

    #[test]
    fn sidecar_round_trips_next_to_the_appmanifest() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("appmanifest_42.json");
        let game = Game {
            id: "42".into(),
            title: "Sample".into(),
            metadata_app_id: Some(42),
            metadata_source: None,
            installation: Installation {
                mode: InstallMode::Managed,
                root: temp.path().join("common/Sample"),
                prefix: temp.path().join("compatdata/Sample"),
                proton: String::new(),
                manifest: manifest.clone(),
                confirmed: true,
            },
            steam: None,
            launch: LaunchSettings::default(),
            launch_targets: Vec::new(),
            fixes_enabled: true,
            artwork: Artwork::default(),
            setup_completed_at: None,
        };
        let mut cache = GameCache::default();
        cache.dlc = Some(serde_json::json!({"fetched_at": 1, "entries": []}));
        save_for_game(&game, &cache).unwrap();
        let loaded = load_for_game(&game);
        assert_eq!(loaded.schema, SCHEMA);
        assert_eq!(loaded.game_id, "42");
        assert_eq!(loaded.steam_app_id, Some(42));
        assert!(loaded.dlc.is_some());
        assert_eq!(path_for_manifest(&manifest), manifest.with_extension("cache.json"));
    }
}
