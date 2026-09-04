//! WP4 Markdown memo store — timestamped MD only, never audio files.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("notes path empty")]
    EmptyPath,
    #[error("invalid notes path: {0}")]
    InvalidPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoRecord {
    pub path: PathBuf,
    pub title: String,
    pub body: String,
    /// Filename stem / wall-clock stamp used in the file name.
    pub stamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoHit {
    pub path: PathBuf,
    pub title: String,
    pub snippet: String,
}

/// Expand `~` and ensure the notes directory exists.
pub fn ensure_notes_dir(notes_path: &str) -> Result<PathBuf, MemoError> {
    let path = expand_notes_path(notes_path)?;
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn expand_notes_path(notes_path: &str) -> Result<PathBuf, MemoError> {
    let trimmed = notes_path.trim();
    if trimmed.is_empty() {
        return Err(MemoError::EmptyPath);
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| {
            MemoError::InvalidPath("cannot resolve home for ~/…".into())
        })?;
        return Ok(home.join(rest));
    }
    if trimmed == "~" {
        return dirs::home_dir().ok_or_else(|| MemoError::InvalidPath("~".into()));
    }
    Ok(PathBuf::from(trimmed))
}

/// Write `YYYY-MM-DD-HHMMSS.md` under `notes_dir` with YAML frontmatter + body.
/// Never writes wav/mp3/audio. `stamp` is `YYYY-MM-DD-HHMMSS` (caller supplies for tests).
pub fn write_memo(
    notes_path: &str,
    stamp: &str,
    transcript: &str,
) -> Result<MemoRecord, MemoError> {
    let dir = ensure_notes_dir(notes_path)?;
    let file_name = format!("{stamp}.md");
    let path = dir.join(&file_name);
    let title = stamp_to_title(stamp);
    let body = transcript.trim();
    let content = format!(
        "---\ntitle: \"{title}\"\ncreated: \"{stamp}\"\nkind: memo\n---\n\n# {title}\n\n{body}\n"
    );
    fs::write(&path, content)?;
    Ok(MemoRecord {
        path,
        title,
        body: body.to_string(),
        stamp: stamp.to_string(),
    })
}

fn stamp_to_title(stamp: &str) -> String {
    // 2026-09-04-081500 → 2026-09-04 08:15:00 when well-formed
    if stamp.len() == 17 && stamp.as_bytes().get(10) == Some(&b'-') {
        let date = &stamp[0..10];
        let h = &stamp[11..13];
        let m = &stamp[13..15];
        let s = &stamp[15..17];
        return format!("{date} {h}:{m}:{s}");
    }
    stamp.to_string()
}

/// Local wall-clock stamp `YYYY-MM-DD-HHMMSS`.
pub fn now_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Prefer chrono-less formatting via `time` crate? Keep std-only: use libc localtime via format from UTC offset approximate.
    // For portability without extra deps, use UTC stamp labeled as such.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_utc_stamp(secs)
}

fn format_utc_stamp(unix_secs: u64) -> String {
    // Civil UTC from Unix seconds (no leap seconds) — good enough for memo ids.
    let z = unix_secs;
    let days = z / 86400;
    let tod = z % 86400;
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    let sec = tod % 60;
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}-{hour:02}{min:02}{sec:02}")
}

/// Howard Hinnant civil_from_days (UTC).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

/// Substring search over memo title + body (case-sensitive for CJK; ASCII lowercased).
pub fn search_memos(notes_path: &str, query: &str) -> Result<Vec<MemoHit>, MemoError> {
    let dir = expand_notes_path(notes_path)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let q = normalize_query(query);
    if q.is_empty() {
        return list_all_memos(&dir);
    }
    let mut hits = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        let title = extract_title(&raw).unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let hay = normalize_query(&format!("{title}\n{raw}"));
        if hay.contains(&q) {
            hits.push(MemoHit {
                path,
                title,
                snippet: snippet_around(&raw, query),
            });
        }
    }
    hits.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(hits)
}

fn list_all_memos(dir: &Path) -> Result<Vec<MemoHit>, MemoError> {
    let mut hits = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let title = extract_title(&raw).unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        hits.push(MemoHit {
            path,
            title,
            snippet: raw.chars().take(120).collect(),
        });
    }
    hits.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(hits)
}

fn normalize_query(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn extract_title(raw: &str) -> Option<String> {
    for line in raw.lines().take(12) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("title:") {
            let t = rest.trim().trim_matches('"').trim_matches('\'').trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
        if let Some(rest) = line.strip_prefix("# ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn snippet_around(raw: &str, query: &str) -> String {
    let lower_raw = normalize_query(raw);
    let q = normalize_query(query);
    if let Some(idx) = lower_raw.find(&q) {
        let start = idx.saturating_sub(20);
        let end = (idx + q.len() + 40).min(raw.len());
        return raw.chars().skip(start).take(end.saturating_sub(start)).collect();
    }
    raw.chars().take(80).collect()
}

/// True if any audio-like file exists under notes dir (for tests / policy checks).
pub fn notes_dir_has_audio(notes_path: &str) -> Result<bool, MemoError> {
    let dir = expand_notes_path(notes_path)?;
    if !dir.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "wav" | "mp3" | "ogg" | "flac" | "m4a" | "webm") {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_memo_creates_timestamped_md() {
        let dir = tempdir().unwrap();
        let notes = dir.path().to_string_lossy().to_string();
        let rec = write_memo(&notes, "2026-09-04-081500", "实验记录已追加。").unwrap();
        assert!(rec.path.ends_with("2026-09-04-081500.md"));
        assert!(rec.path.is_file());
        let raw = fs::read_to_string(&rec.path).unwrap();
        assert!(raw.contains("title:"));
        assert!(raw.contains("实验记录已追加。"));
        assert!(raw.contains("# 2026-09-04 08:15:00"));
        assert!(!notes_dir_has_audio(&notes).unwrap());
    }

    #[test]
    fn write_memo_never_creates_audio_files() {
        let dir = tempdir().unwrap();
        let notes = dir.path().to_string_lossy().to_string();
        let _ = write_memo(&notes, "2026-09-04-120000", "hello").unwrap();
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].extension().and_then(|e| e.to_str()),
            Some("md")
        );
        assert!(!notes_dir_has_audio(&notes).unwrap());
    }

    #[test]
    fn search_finds_memo_by_substring() {
        let dir = tempdir().unwrap();
        let notes = dir.path().to_string_lossy().to_string();
        write_memo(&notes, "2026-09-04-090000", "今日实验进度正常").unwrap();
        write_memo(&notes, "2026-09-04-100000", " unrelated ").unwrap();
        let hits = search_memos(&notes, "实验").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].title.contains("09:00:00") || hits[0].snippet.contains("实验"));
    }

    #[test]
    fn missing_notes_path_creates_dir() {
        let dir = tempdir().unwrap();
        let notes = dir.path().join("nested").join("memos");
        assert!(!notes.exists());
        let path_str = notes.to_string_lossy().to_string();
        let created = ensure_notes_dir(&path_str).unwrap();
        assert!(created.is_dir());
        let rec = write_memo(&path_str, "2026-01-01-000000", "x").unwrap();
        assert!(rec.path.is_file());
    }

    #[test]
    fn empty_path_is_clear_error() {
        let err = ensure_notes_dir("   ").unwrap_err();
        assert!(matches!(err, MemoError::EmptyPath));
    }
}
