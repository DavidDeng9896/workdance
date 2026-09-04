use serde::{Deserialize, Serialize};

use crate::VisionTier;

/// Visible tray / HUD modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppMode {
    /// Sleep / idle — default. Low-rate palm watch (WP1).
    Sleep,
    /// Gesture active — virtual cursor later (WP2).
    GestureActive,
    /// Recording — G07 long-hold dictation (WP3).
    Recording,
}

impl AppMode {
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Sleep => "休眠",
            Self::GestureActive => "手势开",
            Self::Recording => "录音",
        }
    }

    pub fn tray_title_zh(self) -> String {
        match self {
            Self::Sleep => "WorkDance · 休眠".into(),
            Self::GestureActive => "WorkDance · 手势开".into(),
            Self::Recording => "WorkDance · 录音".into(),
        }
    }

    /// Tray tooltip when voice-only fallback is active.
    pub fn tray_title_voice_only(self) -> String {
        match self {
            Self::Sleep | Self::GestureActive => "WorkDance · 仅语音".into(),
            Self::Recording => "WorkDance · 听写中".into(),
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Sleep => Self::GestureActive,
            Self::GestureActive => Self::Recording,
            Self::Recording => Self::Sleep,
        }
    }

    pub fn from_vision_tier(tier: VisionTier) -> Self {
        match tier {
            VisionTier::Sleep => Self::Sleep,
            VisionTier::Active => Self::GestureActive,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeState {
    pub mode: AppMode,
    pub recording_seconds: u32,
    /// When true, tray ignores vision tier updates (debug manual override).
    pub manual_override: bool,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            mode: AppMode::Sleep,
            recording_seconds: 0,
            manual_override: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_three_states() {
        assert_eq!(AppMode::Sleep.cycle(), AppMode::GestureActive);
        assert_eq!(AppMode::GestureActive.cycle(), AppMode::Recording);
        assert_eq!(AppMode::Recording.cycle(), AppMode::Sleep);
    }

    #[test]
    fn vision_tier_maps_to_tray_modes() {
        assert_eq!(
            AppMode::from_vision_tier(VisionTier::Sleep),
            AppMode::Sleep
        );
        assert_eq!(
            AppMode::from_vision_tier(VisionTier::Active),
            AppMode::GestureActive
        );
    }

    #[test]
    fn voice_only_tray_titles() {
        assert!(AppMode::Sleep.tray_title_voice_only().contains("仅语音"));
        assert!(AppMode::Recording.tray_title_voice_only().contains("听写"));
    }
}
