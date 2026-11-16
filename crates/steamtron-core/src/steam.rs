use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SteamError {
    #[error("Steam installation was not found")]
    NotFound,
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct SteamGame {
    pub appid: String,
    pub name: String,
    pub installdir: String,
    pub build_id: String,
    pub state_flags: u32,
    pub last_updated: u64,
    pub size_on_disk: u64,
    pub library: PathBuf,
    pub common: PathBuf,
    pub manifest: PathBuf,
}

#[cfg(target_os = "windows")]
fn steam_registry_path() -> Vec<PathBuf> {
    use std::process::Command;

    let mut result = Vec::new();
    for (hive, key, value) in [
        ("HKCU", "Software\\Valve\\Steam", "SteamPath"),
        ("HKLM", "SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath"),
        ("HKLM", "SOFTWARE\\Valve\\Steam", "InstallPath"),
    ] {
        let query = format!("{hive}\\{key}");
        let Ok(output) = Command::new("reg")
            .args(["query", &query, "/v", value])
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(value) = text
            .lines()
            .find(|line| line.contains(value))
            .and_then(|line| line.split_once("REG_SZ"))
            .map(|(_, value)| value.trim())
        {
            if !value.is_empty() {
                result.push(PathBuf::from(value));
            }
        }
    }
    result
}

fn is_steam_installation(root: &Path) -> bool {
    if !root.join("steamapps").is_dir() {
        return false;
    }
    #[cfg(target_os = "windows")]
    let markers = ["steam.exe", "steamapps/libraryfolders.vdf", "config/loginusers.vdf", "registry.vdf"];
    #[cfg(not(target_os = "windows"))]
    let markers = [
        "steam.sh",
        "linux32/steam",
        "ubuntu12_32/steam",
        "steamapps/libraryfolders.vdf",
        "config/loginusers.vdf",
        "config/config.vdf",
        "registry.vdf",
    ];
    markers.iter().any(|marker| root.join(marker).is_file())
}

pub fn steam_root() -> Result<PathBuf, SteamError> {
    if let Some(value) = std::env::var_os("STEAMTRON_STEAM_ROOT") {
        let path = PathBuf::from(value);
        if is_steam_installation(&path) {
            return Ok(path);
        }
    }
    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        if let Some(value) = std::env::var_os("STEAM_PATH") {
            candidates.push(PathBuf::from(value));
        }
        candidates.extend(steam_registry_path());
        for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(value) = std::env::var_os(variable) {
                candidates.push(PathBuf::from(value).join("Steam"));
            }
        }
        for candidate in candidates {
            if is_steam_installation(&candidate) {
                return Ok(candidate);
            }
        }
        for candidate in [
            PathBuf::from("C:/Program Files (x86)/Steam"),
            PathBuf::from("C:/Program Files/Steam"),
        ] {
            if is_steam_installation(&candidate) {
                return Ok(candidate);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        for candidate in [
            home.join(".local/share/Steam"),
            home.join(".steam/steam"),
            home.join(".steam/root"),
        ] {
            if is_steam_installation(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(SteamError::NotFound)
}

pub fn libraries() -> Result<Vec<PathBuf>, SteamError> {
    Ok(discover_libraries(&steam_root()?))
}

pub fn discover_libraries(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![root.join("steamapps")];
    if let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        for line in text.lines() {
            if let Some(value) = field(line, "path") {
                if !value.is_empty() {
                    paths.push(PathBuf::from(value).join("steamapps"));
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for path in paths {
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        if path.is_dir() && seen.insert(path.clone()) {
            result.push(path);
        }
    }
    result
}

pub fn games() -> Result<Vec<SteamGame>, SteamError> {
    Ok(discover_games(&discover_libraries(&steam_root()?)))
}

pub fn discover_games(libraries: &[PathBuf]) -> Vec<SteamGame> {
    let ignored = [
        "proton",
        "steam linux runtime",
        "steamworks common redistributables",
        "steam controller configs",
    ];
    let mut result = Vec::new();
    for library in libraries {
        let Ok(entries) = fs::read_dir(library) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            let Some(appid) = name
                .strip_prefix("appmanifest_")
                .and_then(|value| value.strip_suffix(".acf"))
            else {
                continue;
            };
            if appid.is_empty() || !appid.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let title = field(&text, "name").unwrap_or_default();
            if ignored.iter().any(|fragment| title.to_ascii_lowercase().contains(fragment)) {
                continue;
            }
            let installdir = field(&text, "installdir").unwrap_or_default();
            let common = if installdir.is_empty() {
                PathBuf::new()
            } else {
                library.join("common").join(&installdir)
            };
            result.push(SteamGame {
                appid: appid.to_owned(),
                name: if title.is_empty() {
                    format!("Steam App {appid}")
                } else {
                    title
                },
                installdir,
                build_id: field(&text, "buildid").unwrap_or_default(),
                state_flags: field(&text, "StateFlags")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                last_updated: field(&text, "LastUpdated")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                size_on_disk: field(&text, "SizeOnDisk")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                library: library.clone(),
                common,
                manifest: path,
            });
        }
    }
    let mut unique: BTreeMap<String, SteamGame> = BTreeMap::new();
    for game in result {
        let installed = game.state_flags & 4 != 0 && game.size_on_disk > 0;
        match unique.get(&game.appid) {
            Some(existing)
                if installed
                    && (existing.state_flags & 4 == 0 || existing.size_on_disk == 0) =>
            {
                unique.insert(game.appid.clone(), game);
            }
            Some(_) => {}
            None => {
                unique.insert(game.appid.clone(), game);
            }
        }
    }
    let mut result: Vec<SteamGame> = unique.into_values().collect();
    result.sort_by(|left, right| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()));
    result
}

/// Every App ID that currently has an appmanifest in any Steam library.
/// This intentionally includes entries filtered out of the visible game list.
pub fn installed_appids() -> Result<BTreeSet<String>, SteamError> {
    let mut result = BTreeSet::new();
    for library in libraries()? {
        let Ok(entries) = fs::read_dir(library) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(appid) = name
                .strip_prefix("appmanifest_")
                .and_then(|value| value.strip_suffix(".acf"))
            else {
                continue;
            };
            if !appid.is_empty() && appid.bytes().all(|byte| byte.is_ascii_digit()) {
                result.insert(appid.to_owned());
            }
        }
    }
    Ok(result)
}

pub fn managed_appids() -> Result<BTreeSet<String>, SteamError> {
    let root = steam_root()?;
    let root = if root.join("config/stplug-in").is_dir() {
        root.join("config/stplug-in")
    } else {
        root.join("config/lua")
    };
    let mut result = BTreeSet::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(appid) = name.strip_suffix(".lua") else {
                continue;
            };
            if !appid.is_empty() && appid.bytes().all(|byte| byte.is_ascii_digit()) {
                result.insert(appid.to_owned());
            }
        }
    }
    Ok(result)
}

fn field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('"').collect();
        if parts.len() >= 4 && parts[1] == key {
            return Some(parts[3].to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatdata_only_root_is_not_a_steam_install() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("steamapps/compatdata/42/pfx")).unwrap();
        assert!(!is_steam_installation(temp.path()));
    }

    #[test]
    fn steam_marker_marks_a_real_install() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("steamapps")).unwrap();
        #[cfg(target_os = "windows")]
        fs::write(temp.path().join("steam.exe"), b"").unwrap();
        #[cfg(not(target_os = "windows"))]
        fs::write(temp.path().join("steam.sh"), b"").unwrap();
        assert!(is_steam_installation(temp.path()));
    }

    #[test]
    fn deduplicates_appids_across_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let manifest = |name: &str, flags: &str| {
            format!("\"AppState\"\n{{\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"SizeOnDisk\"\t\t\"0\"\n}}\n")
        };
        fs::write(first.join("appmanifest_42.acf"), manifest("Game", "4")).unwrap();
        fs::write(second.join("appmanifest_42.acf"), manifest("Game duplicate", "2")).unwrap();
        let games = discover_games(&[first, second]);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].appid, "42");
    }

    #[test]
    fn parses_manifest_fields() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("steamapps");
        fs::create_dir_all(&library).unwrap();
        fs::write(
            library.join("appmanifest_42.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"42\"\n\t\"name\"\t\t\"Game\"\n\t\"installdir\"\t\t\"GameDir\"\n\t\"buildid\"\t\t\"123\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"LastUpdated\"\t\t\"99\"\n\t\"SizeOnDisk\"\t\t\"1000\"\n}\n",
        )
        .unwrap();
        let games = discover_games(&[library]);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].appid, "42");
        assert_eq!(games[0].name, "Game");
        assert_eq!(games[0].build_id, "123");
        assert_eq!(games[0].size_on_disk, 1000);
    }
}
