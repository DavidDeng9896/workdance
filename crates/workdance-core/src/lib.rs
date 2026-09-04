//! WorkDance core: persisted config, tray app modes, permission placeholders,
//! and empty hooks for later vision / ASR / input-injection threads (WP1+).

mod config;
mod permissions;
mod state;
mod stubs;

pub use config::{
    config_path, load_config, load_config_from, save_config, save_config_to, AppConfig,
    CalibrationProfile, ConfigError,
};
pub use permissions::{PermissionKind, PermissionStatus, PermissionsSnapshot, probe_permissions};
pub use state::{AppMode, RuntimeState};
pub use stubs::{AsrHandle, InjectHandle, VisionHandle};
