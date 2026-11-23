//! Goldberg emulator and Steamless helpers applied through a game's fixes overlay.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use steamtron_core::library::{Game, Library};

pub const GSE_TAG: &str = "2026_02_16";
const GSE_ASSET: &str = "emu-win-release.7z";
const GSE_SHA256: &str = "cdcee5f8a01b837205aba4d0c6867da98975c74d4f767da60b1724c318cdc2e4";
const GSE_URL: &str = "https://github.com/alex47exe/gse_fork/releases/download/2026_02_16/emu-win-release.7z";
const GOLDBERG_MANIFEST: &str = ".steamtron-goldberg.json";
const STEAMLESS_MANIFEST: &str = ".steamtron-steamless.json";

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|error| error.to_string())
}

fn fixes_dir(game: &Game) -> Result<PathBuf, String> {
    steamtron_core::library::migrate_legacy_fixes(game)
        .map_err(|error| error.to_string())
}

fn download(url: &str, path: &Path, expected: Option<&str>, limit: u64) -> Result<(), String> {
    let mut response = client()?
        .get(url)
        .header("User-Agent", "steamtron-desktop")
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download returned HTTP {}", response.status()));
    }
    let mut file = fs::File::create(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = response.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > limit {
            let _ = fs::remove_file(path);
            return Err("download is too large".into());
        }
        digest.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
    }
    file.sync_all().map_err(|error| error.to_string())?;
    if let Some(expected) = expected {
        if format!("{:x}", digest.finalize()) != expected {
            let _ = fs::remove_file(path);
            return Err("download failed its integrity check".into());
        }
    }
    Ok(())
}


fn ensure_goldberg_assets(progress: &dyn Fn(&str)) -> Result<PathBuf, String> {
    let root = steamtron_core::paths::layout().goldberg(GSE_TAG);
    if root.join("regular/x64/steam_api64.dll").is_file() {
        return Ok(root);
    }
    progress("Downloading GoldBerg assets…");
    let parent = root.parent().ok_or_else(|| "invalid Goldberg cache path".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staging = parent.join(format!(".gse-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let archive = staging.join(GSE_ASSET);
    if let Err(error) = download(GSE_URL, &archive, Some(GSE_SHA256), 512 * 1024 * 1024) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    progress("Extracting GoldBerg assets…");
    let out = staging.join("out");
    fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    if let Err(error) = sevenz_rust::decompress_file(&archive, &out) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("the GoldBerg release could not be extracted: {error}"));
    }
    let mut found = None;
    for entry in walk(&staging.join("out"), 0)? {
        if entry.ends_with("regular/x64/steam_api64.dll") {
            found = Some(entry.parent().unwrap().parent().unwrap().parent().unwrap().to_path_buf());
            break;
        }
    }
    let Some(found) = found else {
        let _ = fs::remove_dir_all(&staging);
        return Err("the GoldBerg release does not contain the expected files".into());
    };
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    fs::rename(&found, &root).map_err(|error| error.to_string())?;
    let _ = fs::remove_dir_all(&staging);
    Ok(root)
}

/// Deepest directory level scanned inside a fixes folder. UE4 titles keep the
/// emulator at `Engine/Binaries/ThirdParty/Steamworks/Steamv132/Win64/`, whose
/// `steam_settings` files sit eight levels down; a shallower limit made GoldBerg
/// look applied while removal found nothing to delete.
const MAX_FIXES_DEPTH: usize = 16;

fn walk(root: &Path, depth: usize) -> Result<Vec<PathBuf>, String> {
    if depth > MAX_FIXES_DEPTH {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    let entries = fs::read_dir(root).map_err(|error| error.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            result.extend(walk(&path, depth + 1)?);
        } else {
            result.push(path);
        }
    }
    Ok(result)
}

fn pe_machine(path: &Path) -> Option<u16> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 0x40 || &bytes[..2] != b"MZ" {
        return None;
    }
    let offset = u32::from_le_bytes(bytes.get(0x3c..0x40)?.try_into().ok()?) as usize;
    if bytes.get(offset..offset + 4)? != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes(bytes.get(offset + 4..offset + 6)?.try_into().ok()?))
}



#[derive(Serialize, Deserialize)]
struct Manifest { files: Vec<String> }

fn read_manifest(path: &Path) -> Vec<String> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok())
        .map(|manifest| manifest.files)
        .unwrap_or_default()
}

fn write_manifest(path: &Path, files: &[String]) -> Result<(), String> {
    let payload = serde_json::to_vec(&Manifest { files: files.to_vec() }).map_err(|error| error.to_string())?;
    fs::write(path, payload).map_err(|error| error.to_string())
}

fn find_steam_api(game: &Game) -> Result<(PathBuf, u16), String> {
    let root = &game.installation.root;
    let mut candidates: Vec<PathBuf> = Vec::new();
    for directory in [game.installation.root.clone()] {
        for name in ["steam_api64.dll", "steam_api.dll"] {
            let path = directory.join(name);
            if path.is_file() {
                candidates.push(path);
            }
        }
    }
    if let Some(target) = game.launch_targets.iter().find(|target| target.primary).or_else(|| game.launch_targets.first()) {
        let resolved = steamtron_core::runtime::resolve_executable(&game.installation.root, &target.executable);
        if let Some(parent) = resolved.as_deref().and_then(|path| path.parent()) {
            for name in ["steam_api64.dll", "steam_api.dll"] {
                let path = parent.join(name);
                if path.is_file() && !candidates.contains(&path) {
                    candidates.push(path);
                }
            }
        }
    }
    if candidates.is_empty() {
        if let Ok(files) = walk(root, 0) {
            for path in files {
                if path.file_name().and_then(|value| value.to_str()) == Some("steam_api64.dll")
                    || path.file_name().and_then(|value| value.to_str()) == Some("steam_api.dll")
                {
                    candidates.push(path);
                }
            }
        }
    }
    for path in candidates {
        if let Some(machine) = pe_machine(&path) {
            return Ok((path, machine));
        }
    }
    Err("No steam_api.dll or steam_api64.dll was found in this game.".into())
}

pub fn can_apply_goldberg(game: &Game) -> bool {
    find_steam_api(game).is_ok()
}

pub fn fixes_have_steam_api(game: &Game) -> bool {
    let Ok(fixes) = fixes_dir(game) else {
        return false;
    };
    let Ok(files) = walk(&fixes, 0) else {
        return false;
    };
    files.iter().any(|path| {
        matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("steam_api64.dll") | Some("steam_api.dll")
        )
    })
}

pub fn goldberg_applied(game: &Game) -> bool {
    let Ok(fixes) = fixes_dir(game) else { return false };
    if !read_manifest(&fixes.join(GOLDBERG_MANIFEST)).is_empty() {
        return true;
    }
    // Recover detection if the manifest was lost or the folder was copied in.
    let assets = steamtron_core::paths::layout().goldberg(GSE_TAG);
    let release = [assets.join("regular/x64/steam_api64.dll"), assets.join("regular/x32/steam_api.dll")];
    let Ok(files) = walk(&fixes, 0) else { return false };
    for path in files {
        let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        if name == "steam_appid.txt" && path.parent().and_then(|parent| parent.file_name()).and_then(|value| value.to_str()) == Some("steam_settings") {
            return true;
        }
        if (name == "steam_api64.dll" || name == "steam_api.dll")
            && release.iter().any(|candidate| candidate.is_file() && same_bytes(&path, candidate))
        {
            return true;
        }
    }
    false
}

fn same_bytes(left: &Path, right: &Path) -> bool {
    let (Ok(left), Ok(right)) = (fs::read(left), fs::read(right)) else { return false };
    left == right
}

fn payload_dirs(fixes: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let Ok(files) = walk(fixes, 0) else { return result };
    let mut push = |path: &Path| {
        if !result.contains(&path.to_path_buf()) {
            result.push(path.to_path_buf());
        }
    };
    for path in files {
        let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        let Some(parent) = path.parent() else { continue };
        // The emulator library itself names the payload directory, even when
        // the pack has no `steam_settings` (a bare DLL swap still counts).
        if name == "steam_api64.dll" || name == "steam_api.dll" {
            push(parent);
            continue;
        }
        if (name == "steam_appid.txt" || name == "steam_interfaces.txt")
            && parent.file_name().and_then(|value| value.to_str()) == Some("steam_settings")
        {
            if let Some(payload) = parent.parent() {
                push(payload);
            }
        }
    }
    result
}

pub fn apply_goldberg(game_id: &str, progress: &dyn Fn(&str)) -> Result<serde_json::Value, String> {
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library.game(game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    let fixes = fixes_dir(&game)?;
    fs::create_dir_all(&fixes).map_err(|error| error.to_string())?;
    let assets = ensure_goldberg_assets(progress)?;
    progress("Locating the game's Steam API…");
    let (steam_api, machine) = find_steam_api(&game)?;
    let relative = steam_api.strip_prefix(&game.installation.root).map_err(|_| "Steam API is outside the game folder".to_string())?.to_path_buf();
    let emulator = if machine == 0x8664 {
        assets.join("regular/x64/steam_api64.dll")
    } else {
        assets.join("regular/x32/steam_api.dll")
    };
    if !emulator.is_file() {
        return Err("the GoldBerg release does not contain the required emulator library".into());
    }
    progress("Writing the GoldBerg overlay…");
    let destination = fixes.join(&relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::copy(&emulator, &destination).map_err(|error| error.to_string())?;
    let payload = destination.parent().unwrap_or(&fixes);
    let settings = payload.join("steam_settings");
    fs::create_dir_all(&settings).map_err(|error| error.to_string())?;
    let app_id = game.metadata_app_id.map(|id| id.to_string()).unwrap_or_default();
    if !app_id.is_empty() {
        fs::write(settings.join("steam_appid.txt"), &app_id).map_err(|error| error.to_string())?;
    }
    // GSE's `steam_interfaces.txt` lists interface *versions* the game uses
    // (`SteamUtils007`), never the emulator's exported symbols. Writing
    // `pe_exports` into it made GSE reject every lookup ("Missing interface")
    // and the game exited right after the splash. Keep whatever the fixes pack
    // already ships; when it ships none, GSE falls back to its built-in table.
    let interfaces = settings.join("steam_interfaces.txt");
    let mut files = vec![relative.to_string_lossy().into_owned()];
    if !app_id.is_empty() {
        files.push(
            settings
                .join("steam_appid.txt")
                .strip_prefix(&fixes)
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .into_owned(),
        );
    }
    if interfaces.is_file() {
        files.push(
            interfaces
                .strip_prefix(&fixes)
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .into_owned(),
        );
    }
    files.retain(|file| !file.is_empty());
    write_manifest(&fixes.join(GOLDBERG_MANIFEST), &files)?;
    game.launch.environment.insert("PROTON_DISABLE_LSTEAMCLIENT".into(), "1".into());
    let overrides = game.launch.environment.entry("WINEDLLOVERRIDES".into()).or_default();
    let mut parts: Vec<String> = overrides.split(';').filter(|part| !part.is_empty()).map(str::to_owned).collect();
    let library_name = if machine == 0x8664 { "steam_api64" } else { "steam_api" };
    if !parts.iter().any(|part| part.split('=').next().is_some_and(|name| name.eq_ignore_ascii_case(library_name))) {
        parts.push(format!("{library_name}=n,b"));
    }
    *overrides = parts.join(";");
    game.fixes_enabled = true;
    library.upsert_game(game).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!({ "applied": true, "files": files.len() }))
}

pub fn remove_goldberg(game_id: &str) -> Result<serde_json::Value, String> {
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library.game(game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    let fixes = fixes_dir(&game)?;
    let manifest_path = fixes.join(GOLDBERG_MANIFEST);
    let mut files = read_manifest(&manifest_path);
    for payload in payload_dirs(&fixes) {
        if let Ok(entries) = walk(&payload, 0) {
            for path in entries {
                let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
                if name == "steam_api64.dll" || name == "steam_api.dll" || name == "steam_interfaces.txt" {
                    if let Ok(relative) = path.strip_prefix(&fixes) {
                        files.push(relative.display().to_string());
                    }
                }
            }
        }
        let settings = payload.join("steam_settings");
        if settings.is_dir() {
            files.push(settings.strip_prefix(&fixes).unwrap_or(&settings).display().to_string());
        }
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return Err("GoldBerg is not applied to this fixes folder.".into());
    }
    let mut removed = 0;
    for file in &files {
        let path = fixes.join(file);
        if path.is_dir() {
            let _ = fs::remove_dir_all(&path);
        } else if path.is_file() {
            let _ = fs::remove_file(&path);
        }
        if !path.exists() {
            removed += 1;
        }
    }
    let _ = fs::remove_file(manifest_path);
    game.launch.environment.remove("PROTON_DISABLE_LSTEAMCLIENT");
    if let Some(overrides) = game.launch.environment.get_mut("WINEDLLOVERRIDES") {
        let kept: Vec<&str> = overrides.split(';').filter(|part| {
            let name = part.split('=').next().unwrap_or("").trim();
            !name.eq_ignore_ascii_case("steam_api64") && !name.eq_ignore_ascii_case("steam_api")
        }).filter(|part| !part.is_empty()).collect();
        if kept.is_empty() { game.launch.environment.remove("WINEDLLOVERRIDES"); }
        else { *overrides = kept.join(";"); }
    }
    library.upsert_game(game).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!({ "removed": removed }))
}

pub fn steamless_applied(game: &Game) -> bool {
    let Ok(fixes) = fixes_dir(game) else { return false };
    !read_manifest(&fixes.join(STEAMLESS_MANIFEST)).is_empty()
}

pub fn apply_steamless(game_id: &str, progress: &dyn Fn(&str)) -> Result<serde_json::Value, String> {
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library.game(game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    let fixes = fixes_dir(&game)?;
    fs::create_dir_all(&fixes).map_err(|error| error.to_string())?;
    progress("Unpacking registered launchers…");
    let mut launchers = Vec::new();
    for target in &game.launch_targets {
        // Stored targets can be Linux absolute paths after a library import;
        // resolve_executable re-roots them onto the current library layout.
        if let Some(executable) = steamtron_core::runtime::resolve_executable(
            &game.installation.root,
            &target.executable,
        ) {
            launchers.push(executable);
        }
    }
    if launchers.is_empty() {
        return Err("No launcher was found inside this game's folder.".into());
    }
    let mut files = Vec::new();
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for source in launchers {
        let relative = source.strip_prefix(&game.installation.root).map_err(|_| "launcher is outside the game folder".to_string())?;
        let destination = fixes.join(relative);
        if destination.exists() {
            files.push(relative.to_string_lossy().into_owned());
            results.push(format!("{}: existing", relative.display()));
            continue;
        }
        if let Some(parent) = destination.parent() { fs::create_dir_all(parent).map_err(|error| error.to_string())?; }
        fs::copy(&source, &destination).map_err(|error| error.to_string())?;
        let data = fs::read(&destination).map_err(|error| error.to_string())?;
        let pe = match steamless_core::pe::PeFile::parse(&data) {
            Ok(pe) => pe,
            Err(error) => {
                let _ = fs::remove_file(&destination);
                errors.push(format!("{}: {error}", relative.display()));
                results.push(format!("{}: {error}", relative.display()));
                continue;
            }
        };
        let registry = steamless_core::unpackers::UnpackerRegistry::new();
        let Some(unpacker) = registry.find_unpacker(&pe) else {
            let _ = fs::remove_file(&destination);
            results.push(format!("{}: not SteamStub-packed", relative.display()));
            continue;
        };
        let unpacked = destination.with_extension("unpacked.exe");
        let process = unpacker.unpack(&pe, &unpacked.to_string_lossy());
        if process.is_ok() && unpacked.is_file() {
            let _ = fs::remove_file(&destination);
            fs::rename(&unpacked, &destination).map_err(|error| error.to_string())?;
            files.push(relative.to_string_lossy().into_owned());
            results.push(format!("{}: unpacked", relative.display()));
        } else {
            let _ = fs::remove_file(&destination);
            let _ = fs::remove_file(&unpacked);
            let error = match process {
                Ok(()) => "no unpacked output was produced".to_string(),
                Err(error) => error.to_string(),
            };
            errors.push(format!("{}: {error}", relative.display()));
            results.push(format!("{}: {error}", relative.display()));
        }
    }
    if files.is_empty() {
        if !errors.is_empty() {
            return Err(format!("Steamless could not unpack any launcher: {}", errors.join("; ")));
        }
        return Ok(serde_json::json!({
            "files": [],
            "results": results,
            "applied": false,
            "message": "No SteamStub protection was found in the registered launchers; nothing to unpack."
        }));
    }
    write_manifest(&fixes.join(STEAMLESS_MANIFEST), &files)?;
    game.fixes_enabled = true;
    library.upsert_game(game).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "files": files,
        "results": results,
        "applied": true,
        "message": format!("Unpacked {} launcher{}.", files.len(), if files.len() == 1 { "" } else { "s" })
    }))
}

pub fn remove_steamless(game_id: &str) -> Result<serde_json::Value, String> {
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let game = library.game(game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    let fixes = fixes_dir(&game)?;
    let manifest = fixes.join(STEAMLESS_MANIFEST);
    let files = read_manifest(&manifest);
    if files.is_empty() { return Err("Steamless is not applied to this fixes folder.".into()); }
    let mut removed = 0;
    for file in files { let path = fixes.join(file); if path.is_file() { let _ = fs::remove_file(path); removed += 1; } }
    let _ = fs::remove_file(manifest);
    Ok(serde_json::json!({ "removed": removed }))
}

pub fn status(game: &Game) -> serde_json::Value {
    let fixes = fixes_dir(game).ok();
    let path = fixes.as_ref().map(|path| path.display().to_string()).unwrap_or_default();
    serde_json::json!({
        "path": path,
        "goldberg": goldberg_applied(game),
        "steamless": steamless_applied(game),
    })
}

#[cfg(test)]
mod goldberg_tests {
    use super::*;

    #[test]
    fn payload_dirs_finds_deep_ue4_layout() {
        let temp = tempfile::tempdir().unwrap();
        let payload = temp
            .path()
            .join("Engine/Binaries/ThirdParty/Steamworks/Steamv132/Win64");
        fs::create_dir_all(payload.join("steam_settings")).unwrap();
        fs::write(payload.join("steam_api64.dll"), b"dll").unwrap();
        fs::write(payload.join("steam_settings/steam_appid.txt"), b"424840").unwrap();
        assert_eq!(payload_dirs(temp.path()), vec![payload]);
    }

    #[test]
    fn payload_dirs_uses_bare_dll_swap() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("a/b/c/d/e/f/g");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("steam_api64.dll"), b"dll").unwrap();
        assert_eq!(payload_dirs(temp.path()), vec![dir]);
    }
}
