use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::steamtron_home;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid game entry: {0}")]
    Invalid(String),
}

impl ConfigError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameEntry {
    pub appid: String,
    pub enabled: bool,
    pub fixes: PathBuf,
    #[serde(default)]
    pub launch_option_disabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    version: u32,
    games: BTreeMap<String, GameEntry>,
}

pub struct Config {
    path: PathBuf,
    games: BTreeMap<String, GameEntry>,
}

impl Config {
    pub fn default_path() -> PathBuf {
        steamtron_home().join("config.json")
    }

    pub fn load() -> Result<Self, ConfigError> {
        Self::open(Self::default_path())
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self {
                path,
                games: BTreeMap::new(),
            });
        }
        let text = fs::read_to_string(&path).map_err(|error| ConfigError::io(&path, error))?;
        let document: Document =
            serde_json::from_str(&text).map_err(|source| ConfigError::Json {
                path: path.clone(),
                source,
            })?;
        let mut games = BTreeMap::new();
        for (key, entry) in document.games {
            if !key.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            let Ok(fixes) = entry.fixes.canonicalize() else {
                continue;
            };
            if !fixes.is_dir() {
                continue;
            }
            games.insert(
                key.clone(),
                GameEntry {
                    appid: key,
                    enabled: entry.enabled,
                    fixes,
                    launch_option_disabled: entry.launch_option_disabled,
                },
            );
        }
        Ok(Self { path, games })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> impl Iterator<Item = &GameEntry> {
        self.games.values()
    }

    pub fn entry(&self, appid: &str) -> Option<&GameEntry> {
        self.games.get(appid)
    }

    pub fn upsert(
        &mut self,
        appid: impl Into<String>,
        fixes: impl Into<PathBuf>,
        enabled: bool,
    ) -> Result<GameEntry, ConfigError> {
        let appid = appid.into();
        if appid.is_empty() || !appid.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ConfigError::Invalid("App ID must be numeric".into()));
        }
        let fixes = fixes.into();
        let fixes = fixes
            .canonicalize()
            .map_err(|_| ConfigError::Invalid(format!("fixes path does not exist: {}", fixes.display())))?;
        if !fixes.is_dir() {
            return Err(ConfigError::Invalid(format!(
                "fixes path is not a directory: {}",
                fixes.display()
            )));
        }
        let entry = GameEntry {
            appid: appid.clone(),
            enabled,
            fixes,
            launch_option_disabled: self
                .games
                .get(&appid)
                .map(|entry| entry.launch_option_disabled)
                .unwrap_or(false),
        };
        self.games.insert(appid, entry.clone());
        self.save()?;
        Ok(entry)
    }

    pub fn set_launch_option_disabled(
        &mut self,
        appid: &str,
        disabled: bool,
    ) -> Result<(), ConfigError> {
        if let Some(entry) = self.games.get_mut(appid) {
            entry.launch_option_disabled = disabled;
            self.save()?;
        }
        Ok(())
    }

    pub fn remove(&mut self, appid: &str) -> Result<bool, ConfigError> {
        if self.games.remove(appid).is_some() {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| ConfigError::io(parent, error))?;
        let document = Document {
            version: 1,
            games: self.games.clone(),
        };
        let content = serde_json::to_string_pretty(&document)
            .map_err(|source| ConfigError::Json {
                path: self.path.clone(),
                source,
            })?;
        let temporary = self.path.with_extension("json.steamtron.tmp");
        fs::write(&temporary, content + "\n").map_err(|error| ConfigError::io(&temporary, error))?;
        fs::rename(&temporary, &self.path).map_err(|error| ConfigError::io(&self.path, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_entries() {
        let temp = tempfile::tempdir().unwrap();
        let fixes = temp.path().join("fixes");
        fs::create_dir_all(&fixes).unwrap();
        fs::write(fixes.join("a.dll"), b"x").unwrap();
        let path = temp.path().join("config.json");
        {
            let mut config = Config::open(&path).unwrap();
            config.upsert("42", &fixes, true).unwrap();
        }
        let config = Config::open(&path).unwrap();
        let entry = config.entry("42").unwrap();
        assert!(entry.enabled);
        assert_eq!(entry.fixes, fixes.canonicalize().unwrap());
    }

    #[test]
    fn removes_entries() {
        let temp = tempfile::tempdir().unwrap();
        let fixes = temp.path().join("fixes");
        fs::create_dir_all(&fixes).unwrap();
        let path = temp.path().join("config.json");
        let mut config = Config::open(&path).unwrap();
        config.upsert("42", &fixes, true).unwrap();
        assert!(config.remove("42").unwrap());
        assert!(Config::open(&path).unwrap().entry("42").is_none());
    }
}
