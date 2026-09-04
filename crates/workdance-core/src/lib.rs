//! WorkDance core: persisted config, tray app modes, permission placeholders,
//! memo store (WP4), and hooks for vision / ASR / inject (WP1+).

mod config;
mod memo;
mod permissions;
mod state;
mod stubs;
mod tier;

pub use config::{
    config_path, load_config, load_config_from, save_config, save_config_to, AppConfig,
    CalibrationProfile, ConfigError,
};
pub use memo::{
    ensure_notes_dir, expand_notes_path, notes_dir_has_audio, now_stamp, search_memos, write_memo,
    MemoError, MemoHit, MemoRecord,
};
pub use permissions::{PermissionKind, PermissionStatus, PermissionsSnapshot, probe_permissions};
pub use state::{AppMode, RuntimeState};
pub use stubs::{AsrHandle, InjectHandle, VisionHandle};
pub use tier::{
    DualTierMachine, HandFrame, HandLandmark, PalmObservation, VisionTier, ACTIVE_FPS,
    MIN_PALM_CONFIDENCE, SLEEP_FPS, SLEEP_HOLD, WAKE_HOLD,
};
