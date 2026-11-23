//! Data-driven launcher profiles.
//!
//! The engine stays generic; per-game knowledge lives in JSON. Entries supply
//! the preferred target, arguments, overlay mode, module hints and a clear
//! "needs a matching game-specific patch" status.

use std::path::{Component, Path};
use std::sync::OnceLock;

use serde::Deserialize;
use steamtron_core::library::{Game, LaunchTarget};

use crate::fix_tools;

const EMBEDDED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/catalog/launcher-catalog.json"
));

#[derive(Clone, Debug, Deserialize)]
pub struct Entry {
    pub appid: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default = "default_overlay")]
    pub overlay: String,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub requires_patch: bool,
    #[serde(default)]
    pub note: String,
}

#[derive(Deserialize)]
struct CatalogFile {
    entries: Vec<Entry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    Auto,
    Real,
    Drive,
}

fn default_overlay() -> String {
    "auto".into()
}

fn load() -> &'static Vec<Entry> {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let mut entries = serde_json::from_str::<CatalogFile>(EMBEDDED)
            .map(|catalog| catalog.entries)
            .unwrap_or_default();
        // An app-data catalog can override embedded entries without a rebuild.
        let override_path = steamtron_core::paths::layout()
            .home()
            .join("catalog")
            .join("launcher-catalog.json");
        if let Ok(bytes) = std::fs::read(override_path) {
            if let Ok(catalog) = serde_json::from_slice::<CatalogFile>(&bytes) {
                for entry in catalog.entries {
                    if let Some(existing) = entries.iter_mut().find(|item| item.appid == entry.appid) {
                        *existing = entry;
                    } else {
                        entries.push(entry);
                    }
                }
            }
        }
        entries
    })
}

pub fn entry(game: &Game) -> Option<&'static Entry> {
    let appid = game.metadata_app_id?;
    load().iter().find(|entry| entry.appid == appid)
}

impl Entry {
    pub fn overlay_mode(&self) -> OverlayMode {
        match self.overlay.to_ascii_lowercase().as_str() {
            "real" => OverlayMode::Real,
            "drive" => OverlayMode::Drive,
            _ => OverlayMode::Auto,
        }
    }

    pub fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|item| item.eq_ignore_ascii_case(module))
    }
}

pub fn apply_target(game: &mut Game, entry: &Entry) -> Result<(), String> {
    let Some(target) = entry.target.as_deref() else {
        return Ok(());
    };
    let relative = Path::new(target);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("catalog target is not a safe relative path: {target}"));
    }
    let executable = game.installation.root.join(relative);
    if !executable.is_file() {
        return Err(format!(
            "catalog target is missing: {}",
            executable.display()
        ));
    }
    let existing_index = game
        .launch_targets
        .iter()
        .position(|item| item.primary)
        .or_else(|| (!game.launch_targets.is_empty()).then_some(0));
    if let Some(index) = existing_index {
        let existing = &mut game.launch_targets[index];
        existing.executable = executable;
        existing.primary = true;
        if !entry.arguments.is_empty() {
            existing.arguments = entry.arguments.clone();
        }
    } else {
        game.launch_targets.push(LaunchTarget {
            id: "catalog".into(),
            name: if entry.title.is_empty() {
                "Game".into()
            } else {
                entry.title.clone()
            },
            executable,
            arguments: entry.arguments.clone(),
            primary: true,
        });
    }
    Ok(())
}

fn walk(root: &Path, depth: usize, names: &mut Vec<String>) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, depth + 1, names);
        } else if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            names.push(name.to_ascii_lowercase());
        }
    }
}

fn fixes_names(game: &Game) -> Vec<String> {
    let Some(fixes) = super::managed_fixes_dir(game) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    walk(&fixes, 0, &mut names);
    names
}

fn has_name(names: &[String], token: &str) -> bool {
    names.iter().any(|name| name.contains(token))
}

fn module_status(game: &Game, names: &[String], module: &str) -> serde_json::Value {
    let (status, detail) = match module {
        "goldberg" => {
            if fix_tools::goldberg_applied(game) {
                ("applied", "GoldBerg is applied to the fixes overlay.")
            } else if fix_tools::fixes_have_steam_emulator(game) {
                ("provided_by_fix", "The fixes pack already supplies a Steam emulator.")
            } else if fix_tools::can_apply_goldberg(game) {
                ("available", "GoldBerg can be applied automatically.")
            } else {
                ("not_applicable", "No Steam API DLL was found for GoldBerg.")
            }
        }
        "steamless" => {
            if fix_tools::steamless_applied(game) {
                ("applied", "Steamless has unpacked the launch target.")
            } else {
                ("available", "Steamless can inspect the launch target.")
            }
        }
        "uplay" => {
            if has_name(names, "uplay_r1_loader")
                || has_name(names, "uplay_r2_loader")
                || has_name(names, "upc_")
                || has_name(names, "ubisoftgame")
            {
                ("provided_by_fix", "The imported fix supplies a Uplay layer.")
            } else {
                ("needs_module_asset", "No Uplay loader is present in the fixes overlay.")
            }
        }
        "socialclub" => {
            if has_name(names, "socialclub") {
                ("provided_by_fix", "The imported fix supplies a SocialClub layer.")
            } else {
                ("needs_module_asset", "No SocialClub emulator is present.")
            }
        }
        "rgl" => (
            "needs_game_specific_patch",
            "Rockstar RGL stubs require a game-specific unprotected launcher.",
        ),
        "ea" => {
            if has_name(names, "eacore")
                || has_name(names, "originclient")
                || has_name(names, "eaapp")
                || has_name(names, "eadesktop")
            {
                ("provided_by_fix", "The imported fix supplies an EA/Origin layer.")
            } else {
                ("needs_module_asset", "No EA/Origin emulator is present.")
            }
        }
        "eos" => (
            "needs_module_asset",
            "No Epic Online Services emulator is available.",
        ),
        _ => ("unknown", "Unknown module."),
    };
    serde_json::json!({ "id": module, "status": status, "detail": detail })
}

pub fn status(game: &Game) -> serde_json::Value {
    let Some(entry) = entry(game) else {
        return serde_json::json!({ "catalog": false });
    };
    let names = fixes_names(game);
    let modules: Vec<serde_json::Value> = entry
        .modules
        .iter()
        .map(|module| module_status(game, &names, module))
        .collect();
    serde_json::json!({
        "catalog": true,
        "appid": entry.appid,
        "title": entry.title,
        "target": entry.target,
        "arguments": entry.arguments,
        "overlay": format!("{:?}", entry.overlay_mode()).to_ascii_lowercase(),
        "modules": modules,
        "requires_patch": entry.requires_patch,
        "note": entry.note,
    })
}

pub fn apply_recommended(
    game_id: &str,
    progress: &dyn Fn(&str),
) -> Result<serde_json::Value, String> {
    use steamtron_core::library::Library;

    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library
        .game(game_id)
        .cloned()
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    let entry = entry(&game)
        .cloned()
        .ok_or_else(|| "No launcher profile is available for this game.".to_string())?;
    if entry.requires_patch {
        return Err(entry.note.clone());
    }
    apply_target(&mut game, &entry)?;
    library.upsert_game(game.clone()).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;

    let mut applied = Vec::new();
    if entry.has_module("goldberg")
        && !fix_tools::goldberg_applied(&game)
        && !fix_tools::fixes_have_steam_emulator(&game)
        && fix_tools::can_apply_goldberg(&game)
    {
        progress("Applying GoldBerg…");
        fix_tools::apply_goldberg(game_id, progress)?;
        applied.push("goldberg");
    }
    if entry.has_module("steamless") && !fix_tools::steamless_applied(&game) {
        progress("Applying Steamless…");
        fix_tools::apply_steamless(game_id, progress)?;
        applied.push("steamless");
    }
    let library = Library::load().map_err(|error| error.to_string())?;
    let game = library
        .game(game_id)
        .ok_or_else(|| format!("unknown game {game_id}"))?;
    Ok(serde_json::json!({
        "applied": applied,
        "status": status(game),
    }))
}
