//! Linux adapter for SLSsteam/Lumen.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    validate_appid, write_atomic, BackendStatus, InstalledLua, UnlockBackend, UnlockError,
};

#[derive(Clone, Debug)]
pub struct LinuxUnlock {
    steam_root: PathBuf,
    data_home: PathBuf,
}

impl LinuxUnlock {
    pub fn discover() -> Result<Self, UnlockError> {
        let steam_root = steamtron_core::steam::steam_root().map_err(|_| UnlockError::SteamNotFound)?;
        let data_home = PathBuf::from(
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join(".local/share")
                }),
        );
        Ok(Self {
            steam_root,
            data_home,
        })
    }

    pub fn with_paths(steam_root: impl Into<PathBuf>, data_home: impl Into<PathBuf>) -> Self {
        Self {
            steam_root: steam_root.into(),
            data_home: data_home.into(),
        }
    }

    fn lua_dir(&self) -> PathBuf {
        self.steam_root.join("config/stplug-in")
    }

    fn sls_root(&self) -> PathBuf {
        self.data_home.join("SLSsteam")
    }

    fn lumen_root(&self) -> PathBuf {
        self.data_home.join("Lumen")
    }

    fn manifest_store(&self) -> PathBuf {
        self.data_home.join("SLSsteam/manifests")
    }

    fn depotcache(&self) -> PathBuf {
        self.steam_root.join("steamapps/depotcache")
    }
}

impl UnlockBackend for LinuxUnlock {
    fn status(&self) -> Result<BackendStatus, UnlockError> {
        let sls = self.sls_root().join("SLSsteam.so");
        let lumen = self.lumen_root().join("lumen");
        let lua_dir = self.lua_dir();
        let installed = sls.is_file();
        let ready = installed && lua_dir.parent().is_some_and(Path::exists);
        let lumen_note = if lumen.exists() {
            "Lumen is present but is not required by the desktop app."
        } else {
            "Lumen is not installed, which is supported."
        };
        Ok(BackendStatus {
            platform: "linux",
            backend: "SLSsteam",
            installed,
            ready,
            steam_root: self.steam_root.display().to_string(),
            lua_dir: lua_dir.display().to_string(),
            details: if installed {
                format!("SLSsteam hook detected. {lumen_note}")
            } else {
                format!("SLSsteam is not installed; fixes will work, library unlocking will not. {lumen_note}")
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
        if lua.trim().is_empty() {
            return Err(UnlockError::NotInstalled("empty Lua manifest".into()));
        }
        let lua_path = self.lua_dir().join(format!("{appid}.lua"));
        write_atomic(&lua_path, lua.as_bytes(), 0o600)?;
        let store = self.manifest_store();
        let depotcache = self.depotcache();
        let mut written = Vec::new();
        for (name, data) in manifests {
            if name.contains('/') || name.contains('\\') || !name.ends_with(".manifest") {
                return Err(UnlockError::NotInstalled(format!(
                    "invalid manifest name: {name}"
                )));
            }
            let target = store.join(name);
            write_atomic(&target, data, 0o600)?;
            let cache = depotcache.join(name);
            write_atomic(&cache, data, 0o600)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_lua_and_manifests() {
        let temp = tempfile::tempdir().unwrap();
        let steam = temp.path().join("Steam");
        let data = temp.path().join("data");
        fs::create_dir_all(steam.join("config")).unwrap();
        let backend = LinuxUnlock::with_paths(&steam, &data);
        let mut manifests = BTreeMap::new();
        manifests.insert("42_7.manifest".to_owned(), b"manifest".to_vec());
        let result = backend
            .install_lua("42", "addappid(42)\n", &manifests)
            .unwrap();
        assert!(result.lua_path.is_file());
        assert!(backend.lua_dir().join("42.lua").is_file());
        assert!(backend.manifest_store().join("42_7.manifest").is_file());
        assert!(backend.depotcache().join("42_7.manifest").is_file());
    }
}
