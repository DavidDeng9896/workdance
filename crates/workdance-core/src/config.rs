use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml deserialize: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml serialize: {0}")]
    TomlSer(#[from] toml::ser::Error),
}

/// Local WP0 settings. Fields that affect sensing later are stored now but unused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Master gesture enable (tray / settings).
    pub gesture_enabled: bool,
    /// Voice-only fallback: gestures off, dictation path remains.
    pub voice_only: bool,
    /// Gesture sensitivity 0.0–1.0 (persist only in WP0).
    pub sensitivity: f32,
    /// Dead-zone radius 0.0–1.0 for calibration (persist only in WP0).
    pub dead_zone: f32,
    /// Directory for MD memo drafts (WP4 G08).
    pub notes_path: String,
    /// Launch at login stub (not wired to OS autostart in WP0).
    pub launch_at_startup: bool,
    /// Camera mode label: "performance" | "hd" (stub).
    pub camera_mode: String,
    /// ASR model size label: "tiny" | "base" (stub until WP3).
    pub asr_model: String,
    /// Four-point calibration stub profile.
    pub calibration: CalibrationProfile,
    /// First-run permissions wizard completed (or dismissed).
    pub first_run_done: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            gesture_enabled: true,
            voice_only: false,
            sensitivity: 0.7,
            dead_zone: 0.15,
            notes_path: default_notes_path(),
            launch_at_startup: true,
            camera_mode: "performance".into(),
            asr_model: "tiny".into(),
            calibration: CalibrationProfile::default(),
            first_run_done: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrationProfile {
    /// True after user confirms the skeleton calibration flow.
    pub confirmed: bool,
    /// Placeholder corner samples (screen-normalized); empty until WP1.
    pub corners: Vec<[f32; 2]>,
}

impl Default for CalibrationProfile {
    fn default() -> Self {
        Self {
            confirmed: false,
            corners: Vec::new(),
        }
    }
}

fn default_notes_path() -> String {
    dirs::document_dir()
        .map(|p| p.join("WorkDance").to_string_lossy().into_owned())
        .unwrap_or_else(|| "~/Documents/WorkDance".into())
}

/// Config file under the platform config directory.
pub fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("workdance").join("config.toml")
}

pub fn load_config() -> Result<AppConfig, ConfigError> {
    load_config_from(&config_path())
}

pub fn load_config_from(path: &Path) -> Result<AppConfig, ConfigError> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = fs::read_to_string(path)?;
    let cfg: AppConfig = toml::from_str(&raw)?;
    Ok(cfg)
}

pub fn save_config(cfg: &AppConfig) -> Result<PathBuf, ConfigError> {
    save_config_to(&config_path(), cfg)
}

pub fn save_config_to(path: &Path, cfg: &AppConfig) -> Result<PathBuf, ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(cfg)?;
    fs::write(path, raw)?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_config() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = AppConfig::default();
        cfg.gesture_enabled = false;
        cfg.voice_only = true;
        cfg.sensitivity = 0.42;
        cfg.dead_zone = 0.2;
        cfg.notes_path = "/tmp/wd-notes".into();
        cfg.calibration.confirmed = true;
        cfg.calibration.corners = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        save_config_to(&path, &cfg).unwrap();
        let loaded = load_config_from(&path).unwrap();
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        let cfg = load_config_from(&path).unwrap();
        assert!(cfg.gesture_enabled);
        assert!(!cfg.first_run_done);
    }
}
