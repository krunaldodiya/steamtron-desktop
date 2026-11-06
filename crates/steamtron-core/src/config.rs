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
pub struct LaunchTarget {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub executable: PathBuf,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameEntry {
    pub appid: String,
    pub enabled: bool,
    pub fixes: PathBuf,
    #[serde(default)]
    pub launch_option_disabled: bool,
    #[serde(default)]
    pub launch_target: Option<PathBuf>,
    #[serde(default)]
    pub launch_targets: Vec<LaunchTarget>,
    /// A game entry created only to hold launch targets. It has no fixes
    /// directory and must never be treated as a fixes entry.
    #[serde(default)]
    pub launch_only: bool,
}

fn clean_name(name: String) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect()
}

fn clean_arguments(arguments: Vec<String>) -> Vec<String> {
    arguments
        .into_iter()
        .map(|argument| argument.trim().to_owned())
        .filter(|argument| !argument.is_empty())
        .take(32)
        .collect()
}

fn normalize_targets(targets: &mut Vec<LaunchTarget>) {
    let mut seen_paths = std::collections::BTreeSet::new();
    let mut seen_ids = std::collections::BTreeSet::new();
    targets.retain(|target| seen_paths.insert(target.executable.clone()));
    for (index, target) in targets.iter_mut().enumerate() {
        if target.id.is_empty() || !seen_ids.insert(target.id.clone()) {
            let mut next = index + 1;
            while seen_ids.contains(&format!("t{next}")) {
                next += 1;
            }
            target.id = format!("t{next}");
            seen_ids.insert(target.id.clone());
        }
        target.name = clean_name(std::mem::take(&mut target.name));
        target.arguments = clean_arguments(std::mem::take(&mut target.arguments));
    }
    if !targets.iter().any(|target| target.primary) {
        if let Some(first) = targets.first_mut() {
            first.primary = true;
        }
    }
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
            let fixes = entry.fixes.canonicalize().ok().filter(|path| path.is_dir());
            if !entry.launch_only && fixes.is_none() {
                continue;
            }
            let fixes = fixes.unwrap_or_default();
            games.insert(
                key.clone(),
                GameEntry {
                    appid: key,
                    enabled: entry.enabled && !entry.launch_only,
                    fixes,
                    launch_option_disabled: entry.launch_option_disabled,
                    launch_target: entry.launch_target.clone(),
                    launch_targets: {
                        let mut targets = entry.launch_targets;
                        if targets.is_empty() {
                            if let Some(path) = entry.launch_target {
                                targets.push(LaunchTarget {
                                    id: "default".into(),
                                    name: String::new(),
                                    executable: path,
                                    arguments: Vec::new(),
                                    primary: true,
                                });
                            }
                        }
                        normalize_targets(&mut targets);
                        targets
                    },
                    launch_only: entry.launch_only,
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
            launch_target: self
                .games
                .get(&appid)
                .and_then(|entry| entry.launch_target.clone()),
            launch_targets: self
                .games
                .get(&appid)
                .map(|entry| entry.launch_targets.clone())
                .unwrap_or_default(),
            launch_only: false,
        };
        self.games.insert(appid, entry.clone());
        self.save()?;
        Ok(entry)
    }

    pub fn add_launch_target(
        &mut self,
        appid: &str,
        executable: PathBuf,
        name: String,
        arguments: Vec<String>,
    ) -> Result<LaunchTarget, ConfigError> {
        if !self.games.contains_key(appid) {
            self.games.insert(
                appid.to_owned(),
                GameEntry {
                    appid: appid.to_owned(),
                    enabled: false,
                    fixes: PathBuf::new(),
                    launch_option_disabled: false,
                    launch_target: None,
                    launch_targets: Vec::new(),
                    launch_only: true,
                },
            );
        }
        let entry = self
            .games
            .get_mut(appid)
            .ok_or_else(|| ConfigError::Invalid(format!("no fixes entry for App {appid}")))?;
        let executable = executable
            .canonicalize()
            .map_err(|_| ConfigError::Invalid(format!("launch target does not exist: {}", executable.display())))?;
        if !executable.is_file() {
            return Err(ConfigError::Invalid(format!("launch target is not a file: {}", executable.display())));
        }
        if entry.launch_targets.len() >= 16 {
            return Err(ConfigError::Invalid("At most 16 launch targets are allowed.".into()));
        }
        if entry.launch_targets.iter().any(|target| target.executable == executable) {
            return Err(ConfigError::Invalid("That launch target is already added.".into()));
        }
        let id = {
            let mut index = entry.launch_targets.len() + 1;
            while entry.launch_targets.iter().any(|target| target.id == format!("t{index}")) {
                index += 1;
            }
            format!("t{index}")
        };
        let target = LaunchTarget {
            id,
            name: clean_name(name),
            executable,
            arguments: clean_arguments(arguments),
            primary: entry.launch_targets.is_empty(),
        };
        entry.launch_targets.push(target.clone());
        normalize_targets(&mut entry.launch_targets);
        self.save()?;
        Ok(target)
    }

    pub fn update_launch_target(
        &mut self,
        appid: &str,
        id: &str,
        name: String,
        arguments: Vec<String>,
        executable: Option<PathBuf>,
    ) -> Result<(), ConfigError> {
        let executable = match executable {
            Some(path) => Some(
                path.canonicalize()
                    .map_err(|_| ConfigError::Invalid(format!("launch target does not exist: {}", path.display())))?,
            ),
            None => None,
        };
        if let Some(path) = executable.as_ref() {
            if !path.is_file() {
                return Err(ConfigError::Invalid(format!("launch target is not a file: {}", path.display())));
            }
        }
        let entry = self
            .games
            .get_mut(appid)
            .ok_or_else(|| ConfigError::Invalid(format!("no fixes entry for App {appid}")))?;
        if let Some(path) = executable.as_ref() {
            if entry
                .launch_targets
                .iter()
                .any(|target| target.id != id && target.executable == *path)
            {
                return Err(ConfigError::Invalid("That launch target is already added.".into()));
            }
        }
        let target = entry
            .launch_targets
            .iter_mut()
            .find(|target| target.id == id)
            .ok_or_else(|| ConfigError::Invalid("That launch target no longer exists.".into()))?;
        target.name = clean_name(name);
        target.arguments = clean_arguments(arguments);
        if let Some(path) = executable {
            target.executable = path;
        }
        self.save()?;
        Ok(())
    }

    pub fn remove_launch_target(&mut self, appid: &str, id: &str) -> Result<(), ConfigError> {
        let entry = self
            .games
            .get_mut(appid)
            .ok_or_else(|| ConfigError::Invalid(format!("no fixes entry for App {appid}")))?;
        entry.launch_targets.retain(|target| target.id != id);
        normalize_targets(&mut entry.launch_targets);
        self.save()?;
        Ok(())
    }

    pub fn set_launch_target(
        &mut self,
        appid: &str,
        target: Option<PathBuf>,
    ) -> Result<(), ConfigError> {
        let Some(entry) = self.games.get_mut(appid) else {
            return Err(ConfigError::Invalid(format!(
                "no fixes entry for App {appid}"
            )));
        };
        let target = match target {
            Some(path) => {
                let path = path
                    .canonicalize()
                    .map_err(|_| ConfigError::Invalid(format!(
                        "launch target does not exist: {}",
                        path.display()
                    )))?;
                if !path.is_file() {
                    return Err(ConfigError::Invalid(format!(
                        "launch target is not a file: {}",
                        path.display()
                    )));
                }
                Some(path)
            }
            None => None,
        };
        entry.launch_target = target.clone();
        entry.launch_targets = target
            .map(|path| vec![LaunchTarget {
                id: "t1".into(),
                name: String::new(),
                executable: path,
                arguments: Vec::new(),
                primary: true,
            }])
            .unwrap_or_default();
        self.save()?;
        Ok(())
    }

    pub fn set_fixes_path(
        &mut self,
        appid: &str,
        fixes: PathBuf,
    ) -> Result<(), ConfigError> {
        let entry = self
            .games
            .get_mut(appid)
            .ok_or_else(|| ConfigError::Invalid(format!("no fixes entry for App {appid}")))?;
        entry.fixes = fixes;
        self.save()?;
        Ok(())
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
    fn round_trips_launch_target() {
        let temp = tempfile::tempdir().unwrap();
        let fixes = temp.path().join("fixes");
        let common = temp.path().join("game");
        fs::create_dir_all(&fixes).unwrap();
        fs::create_dir_all(&common).unwrap();
        let target = common.join("game.exe");
        fs::write(&target, b"x").unwrap();
        let path = temp.path().join("config.json");
        {
            let mut config = Config::open(&path).unwrap();
            config.upsert("42", &fixes, true).unwrap();
            config
                .set_launch_target("42", Some(target.clone()))
                .unwrap();
        }
        let config = Config::open(&path).unwrap();
        assert_eq!(
            config.entry("42").unwrap().launch_target.as_ref(),
            Some(&target.canonicalize().unwrap())
        );
    }

    #[test]
    fn keeps_launch_targets_without_fixes() {
        let temp = tempfile::tempdir().unwrap();
        let common = temp.path().join("game");
        fs::create_dir_all(&common).unwrap();
        let target = common.join("game.exe");
        fs::write(&target, b"x").unwrap();
        let path = temp.path().join("config.json");
        {
            let mut config = Config::open(&path).unwrap();
            config
                .add_launch_target("42", target.clone(), String::new(), vec!["-nolauncher".into()])
                .unwrap();
        }
        let config = Config::open(&path).unwrap();
        let entry = config.entry("42").expect("launch-only entry");
        assert!(entry.launch_only);
        assert_eq!(entry.launch_targets.len(), 1);
        assert_eq!(entry.launch_targets[0].executable, target.canonicalize().unwrap());
    }

    #[test]
    fn adds_updates_and_removes_launch_targets() {
        let temp = tempfile::tempdir().unwrap();
        let fixes = temp.path().join("fixes");
        let common = temp.path().join("game");
        fs::create_dir_all(&fixes).unwrap();
        fs::create_dir_all(&common).unwrap();
        let first = common.join("a.exe");
        let second = common.join("b.exe");
        fs::write(&first, b"a").unwrap();
        fs::write(&second, b"b").unwrap();
        let path = temp.path().join("config.json");
        let mut config = Config::open(&path).unwrap();
        config.upsert("42", &fixes, true).unwrap();
        let target = config.add_launch_target("42", first.clone(), "A".into(), vec!["-nolauncher".into()]).unwrap();
        assert_eq!(target.arguments, vec!["-nolauncher"]);
        let target2 = config.add_launch_target("42", second, "B".into(), vec![]).unwrap();
        config.update_launch_target("42", &target2.id, "Bee".into(), vec!["-skip".into()], None).unwrap();
        assert_eq!(config.entry("42").unwrap().launch_targets.len(), 2);
        config.remove_launch_target("42", &target.id).unwrap();
        assert_eq!(config.entry("42").unwrap().launch_targets.len(), 1);
        let reopened = Config::open(&path).unwrap();
        assert_eq!(reopened.entry("42").unwrap().launch_targets[0].name, "Bee");
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
