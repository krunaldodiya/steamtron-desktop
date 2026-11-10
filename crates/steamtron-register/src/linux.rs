//! Linux adapter for SLSsteam Moon.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    validate_appid, write_atomic, write_manifests, BackendStatus, Registration, RegistrationBackend,
    RegistrationError, RegistrationRequest, RegistrationState,
};

#[derive(Clone, Debug)]
pub struct SlsteamBackend {
    steam_root: PathBuf,
    data_home: PathBuf,
    config_home: PathBuf,
}

impl SlsteamBackend {
    pub fn discover() -> Result<Self, RegistrationError> {
        let steam_root =
            steamtron_core::steam::steam_root().map_err(|_| RegistrationError::SteamNotFound)?;
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
        let config_home = PathBuf::from(
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join(".config")
                }),
        );
        Ok(Self {
            steam_root,
            data_home,
            config_home,
        })
    }

    pub fn with_paths(steam_root: impl Into<PathBuf>, data_home: impl Into<PathBuf>) -> Self {
        let data_home = data_home.into();
        Self {
            steam_root: steam_root.into(),
            config_home: data_home.clone(),
            data_home,
        }
    }

    fn lua_dir(&self) -> PathBuf {
        self.steam_root.join("config/stplug-in")
    }

    fn sls_root(&self) -> PathBuf {
        self.data_home.join("SLSsteam")
    }

    fn manifest_store(&self) -> PathBuf {
        self.config_home.join("SLSsteam/manifests")
    }

    fn depotcache(&self) -> PathBuf {
        self.steam_root.join("steamapps/depotcache")
    }

    fn lua_path(&self, appid: &str) -> PathBuf {
        self.lua_dir().join(format!("{appid}.lua"))
    }
}

impl RegistrationBackend for SlsteamBackend {
    fn id(&self) -> &'static str {
        "slssteam-moon"
    }

    fn status(&self) -> Result<BackendStatus, RegistrationError> {
        let sls = self.sls_root().join("SLSsteam.so");
        let lua_dir = self.lua_dir();
        let installed = sls.is_file();
        let ready = installed && lua_dir.parent().is_some_and(Path::exists);
        Ok(BackendStatus {
            platform: "linux",
            backend: self.id(),
            installed,
            ready,
            steam_root: self.steam_root.display().to_string(),
            lua_dir: lua_dir.display().to_string(),
            details: if installed {
                "SLSsteam Moon hook detected.".into()
            } else {
                "SLSsteam Moon is not installed; fixes will work, library unlocking will not.".into()
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
        let store = self.manifest_store();
        let depotcache = self.depotcache();
        let mut written = write_manifests(&store, request.manifests)?;
        written.extend(write_manifests(&depotcache, request.manifests)?);
        for name in request.manifests.keys() {
            if let Some((depot, gid)) = name
                .strip_suffix(".manifest")
                .and_then(|value| value.split_once('_'))
            {
                let preferred = store.join(format!(".preferred_{depot}"));
                if !preferred.exists() {
                    write_atomic(&preferred, format!("{gid}\n").as_bytes(), 0o600)?;
                }
            }
        }
        Ok(Registration {
            app_id: request.app_id.to_owned(),
            title: request.title.to_owned(),
            backend: self.id().to_owned(),
            lua_path,
            manifests: written,
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

    fn backend() -> (tempfile::TempDir, SlsteamBackend) {
        let temp = tempfile::tempdir().unwrap();
        let steam = temp.path().join("Steam");
        let data = temp.path().join("data");
        fs::create_dir_all(steam.join("config")).unwrap();
        let backend = SlsteamBackend::with_paths(&steam, &data);
        (temp, backend)
    }

    #[test]
    fn registers_verifies_and_removes_one_app() {
        let (_temp, backend) = backend();
        let mut manifests = BTreeMap::new();
        manifests.insert("42_7.manifest".to_owned(), b"manifest".to_vec());
        let request = RegistrationRequest {
            app_id: "42",
            title: "Sample",
            lua: "addappid(42, 1, \"key\")\n",
            manifests: &manifests,
        };
        let result = backend.register(&request).unwrap();
        assert_eq!(result.backend, "slssteam-moon");
        assert!(result.lua_path.is_file());
        assert!(backend.manifest_store().join("42_7.manifest").is_file());
        assert!(backend.depotcache().join("42_7.manifest").is_file());
        assert_eq!(backend.manifest_store().join(".preferred_42").is_file(), true);

        let state = backend.verify("42").unwrap();
        assert!(state.registered);

        backend.unregister("42").unwrap();
        assert!(!backend.verify("42").unwrap().registered);
    }

    #[test]
    fn rejects_invalid_app_ids_and_empty_lua() {
        let (_temp, backend) = backend();
        let manifests = BTreeMap::new();
        let invalid = RegistrationRequest {
            app_id: "abc",
            title: "Sample",
            lua: "addappid(1)\n",
            manifests: &manifests,
        };
        assert!(backend.register(&invalid).is_err());
        let empty = RegistrationRequest {
            app_id: "42",
            title: "Sample",
            lua: "  \n",
            manifests: &manifests,
        };
        assert!(backend.register(&empty).is_err());
    }
}
