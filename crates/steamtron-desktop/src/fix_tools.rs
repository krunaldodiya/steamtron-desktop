//! Goldberg emulator and Steamless helpers applied through a game's fixes overlay.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use steamtron_core::library::{Game, Library};

pub const GSE_TAG: &str = "2026_02_16";
const GSE_ASSET: &str = "emu-win-release.7z";
const GSE_SHA256: &str = "cdcee5f8a01b837205aba4d0c6867da98975c74d4f767da60b1724c318cdc2e4";
const GSE_URL: &str = "https://github.com/alex47exe/gse_fork/releases/download/2026_02_16/emu-win-release.7z";
const STEAMLESS_URL: &str = "https://codeload.github.com/krunaldodiya/steamless-linux/tar.gz/refs/heads/main";
const GOLDBERG_MANIFEST: &str = ".steamtron-goldberg.json";
const STEAMLESS_MANIFEST: &str = ".steamtron-steamless.json";

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|error| error.to_string())
}

fn fixes_dir(game: &Game) -> Result<PathBuf, String> {
    let drive = steamtron_core::library::drive_root_for(&game.installation.root)
        .ok_or_else(|| "game is not inside a managed library".to_string())?;
    steamtron_core::library::game_layout(&drive, game)
        .map(|layout| layout.fixes)
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

fn seven_zip() -> Option<PathBuf> {
    let bundled = steamtron_core::paths::layout().seven_zip();
    if bundled.is_file() {
        return Some(bundled);
    }
    let name = if cfg!(windows) { "7z.exe" } else { "7z" };
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

fn ensure_goldberg_assets() -> Result<PathBuf, String> {
    let root = steamtron_core::paths::layout().goldberg(GSE_TAG);
    if root.join("regular/x64/steam_api64.dll").is_file() {
        return Ok(root);
    }
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
    let seven = seven_zip().ok_or_else(|| "7-Zip is required to extract GoldBerg".to_string())?;
    let status = Command::new(&seven)
        .args(["x", "-y", &format!("-o{}", staging.join("out").display()), &archive.display().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        let _ = fs::remove_dir_all(&staging);
        return Err("the GoldBerg release could not be extracted".into());
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

fn walk(root: &Path, depth: usize) -> Result<Vec<PathBuf>, String> {
    if depth > 6 {
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

fn pe_exports(path: &Path) -> Vec<String> {
    let Ok(bytes) = fs::read(path) else { return Vec::new() };
    let Some(names) = (|| -> Option<Vec<String>> {
        if bytes.get(0..2)? != b"MZ" { return None; }
        let pe = u32::from_le_bytes(bytes.get(0x3c..0x40)?.try_into().ok()?) as usize;
        if bytes.get(pe..pe + 4)? != b"PE\0\0" { return None; }
        let coff = pe + 4;
        let sections = u16::from_le_bytes(bytes.get(coff + 2..coff + 4)?.try_into().ok()?) as usize;
        let optional_size = u16::from_le_bytes(bytes.get(coff + 16..coff + 18)?.try_into().ok()?) as usize;
        let optional = coff + 20;
        let magic = u16::from_le_bytes(bytes.get(optional..optional + 2)?.try_into().ok()?);
        let directories = optional + if magic == 0x20b { 112 } else { 96 };
        let export_rva = u32::from_le_bytes(bytes.get(directories..directories + 4)?.try_into().ok()?) as usize;
        if export_rva == 0 { return Some(Vec::new()); }
        let section_table = optional + optional_size;
        let mut export_file = None;
        for index in 0..sections {
            let section = section_table + index * 40;
            let virtual_size = u32::from_le_bytes(bytes.get(section + 8..section + 12)?.try_into().ok()?) as usize;
            let virtual_address = u32::from_le_bytes(bytes.get(section + 12..section + 16)?.try_into().ok()?) as usize;
            let raw_size = u32::from_le_bytes(bytes.get(section + 16..section + 20)?.try_into().ok()?) as usize;
            let raw_pointer = u32::from_le_bytes(bytes.get(section + 20..section + 24)?.try_into().ok()?) as usize;
            if export_rva >= virtual_address && export_rva < virtual_address + virtual_size.max(raw_size) {
                export_file = Some(raw_pointer + export_rva - virtual_address);
                break;
            }
        }
        let export = export_file?;
        let name_count = u32::from_le_bytes(bytes.get(export + 24..export + 28)?.try_into().ok()?) as usize;
        let names_rva = u32::from_le_bytes(bytes.get(export + 32..export + 36)?.try_into().ok()?) as usize;
        let mut names_file = None;
        for index in 0..sections {
            let section = section_table + index * 40;
            let virtual_address = u32::from_le_bytes(bytes.get(section + 12..section + 16)?.try_into().ok()?) as usize;
            let raw_size = u32::from_le_bytes(bytes.get(section + 16..section + 20)?.try_into().ok()?) as usize;
            let raw_pointer = u32::from_le_bytes(bytes.get(section + 20..section + 24)?.try_into().ok()?) as usize;
            if names_rva >= virtual_address && names_rva < virtual_address + raw_size {
                names_file = Some(raw_pointer + names_rva - virtual_address);
                break;
            }
        }
        let names_file = names_file?;
        let mut result = Vec::new();
        for index in 0..name_count.min(4096) {
            let pointer = u32::from_le_bytes(bytes.get(names_file + index * 4..names_file + index * 4 + 4)?.try_into().ok()?) as usize;
            let mut string_file = None;
            for section_index in 0..sections {
                let section = section_table + section_index * 40;
                let virtual_address = u32::from_le_bytes(bytes.get(section + 12..section + 16)?.try_into().ok()?) as usize;
                let raw_size = u32::from_le_bytes(bytes.get(section + 16..section + 20)?.try_into().ok()?) as usize;
                let raw_pointer = u32::from_le_bytes(bytes.get(section + 20..section + 24)?.try_into().ok()?) as usize;
                if pointer >= virtual_address && pointer < virtual_address + raw_size {
                    string_file = Some(raw_pointer + pointer - virtual_address);
                    break;
                }
            }
            let Some(start) = string_file else { continue };
            let Some(end) = bytes[start..].iter().position(|byte| *byte == 0).map(|length| start + length) else { continue };
            let Ok(name) = std::str::from_utf8(&bytes[start..end]) else { continue };
            if name.starts_with("Steam") && name.len() < 128 {
                result.push(name.to_owned());
            }
        }
        result.sort();
        result.dedup();
        Some(result)
    })() else { return Vec::new() };
    names
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
        if let Some(parent) = target.executable.parent() {
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

pub fn goldberg_applied(game: &Game) -> bool {
    let Ok(fixes) = fixes_dir(game) else { return false };
    !read_manifest(&fixes.join(GOLDBERG_MANIFEST)).is_empty()
}

pub fn apply_goldberg(game_id: &str) -> Result<serde_json::Value, String> {
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library.game(game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    let fixes = fixes_dir(&game)?;
    fs::create_dir_all(&fixes).map_err(|error| error.to_string())?;
    let assets = ensure_goldberg_assets()?;
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
    let exports = pe_exports(&steam_api);
    let interfaces = exports.join("\n");
    fs::write(payload.join("steam_interfaces.txt"), &interfaces).map_err(|error| error.to_string())?;
    fs::write(settings.join("steam_interfaces.txt"), &interfaces).map_err(|error| error.to_string())?;
    let mut files = vec![
        relative.to_string_lossy().into_owned(),
        payload.strip_prefix(&fixes).unwrap_or(Path::new("")).join("steam_interfaces.txt").to_string_lossy().into_owned(),
    ];
    for name in ["steam_settings/steam_appid.txt", "steam_settings/steam_interfaces.txt"] {
        files.push(payload.join(name).strip_prefix(&fixes).unwrap_or(Path::new("")).to_string_lossy().into_owned());
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
    let files = read_manifest(&manifest_path);
    if files.is_empty() {
        return Err("GoldBerg is not applied to this fixes folder.".into());
    }
    let mut removed = 0;
    for file in files {
        let path = fixes.join(&file);
        if path.is_file() {
            let _ = fs::remove_file(&path);
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

fn ensure_steamless_assets() -> Result<PathBuf, String> {
    let root = steamtron_core::paths::layout().steamless();
    if root.join("steamless").is_file() && root.join("Steamless.CLI.exe").is_file() {
        return Ok(root);
    }
    let parent = root.parent().ok_or_else(|| "invalid Steamless cache path".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staging = parent.join(format!(".steamless-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let archive = staging.join("steamless.tar.gz");
    download(STEAMLESS_URL, &archive, None, 64 * 1024 * 1024)?;
    tar::Archive::new(GzDecoder::new(fs::File::open(&archive).map_err(|error| error.to_string())?))
        .unpack(&staging)
        .map_err(|error| format!("Steamless archive is invalid: {error}"))?;
    let mut found = None;
    for path in walk(&staging, 0)? {
        if path.file_name().and_then(|value| value.to_str()) == Some("steamless") {
            if path.parent().map(|parent| parent.join("Steamless.CLI.exe").is_file()).unwrap_or(false) {
                found = path.parent().map(Path::to_path_buf);
                break;
            }
        }
    }
    let Some(found) = found else {
        let _ = fs::remove_dir_all(&staging);
        return Err("the Steamless download does not contain the expected tool".into());
    };
    if root.exists() { fs::remove_dir_all(&root).map_err(|error| error.to_string())?; }
    fs::rename(&found, &root).map_err(|error| error.to_string())?;
    let _ = fs::remove_dir_all(&staging);
    Ok(root)
}

pub fn steamless_applied(game: &Game) -> bool {
    let Ok(fixes) = fixes_dir(game) else { return false };
    !read_manifest(&fixes.join(STEAMLESS_MANIFEST)).is_empty()
}

pub fn apply_steamless(game_id: &str) -> Result<serde_json::Value, String> {
    if std::env::var_os("PATH").map(|path| std::env::split_paths(&path).any(|dir| dir.join("wine").is_file())).unwrap_or(false) == false {
        return Err("Steamless requires Wine. Install Wine and try again.".into());
    }
    let mut library = Library::load().map_err(|error| error.to_string())?;
    let mut game = library.game(game_id).cloned().ok_or_else(|| "unknown game".to_string())?;
    let fixes = fixes_dir(&game)?;
    fs::create_dir_all(&fixes).map_err(|error| error.to_string())?;
    let tool = ensure_steamless_assets()?;
    let mut launchers = Vec::new();
    for target in &game.launch_targets {
        if target.executable.is_file() {
            launchers.push(target.executable.clone());
        }
    }
    if launchers.is_empty() {
        return Err("No launcher was found inside this game's folder.".into());
    }
    let mut files = Vec::new();
    let mut results = Vec::new();
    for source in launchers {
        let relative = source.strip_prefix(&game.installation.root).map_err(|_| "launcher is outside the game folder".to_string())?;
        let destination = fixes.join(relative);
        if destination.exists() {
            results.push(format!("{}: existing", relative.display()));
            continue;
        }
        if let Some(parent) = destination.parent() { fs::create_dir_all(parent).map_err(|error| error.to_string())?; }
        fs::copy(&source, &destination).map_err(|error| error.to_string())?;
        let status = Command::new(tool.join("steamless"))
            .args(["--inplace", &destination.display().to_string()])
            .current_dir(&tool)
            .env("STEAMLESS_HOME", &tool)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .status().map_err(|error| error.to_string())?;
        let unpacked = destination.with_file_name(format!("{}.unpacked.exe", destination.file_name().unwrap_or_default().to_string_lossy()));
        if status.success() && unpacked.is_file() {
            fs::rename(&unpacked, &destination).map_err(|error| error.to_string())?;
            files.push(relative.to_string_lossy().into_owned());
            results.push(format!("{}: unpacked", relative.display()));
        } else {
            let _ = fs::remove_file(&destination);
            let _ = fs::remove_file(&unpacked);
            results.push(format!("{}: skipped", relative.display()));
        }
    }
    if files.is_empty() {
        return Err("Steamless could not unpack any launcher.".into());
    }
    write_manifest(&fixes.join(STEAMLESS_MANIFEST), &files)?;
    game.fixes_enabled = true;
    library.upsert_game(game).map_err(|error| error.to_string())?;
    library.save().map_err(|error| error.to_string())?;
    Ok(serde_json::json!({ "files": files, "results": results }))
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
