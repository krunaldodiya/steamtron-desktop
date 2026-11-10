//! Recent-play history written only after a launch request succeeds.
//!
//! Adding a game, saving metadata, or completing a setup never records a play.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Serialize, Deserialize)]
struct PlayRecord {
    game_id: String,
    observed_at: i64,
}

fn safe_id(game_id: &str) -> String {
    game_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn record_path(root: &std::path::Path, game_id: &str) -> PathBuf {
    root.join(format!("{}.json", safe_id(game_id)))
}

fn record_in(root: &std::path::Path, game_id: &str, observed_at: i64) -> std::io::Result<()> {
    if game_id.is_empty() || observed_at <= 0 {
        return Ok(());
    }
    let path = record_path(root, game_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let payload = serde_json::to_vec(&PlayRecord {
        game_id: game_id.to_owned(),
        observed_at,
    })
    .map_err(std::io::Error::other)?;
    fs::write(&temporary, payload)?;
    fs::rename(&temporary, &path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

/// Persist the latest play for one game. Failures are non-fatal to launches.
pub fn record(game_id: &str, observed_at: i64) -> std::io::Result<()> {
    record_in(&paths::layout().play_history(), game_id, observed_at)
}

fn recent_ids_in(root: &std::path::Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut records: Vec<(i64, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.len() > 4096 {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(record) = serde_json::from_slice::<PlayRecord>(&bytes) else {
            continue;
        };
        if record.game_id.is_empty() || record.observed_at <= 0 {
            continue;
        }
        records.push((record.observed_at, record.game_id));
    }
    records.sort_by(|left, right| right.0.cmp(&left.0));
    records.into_iter().map(|(_, game_id)| game_id).collect()
}

/// Game IDs ordered by most recent observed play.
pub fn recent_ids() -> Vec<String> {
    recent_ids_in(&paths::layout().play_history())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_games_by_most_recent_play() {
        let temp = tempfile::tempdir().unwrap();
        record_in(temp.path(), "42", 100).unwrap();
        record_in(temp.path(), "43", 300).unwrap();
        record_in(temp.path(), "42", 200).unwrap();
        assert_eq!(recent_ids_in(temp.path()), vec!["43", "42"]);
    }

    #[test]
    fn ignores_invalid_records() {
        let temp = tempfile::tempdir().unwrap();
        cargo_test_write(temp.path().join("bad.json"), br#"{"game_id":"","observed_at":100}"#);
        record_in(temp.path(), "42", 100).unwrap();
        assert_eq!(recent_ids_in(temp.path()), vec!["42"]);
    }

    fn cargo_test_write(path: std::path::PathBuf, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }
}
