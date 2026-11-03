//! Steam `localconfig.vdf` launch-option read/write helpers.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

const MARKER: &str = "--steamtron-overlay";

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("Steam user configuration was not found")]
    ConfigNotFound,
    #[error("App {0} has no apps block in localconfig.vdf")]
    AppNotFound(String),
    #[error("invalid VDF near byte {0}")]
    InvalidVdf(usize),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl LaunchError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

pub fn vdf_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn launch_option_value(exe: &Path, appid: &str, common: &Path, fixes: &Path) -> String {
    format!(
        "\"{}\" {MARKER} --appid {appid} --common \"{}\" --fixes \"{}\" -- %command%",
        vdf_string(&exe.display().to_string()),
        vdf_string(&common.display().to_string()),
        vdf_string(&fixes.display().to_string())
    )
}

fn skip_space(text: &str, mut pos: usize) -> usize {
    let bytes = text.as_bytes();
    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
        } else if bytes[pos..].starts_with(b"//") {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
        } else {
            break;
        }
    }
    pos
}

fn quoted(text: &str, mut pos: usize) -> Option<(usize, usize, String)> {
    let bytes = text.as_bytes();
    if bytes.get(pos) != Some(&b'"') {
        return None;
    }
    let start = pos;
    pos += 1;
    let mut value = String::new();
    while pos < bytes.len() {
        match bytes[pos] {
            b'"' => return Some((start, pos + 1, value)),
            b'\\' if pos + 1 < bytes.len() => {
                pos += 1;
                value.push(bytes[pos] as char);
                pos += 1;
            }
            byte => {
                value.push(byte as char);
                pos += 1;
            }
        }
    }
    None
}

fn match_brace(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut pos = open;
    while pos < bytes.len() {
        match bytes[pos] {
            b'"' => {
                pos = quoted(text, pos)?.1;
            }
            b'{' => {
                depth += 1;
                pos += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                pos += 1;
                if depth == 0 {
                    return Some(pos - 1);
                }
            }
            _ => pos += 1,
        }
    }
    None
}

fn direct_child(text: &str, open: usize, close: usize, key: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut pos = open + 1;
    while pos < close {
        pos = skip_space(text, pos);
        if pos >= close {
            break;
        }
        if bytes[pos] == b'}' {
            break;
        }
        if bytes[pos] != b'"' {
            pos += 1;
            continue;
        }
        let (token_start, token_end, name) = quoted(text, pos)?;
        let after = skip_space(text, token_end);
        if bytes.get(after) == Some(&b'{') {
            let block_close = match_brace(text, after)?;
            if name == key {
                return Some((token_start, block_close + 1));
            }
            pos = block_close + 1;
        } else {
            pos = token_end;
        }
    }
    None
}

fn brace_in_range(text: &str, range: (usize, usize)) -> usize {
    text.as_bytes()[range.0..range.1]
        .iter()
        .position(|byte| *byte == b'{')
        .map(|offset| range.0 + offset)
        .unwrap_or(range.0)
}

fn path_child(text: &str, mut range: (usize, usize), keys: &[&str]) -> Option<(usize, usize)> {
    for key in keys {
        range = direct_child(text, brace_in_range(text, range), range.1, key)?;
    }
    Some(range)
}

fn app_block(text: &str, appid: &str) -> Option<(usize, usize, usize)> {
    let root = find_named_block(text, "UserLocalConfigStore")?;
    let steam = path_child(text, root, &["Software", "Valve", "Steam"])?;
    let apps = direct_child(text, brace_in_range(text, steam), steam.1, "apps")?;
    let app = direct_child(text, brace_in_range(text, apps), apps.1, appid)?;
    let open = text.as_bytes()[app.0..app.1].iter().position(|byte| *byte == b'{')? + app.0;
    Some((open, app.1, app.0))
}

fn find_named_block(text: &str, key: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() {
        if bytes[pos] != b'"' {
            pos += 1;
            continue;
        }
        let (start, end, name) = quoted(text, pos)?;
        let after = skip_space(text, end);
        if bytes.get(after) == Some(&b'{') {
            let close = match_brace(text, after)?;
            if name == key {
                return Some((start, close + 1));
            }
            pos = close + 1;
        } else {
            pos = end;
        }
    }
    None
}

fn replace_launch_line(block: &str, value: Option<&str>) -> String {
    let regex = regex::Regex::new(r#"(?m)^[ \t]*"LaunchOptions"[ \t]+"(?:[^"\\]|\\.)*"[ \t]*\r?\n?"#).unwrap();
    let found = regex.is_match(block);
    let replacement = value
        .map(|value| format!("\t\t\t\"LaunchOptions\"\t\t\"{}\"\n", vdf_string(value)))
        .unwrap_or_default();
    if found {
        return regex.replace(block, replacement).into_owned();
    }
    let Some(_value) = value else {
        return block.to_owned();
    };
    let Some(close) = block.rfind('}') else {
        return block.to_owned();
    };
    let mut result = String::with_capacity(block.len() + replacement.len());
    result.push_str(&block[..close]);
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&replacement);
    result.push_str(&block[close..]);
    result
}

pub fn set_launch_option_in_text(text: &str, appid: &str, value: Option<&str>) -> Result<String, LaunchError> {
    let Some((open, close, _)) = app_block(text, appid) else {
        let Some(value) = value else {
            return Ok(text.to_owned());
        };
        let Some((_apps_open, apps_close)) = apps_block(text) else {
            return Err(LaunchError::AppNotFound(appid.to_owned()));
        };
        let insertion = format!(
            "\t\t\t\"{appid}\"\n\t\t\t{{\n\t\t\t\t\"LaunchOptions\"\t\t\"{}\"\n\t\t\t}}\n",
            vdf_string(value)
        );
        let insert_at = apps_close.saturating_sub(1);
        let mut result = String::with_capacity(text.len() + insertion.len());
        result.push_str(&text[..insert_at]);
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&insertion);
        result.push_str(&text[insert_at..]);
        return Ok(result);
    };
    let block = &text[open..close];
    let updated = replace_launch_line(block, value);
    let mut result = String::with_capacity(text.len() + updated.len());
    result.push_str(&text[..open]);
    result.push_str(&updated);
    result.push_str(&text[close..]);
    Ok(result)
}

fn apps_block(text: &str) -> Option<(usize, usize)> {
    let root = find_named_block(text, "UserLocalConfigStore")?;
    let steam = path_child(text, root, &["Software", "Valve", "Steam"])?;
    let apps = direct_child(text, brace_in_range(text, steam), steam.1, "apps")?;
    Some((brace_in_range(text, apps), apps.1))
}

pub fn launch_option_in_text(text: &str, appid: &str) -> Option<String> {
    let (open, close, _) = app_block(text, appid)?;
    let block = &text[open..close];
    let regex = regex::Regex::new(r#"(?m)^[ \t]*"LaunchOptions"[ \t]+"((?:[^"\\]|\\.)*)""#).unwrap();
    regex
        .captures(block)
        .map(|captures| captures[1].replace("\\\"", "\"").replace("\\\\", "\\"))
}

pub fn localconfig_paths() -> Result<Vec<PathBuf>, LaunchError> {
    let root = crate::steam::steam_root().map_err(|_| LaunchError::ConfigNotFound)?;
    let userdata = root.join("userdata");
    let Ok(entries) = fs::read_dir(userdata) else {
        return Ok(Vec::new());
    };
    let mut paths = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if !name.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let path = entry.path().join("config/localconfig.vdf");
            path.is_file().then_some(path)
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

pub fn set_launch_option(appid: &str, value: Option<&str>) -> Result<Vec<PathBuf>, LaunchError> {
    let paths = localconfig_paths()?;
    if paths.is_empty() {
        return Err(LaunchError::ConfigNotFound);
    }
    let mut changed = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|error| LaunchError::io(&path, error))?;
        let updated = set_launch_option_in_text(&text, appid, value)?;
        if updated != text {
            let temporary = path.with_file_name("localconfig.vdf.steamtron.tmp");
            fs::write(&temporary, updated).map_err(|error| LaunchError::io(&temporary, error))?;
            fs::rename(&temporary, &path).map_err(|error| LaunchError::io(&path, error))?;
            changed.push(path);
        }
    }
    Ok(changed)
}

pub fn has_launch_option(appid: &str) -> bool {
    let Ok(paths) = localconfig_paths() else {
        return false;
    };
    paths.iter().any(|path| {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| launch_option_in_text(&text, appid))
            .is_some_and(|value| value.contains(MARKER))
    })
}

pub fn clear_managed_launch_option(appid: &str) -> Result<Vec<PathBuf>, LaunchError> {
    let paths = localconfig_paths()?;
    let mut changed = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|error| LaunchError::io(&path, error))?;
        let current = launch_option_in_text(&text, appid);
        if current.as_deref().is_some_and(|value| value.contains(MARKER)) {
            let updated = set_launch_option_in_text(&text, appid, None)?;
            let temporary = path.with_file_name("localconfig.vdf.steamtron.tmp");
            fs::write(&temporary, updated).map_err(|error| LaunchError::io(&temporary, error))?;
            fs::rename(&temporary, &path).map_err(|error| LaunchError::io(&path, error))?;
            changed.push(path);
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> String {
        r#""UserLocalConfigStore"
{
    "Software"
    {
        "Valve"
        {
            "Steam"
            {
                "apps"
                {
                    "42"
                    {
                        "name" "Game"
                    }
                    "43"
                    {
                        "LaunchOptions" "custom %command%"
                    }
                }
            }
        }
    }
}
"#
        .to_owned()
    }

    #[test]
    fn inserts_and_reads_launch_options() {
        let value = "\"/tmp/SteamTron\" --steamtron-overlay -- _ %command%";
        let updated = set_launch_option_in_text(&sample(), "42", Some(value)).unwrap();
        assert!(launch_option_in_text(&updated, "42").unwrap().contains("--steamtron-overlay"));
        assert!(updated.contains("custom %command%"));
    }

    #[test]
    fn inserts_missing_app_entry() {
        let updated = set_launch_option_in_text(&sample(), "44", Some("wrapper -- %command%")).unwrap();
        assert!(launch_option_in_text(&updated, "44").unwrap().contains("wrapper"));
    }

    #[test]
    fn replaces_and_clears_only_selected_app() {
        let updated = set_launch_option_in_text(&sample(), "43", None).unwrap();
        assert!(!launch_option_in_text(&updated, "43").is_some());
        assert!(updated.contains("\"42\""));
    }
}
