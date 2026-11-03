//! Archived manifest builds, installed-build detection, and per-game pins.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use serde::Serialize;

use steamtron_core::steam;

use crate::LibraryError;

const MAGIC_METADATA: u32 = 0x1F4812BE;
const MAGIC_TERMINAL: u32 = 0x32C415AB;

#[derive(Debug, Clone, Serialize)]
pub struct UpdateVersion {
    pub date: String,
    pub gids: BTreeMap<String, String>,
    pub latest: bool,
    pub installed: bool,
    pub pinned: bool,
    pub depots: usize,
    pub buildid: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Updates {
    pub appid: String,
    pub latest: String,
    pub installed: String,
    pub installed_present: bool,
    pub installed_buildid: String,
    pub pinned: String,
    pub locked: bool,
    pub versions: Vec<UpdateVersion>,
}

#[derive(Debug, Clone)]
struct ManifestMeta {
    creation_time: i64,
    gid: String,
}

fn u32_at(data: &[u8], pos: usize) -> Option<u32> {
    data.get(pos..pos + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn varint(data: &[u8], mut pos: usize, limit: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0;
    while pos < limit && shift <= 63 {
        let byte = *data.get(pos)?;
        pos += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte < 0x80 {
            return Some((value, pos));
        }
        shift += 7;
    }
    None
}

fn manifest_metadata(path: &Path) -> Option<ManifestMeta> {
    let data = fs::read(path).ok()?;
    let name = path.file_name()?.to_str()?;
    let mut pos = 0usize;
    let mut depot = None;
    let mut gid = None;
    let mut creation_time = None;
    while pos + 8 <= data.len() {
        let magic = u32_at(&data, pos)?;
        if magic == MAGIC_TERMINAL {
            break;
        }
        let length = u32_at(&data, pos + 4)? as usize;
        let first = pos + 8;
        let last = pos + 7 + length;
        if last > data.len() {
            break;
        }
        if magic == MAGIC_METADATA {
            let mut inner = first;
            while inner < last {
                let Some((tag, next)) = varint(&data, inner, last) else {
                    break;
                };
                inner = next;
                let field = tag >> 3;
                let wire = tag & 7;
                if wire == 0 {
                    let Some((value, next)) = varint(&data, inner, last) else {
                        break;
                    };
                    inner = next;
                    match field {
                        1 => depot = Some(value.to_string()),
                        2 => gid = Some(value.to_string()),
                        3 => creation_time = Some(value as i64),
                        _ => {}
                    }
                } else if wire == 1 {
                    inner += 8;
                } else if wire == 2 {
                    let Some((size, next)) = varint(&data, inner, last) else {
                        break;
                    };
                    inner = next + size as usize;
                } else if wire == 5 {
                    inner += 4;
                } else {
                    break;
                }
            }
        }
        pos = last + 1;
    }
    let (name_depot, name_gid) = name.split_once('_')?;
    let name_gid = name_gid.strip_suffix(".manifest")?;
    let depot = depot.unwrap_or_else(|| name_depot.to_owned());
    let gid = gid.unwrap_or_else(|| name_gid.to_owned());
    let creation_time = creation_time.unwrap_or_else(|| {
        path.metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    });
    let _ = depot;
    Some(ManifestMeta { creation_time, gid })
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from).map(|home| home.join(".config")))?;
    Some(base.join("SLSsteam/config.yaml"))
}

fn manifest_store() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from).map(|home| home.join(".config")))?;
    Some(base.join("SLSsteam/manifests"))
}

fn lua_depots(appid: &str) -> BTreeMap<String, String> {
    let Some(root) = steam::steam_root().ok() else {
        return BTreeMap::new();
    };
    let text = fs::read_to_string(root.join("config/stplug-in").join(format!("{appid}.lua")))
        .unwrap_or_default();
    let regex = Regex::new(r#"setManifestid\s*\(\s*(\d+)\s*,\s*["'](\d+)["']"#).unwrap();
    let mut depots: BTreeMap<String, String> = regex
        .captures_iter(&text)
        .map(|captures| (captures[1].to_owned(), captures[2].to_owned()))
        .collect();
    if depots.is_empty() {
        depots = installed_gids(appid)
            .into_iter()
            .map(|(depot, gid)| (depot, gid.unwrap_or_default()))
            .collect();
    }
    depots
}

fn appmanifest_status(appid: &str) -> (bool, String) {
    let Ok(libraries) = steam::libraries() else {
        return (false, String::new());
    };
    for library in libraries {
        let path = library.join(format!("appmanifest_{appid}.acf"));
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let field = |key: &str| {
            Regex::new(&format!(r#"(?m)^\s*"{key}"\s+"([^"]+)""#))
                .ok()
                .and_then(|regex| regex.captures(&text))
                .map(|captures| captures[1].to_owned())
        };
        let state = field("StateFlags")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let size = field("SizeOnDisk")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let buildid = field("buildid").unwrap_or_default();
        let installed = state & 4 != 0 && size > 0 && buildid != "0";
        return (installed, buildid);
    }
    (false, String::new())
}

fn installed_gids(appid: &str) -> BTreeMap<String, Option<String>> {
    if !appmanifest_status(appid).0 {
        return BTreeMap::new();
    }
    let mut result = BTreeMap::new();
    let Ok(libraries) = steam::libraries() else {
        return result;
    };
    for library in libraries {
        let path = library.join(format!("appmanifest_{appid}.acf"));
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let regex = Regex::new(r#"(?m)^\s*"(\d+)"\s*\{\s*"manifest"\s+"(\d+)""#).unwrap();
        for captures in regex.captures_iter(&text) {
            result.insert(captures[1].to_owned(), Some(captures[2].to_owned()));
        }
    }
    result
}

fn installed_buildid(appid: &str) -> String {
    let (installed, buildid) = appmanifest_status(appid);
    if installed { buildid } else { String::new() }
}

fn preferred_gids(depots: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let Some(store) = manifest_store() else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    for depot in depots.keys() {
        if let Ok(value) = fs::read_to_string(store.join(format!(".preferred_{depot}"))) {
            let value = value.trim();
            if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
                result.insert(depot.clone(), value.to_owned());
            }
        }
    }
    result
}

fn day(timestamp: i64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .map(|value| value.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn day_end(value: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    date.and_hms_opt(23, 59, 59)
        .map(|date| date.and_utc().timestamp())
}

fn sources(depots: &BTreeMap<String, String>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(store) = manifest_store() {
        if let Ok(entries) = fs::read_dir(store) {
            paths.extend(entries.flatten().map(|entry| entry.path()));
        }
    }
    if let Ok(root) = steam::steam_root() {
        if let Ok(entries) = fs::read_dir(root.join("steamapps/depotcache")) {
            paths.extend(entries.flatten().map(|entry| entry.path()));
        }
    }
    if let Ok(libraries) = steam::libraries() {
        for library in libraries {
            if let Ok(entries) = fs::read_dir(library.join("depotcache")) {
                paths.extend(entries.flatten().map(|entry| entry.path()));
            }
        }
    }
    let file = Regex::new(r"^(\d+)_(\d+)\.manifest$").unwrap();
    paths
        .into_iter()
        .filter(|path| {
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                return false;
            };
            let Some(captures) = file.captures(name) else {
                return false;
            };
            depots.contains_key(&captures[1]) && seen.insert(name.to_owned())
        })
        .collect()
}

pub fn available_updates(appid: &str) -> Result<Updates, LibraryError> {
    if !appid.bytes().all(|byte| byte.is_ascii_digit()) || appid.is_empty() {
        return Err(LibraryError::InvalidAppId);
    }
    let depots = lua_depots(appid);
    if depots.is_empty() {
        return Err(LibraryError::Update(
            "This game has no managed stplug-in manifest.".into(),
        ));
    }
    let mut by_depot: BTreeMap<String, Vec<ManifestMeta>> = depots
        .keys()
        .cloned()
        .map(|depot| (depot, Vec::new()))
        .collect();
    for path in sources(&depots) {
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some((depot, _)) = name.split_once('_') else {
            continue;
        };
        if let Some(metadata) = manifest_metadata(&path) {
            by_depot.entry(depot.to_owned()).or_default().push(metadata);
        }
    }
    let mut dates = BTreeSet::new();
    for rows in by_depot.values() {
        dates.extend(rows.iter().map(|row| day(row.creation_time)));
    }
    let mut versions: Vec<UpdateVersion> = Vec::new();
    for date in dates.iter().rev() {
        if date == "unknown" {
            continue;
        }
        let Some(end) = day_end(date) else {
            continue;
        };
        let mut gids = BTreeMap::new();
        for (depot, rows) in &by_depot {
            if let Some(latest) = rows
                .iter()
                .filter(|row| row.creation_time <= end)
                .max_by_key(|row| (row.creation_time, row.gid.parse::<u64>().unwrap_or(0)))
            {
                gids.insert(depot.clone(), latest.gid.clone());
            }
        }
        if !gids.is_empty() {
            versions.push(UpdateVersion {
                date: date.clone(),
                depots: gids.len(),
                gids,
                latest: false,
                installed: false,
                pinned: false,
                buildid: String::new(),
            });
        }
    }
    let installed = installed_gids(appid);
    let installed: BTreeMap<String, String> = installed
        .into_iter()
        .filter(|(depot, _)| depots.contains_key(depot))
        .filter_map(|(depot, gid)| gid.map(|gid| (depot, gid)))
        .collect();
    let preferred = preferred_gids(&depots);
    let configured = configured_pin_gids(appid);
    let pin_source = configured.clone().unwrap_or(preferred);
    let pinned = versions
        .iter()
        .find(|version| {
            version.gids.keys().collect::<BTreeSet<_>>() == pin_source.keys().collect::<BTreeSet<_>>()
                && pin_source.iter().all(|(depot, gid)| version.gids.get(depot) == Some(gid))
        })
        .map(|version| version.date.clone())
        .unwrap_or_default();
    let installed_date = versions
        .iter()
        .find(|version| {
            installed
                .iter()
                .all(|(depot, gid)| version.gids.get(depot) == Some(gid))
                && !installed.is_empty()
        })
        .map(|version| version.date.clone())
        .unwrap_or_default();
    let latest = versions.first().map(|version| version.date.clone()).unwrap_or_default();
    let buildid = installed_buildid(appid);
    for version in &mut versions {
        version.latest = version.date == latest;
        version.installed = version.date == installed_date;
        version.pinned = version.date == pinned;
        version.buildid = if version.installed { buildid.clone() } else { String::new() };
    }
    Ok(Updates {
        appid: appid.to_owned(),
        latest,
        installed: installed_date,
        installed_present: !installed.is_empty(),
        installed_buildid: buildid,
        pinned,
        locked: configured.is_some(),
        versions,
    })
}

pub fn clear_pin(appid: &str) -> Result<Updates, LibraryError> {
    let depots = lua_depots(appid);
    if let Some(store) = manifest_store() {
        for depot in depots.keys() {
            let _ = fs::remove_file(store.join(format!(".preferred_{depot}")));
        }
    }
    write_manifest_pin(appid, None)?;
    available_updates(appid)
}

pub fn pin_update(appid: &str, date: &str) -> Result<Updates, LibraryError> {
    let data = available_updates(appid)?;
    let Some(version) = data.versions.iter().find(|version| version.date == date) else {
        return Err(LibraryError::Update("That archived version is not available.".into()));
    };
    let depots = lua_depots(appid);
    let Some(store) = manifest_store() else {
        return Err(LibraryError::Update("SLSsteam manifest store was not found.".into()));
    };
    fs::create_dir_all(&store).map_err(|error| LibraryError::io(&store, error))?;
    for depot in depots.keys() {
        let preferred = store.join(format!(".preferred_{depot}"));
        let Some(gid) = version.gids.get(depot) else {
            let _ = fs::remove_file(preferred);
            continue;
        };
        fs::write(&preferred, format!("{gid}\n")).map_err(|error| LibraryError::io(&preferred, error))?;
        let manifest = store.join(format!("{depot}_{gid}.manifest"));
        if !manifest.is_file() {
            let mut candidates = Vec::new();
            if let Ok(root) = steam::steam_root() {
                candidates.extend(
                    fs::read_dir(root.join("steamapps/depotcache"))
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some(&format!("{depot}_{gid}.manifest"))),
                );
            }
            if let Ok(libraries) = steam::libraries() {
                for library in libraries {
                    candidates.extend(
                        fs::read_dir(library.join("depotcache"))
                            .into_iter()
                            .flatten()
                            .flatten()
                            .map(|entry| entry.path())
                            .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some(&format!("{depot}_{gid}.manifest"))),
                    );
                }
            }
            let Some(source) = candidates.into_iter().next() else {
                return Err(LibraryError::Update(format!(
                    "Missing archived manifest for depot {depot}."
                )));
            };
            fs::copy(&source, &manifest).map_err(|error| LibraryError::io(&manifest, error))?;
        }
    }
    write_manifest_pin(appid, Some(&version.gids))?;
    available_updates(appid)
}

fn configured_pin_gids(appid: &str) -> Option<BTreeMap<String, String>> {
    let path = config_path()?;
    let text = fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|line| line.starts_with("ManifestPins:"))?;
    let mut inside = false;
    let mut found = false;
    let mut depots = BTreeMap::new();
    for line in &lines[start + 1..] {
        if !line.trim().is_empty() && !line.starts_with(char::is_whitespace) {
            break;
        }
        if let Some(captures) = Regex::new(r"^  (\d+):\s*$").unwrap().captures(line) {
            if inside {
                break;
            }
            if &captures[1] == appid {
                inside = true;
                found = true;
            }
            continue;
        }
        if inside {
            if let Some(captures) = Regex::new(r#"^\s+(\d+):\s*["']?(\d+)"#).unwrap().captures(line) {
                depots.insert(captures[1].to_owned(), captures[2].to_owned());
            }
        }
    }
    if found { Some(depots) } else { None }
}

fn write_manifest_pin(appid: &str, gids: Option<&BTreeMap<String, String>>) -> Result<(), LibraryError> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let updated = splice_app_pin(&text, appid, gids);
    if updated == text {
        return Ok(());
    }
    let temporary = path.with_file_name("config.yaml.steamtron.tmp");
    fs::write(&temporary, updated).map_err(|error| LibraryError::io(&temporary, error))?;
    fs::rename(&temporary, &path).map_err(|error| LibraryError::io(&path, error))
}

fn splice_app_pin(text: &str, appid: &str, gids: Option<&BTreeMap<String, String>>) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let start = lines.iter().position(|line| line.starts_with("ManifestPins:"));
    let pin_lines = |gids: &BTreeMap<String, String>| {
        let mut result = vec![format!("  {appid}:"), "    locked: true".into(), "    depots:".into()];
        for (depot, gid) in gids {
            result.push(format!("      {depot}: \"{gid}\""));
        }
        result
    };
    let Some(start) = start else {
        let Some(gids) = gids else {
            return text.to_owned();
        };
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
        lines.push("ManifestPins:".into());
        lines.extend(pin_lines(gids));
        return format!("{}\n", lines.join("\n"));
    };
    let mut end = lines.len();
    for index in start + 1..lines.len() {
        if !lines[index].trim().is_empty() && !lines[index].starts_with(char::is_whitespace) {
            end = index;
            break;
        }
    }
    let app_header = Regex::new(r"^  \d+:\s*$").unwrap();
    let mut prefix = Vec::new();
    let mut groups: Vec<Vec<String>> = Vec::new();
    for line in &lines[start + 1..end] {
        if app_header.is_match(line) {
            groups.push(vec![line.clone()]);
        } else if let Some(group) = groups.last_mut() {
            group.push(line.clone());
        } else {
            prefix.push(line.clone());
        }
    }
    groups.retain(|group| !Regex::new(&format!(r"^  {appid}:\s*$")).unwrap().is_match(&group[0]));
    let mut rebuilt = prefix;
    for group in groups {
        rebuilt.extend(group);
    }
    if let Some(gids) = gids {
        rebuilt.extend(pin_lines(gids));
    }
    while rebuilt.last().is_some_and(|line| line.trim().is_empty()) {
        rebuilt.pop();
    }
    if !rebuilt.iter().any(|line| line.starts_with("  ")) && gids.is_none() {
        let mut result = lines[..start].to_vec();
        result.extend_from_slice(&lines[end..]);
        return format!("{}\n", result.join("\n"));
    }
    let mut result = lines[..start + 1].to_vec();
    result.extend(rebuilt);
    result.extend_from_slice(&lines[end..]);
    format!("{}\n", result.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_splice_preserves_other_apps() {
        let text = "ManifestPins:\n  1:\n    locked: true\n    depots:\n      11: \"111\"\n  2:\n    locked: true\n    depots:\n      22: \"222\"\n";
        let mut gids = BTreeMap::new();
        gids.insert("11".to_owned(), "333".to_owned());
        let updated = splice_app_pin(text, "1", Some(&gids));
        assert!(updated.contains("  1:\n    locked: true\n    depots:\n      11: \"333\""));
        assert!(updated.contains("  2:\n    locked: true\n    depots:\n      22: \"222\""));
        assert_eq!(updated.matches("  1:").count(), 1);
    }

    #[test]
    fn pin_splice_removes_only_selected_app() {
        let text = "ManifestPins:\n  1:\n    locked: true\n    depots:\n      11: \"111\"\n  2:\n    locked: true\n    depots:\n      22: \"222\"\n";
        let updated = splice_app_pin(text, "1", None);
        assert!(!updated.contains("  1:"));
        assert!(updated.contains("  2:"));
        assert!(updated.contains("ManifestPins:"));
    }

    #[test]
    fn pin_splice_creates_block() {
        let mut gids = BTreeMap::new();
        gids.insert("11".to_owned(), "333".to_owned());
        let updated = splice_app_pin("Other: yes\n", "1", Some(&gids));
        assert!(updated.starts_with("Other: yes\nManifestPins:\n"));
        assert!(updated.contains("      11: \"333\""));
    }
}
