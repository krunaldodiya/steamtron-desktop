//! Windows adapter for BetterSteamTools/OpenSteamTool.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    validate_appid, write_atomic, BackendStatus, InstalledLua, UnlockBackend, UnlockError,
};

#[derive(Clone, Debug)]
pub struct WindowsUnlock {
    steam_root: PathBuf,
}

impl WindowsUnlock {
    pub fn discover() -> Result<Self, UnlockError> {
        let candidates = [
            std::env::var_os("STEAM_PATH").map(PathBuf::from),
            std::env::var_os("ProgramFiles(x86)").map(|value| PathBuf::from(value).join("Steam")),
            std::env::var_os("ProgramFiles").map(|value| PathBuf::from(value).join("Steam")),
            Some(PathBuf::from("C:/Program Files (x86)/Steam")),
        ];
        for candidate in candidates.into_iter().flatten() {
            if candidate.join("steamapps").is_dir() {
                return Ok(Self {
                    steam_root: candidate,
                });
            }
        }
        Err(UnlockError::SteamNotFound)
    }

    pub fn with_steam_root(steam_root: impl Into<PathBuf>) -> Self {
        Self {
            steam_root: steam_root.into(),
        }
    }

    fn lua_dir(&self) -> PathBuf {
        let stplug = self.steam_root.join("config/stplug-in");
        if stplug.exists() {
            stplug
        } else {
            self.steam_root.join("config/lua")
        }
    }

    fn depotcache(&self) -> PathBuf {
        self.steam_root.join("steamapps/depotcache")
    }

    fn backend_files(&self) -> [PathBuf; 3] {
        [
            self.steam_root.join("OpenSteamTool.dll"),
            self.steam_root.join("dwmapi.dll"),
            self.steam_root.join("opensteamtool.toml"),
        ]
    }
}

impl UnlockBackend for WindowsUnlock {
    fn status(&self) -> Result<BackendStatus, UnlockError> {
        let files = self.backend_files();
        let installed = files.iter().all(|path| path.exists());
        let lua_dir = self.lua_dir();
        Ok(BackendStatus {
            platform: "windows",
            backend: "BetterSteamTools/OpenSteamTool",
            installed,
            ready: installed,
            steam_root: self.steam_root.display().to_string(),
            lua_dir: lua_dir.display().to_string(),
            details: if installed {
                "Windows unlock backend detected.".into()
            } else {
                "BetterSteamTools/OpenSteamTool files are missing from the Steam root.".into()
            },
        })
    }

    fn install_lua(
        &self,
        appid: &str,
        lua: &str,
        manifests: &BTreeMap<String, Vec<u8>>,
    ) -> Result<InstalledLua, UnlockError> {
        validate_appid(appid)?;
        let lua_path = self.lua_dir().join(format!("{appid}.lua"));
        write_atomic(&lua_path, lua.as_bytes(), 0o600)?;
        let mut written = Vec::new();
        for (name, data) in manifests {
            if name.contains('/') || name.contains('\\') || !name.ends_with(".manifest") {
                return Err(UnlockError::NotInstalled(format!(
                    "invalid manifest name: {name}"
                )));
            }
            let target = self.depotcache().join(name);
            write_atomic(&target, data, 0o600)?;
            written.push(target);
        }
        Ok(InstalledLua {
            appid: appid.to_owned(),
            lua_path,
            manifests: written,
        })
    }

    fn remove_lua(&self, appid: &str) -> Result<(), UnlockError> {
        validate_appid(appid)?;
        let lua = self.lua_dir().join(format!("{appid}.lua"));
        if lua.exists() {
            fs::remove_file(&lua).map_err(|error| UnlockError::io(&lua, error))?;
        }
        Ok(())
    }
}
