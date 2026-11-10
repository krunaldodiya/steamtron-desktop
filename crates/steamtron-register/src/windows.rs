//! Windows adapter for BetterSteamTools.

use std::fs;
use std::path::PathBuf;

use crate::{
    validate_appid, write_atomic, write_manifests, BackendStatus, Registration, RegistrationBackend,
    RegistrationError, RegistrationRequest, RegistrationState,
};

#[derive(Clone, Debug)]
pub struct BtsBackend {
    steam_root: PathBuf,
}

impl BtsBackend {
    pub fn discover() -> Result<Self, RegistrationError> {
        let steam_root =
            steamtron_core::steam::steam_root().map_err(|_| RegistrationError::SteamNotFound)?;
        Ok(Self { steam_root })
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

    fn backend_files(&self) -> [PathBuf; 4] {
        [
            self.steam_root.join("OpenSteamTool.dll"),
            self.steam_root.join("dwmapi.dll"),
            self.steam_root.join("xinput1_4.dll"),
            self.steam_root.join("opensteamtool.toml"),
        ]
    }

    fn lua_path(&self, appid: &str) -> PathBuf {
        self.lua_dir().join(format!("{appid}.lua"))
    }
}

impl RegistrationBackend for BtsBackend {
    fn id(&self) -> &'static str {
        "bettersteamtools"
    }

    fn status(&self) -> Result<BackendStatus, RegistrationError> {
        let files = self.backend_files();
        let installed = files[0].exists() && files[1].exists() && files[3].exists();
        let lua_dir = self.lua_dir();
        Ok(BackendStatus {
            platform: "windows",
            backend: self.id(),
            installed,
            ready: installed,
            steam_root: self.steam_root.display().to_string(),
            lua_dir: lua_dir.display().to_string(),
            details: if installed {
                "BetterSteamTools detected.".into()
            } else {
                "BetterSteamTools files are missing from the Steam root.".into()
            },
        })
    }

    fn register(&self, request: &RegistrationRequest<'_>) -> Result<Registration, RegistrationError> {
        validate_appid(request.app_id)?;
        if request.lua.trim().is_empty() {
            return Err(RegistrationError::NotInstalled("empty Lua manifest".into()));
        }
        let lua_path = self.lua_path(request.app_id);
        write_atomic(&lua_path, request.lua.as_bytes(), 0o600)?;
        let manifests = write_manifests(&self.depotcache(), request.manifests)?;
        Ok(Registration {
            app_id: request.app_id.to_owned(),
            title: request.title.to_owned(),
            backend: self.id().to_owned(),
            lua_path,
            manifests,
        })
    }

    fn verify(&self, app_id: &str) -> Result<RegistrationState, RegistrationError> {
        validate_appid(app_id)?;
        let lua_path = self.lua_path(app_id);
        Ok(RegistrationState {
            app_id: app_id.to_owned(),
            backend: self.id().to_owned(),
            registered: lua_path.is_file(),
            lua_path: lua_path.display().to_string(),
        })
    }

    fn unregister(&self, app_id: &str) -> Result<(), RegistrationError> {
        validate_appid(app_id)?;
        let lua = self.lua_path(app_id);
        if lua.exists() {
            fs::remove_file(&lua).map_err(|error| RegistrationError::io(&lua, error))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn registers_verifies_and_removes_one_app() {
        let temp = tempfile::tempdir().unwrap();
        let backend = BtsBackend::with_steam_root(temp.path());
        let manifests = BTreeMap::new();
        let request = RegistrationRequest {
            app_id: "42",
            title: "Sample",
            lua: "addappid(42)\n",
            manifests: &manifests,
        };
        backend.register(&request).unwrap();
        assert!(backend.verify("42").unwrap().registered);
        backend.unregister("42").unwrap();
        assert!(!backend.verify("42").unwrap().registered);
    }
}
