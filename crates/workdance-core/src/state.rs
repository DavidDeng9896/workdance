use serde::{Deserialize, Serialize};

/// Visible tray / HUD modes for WP0 stubs (manual cycling for demo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppMode {
    /// Sleep / idle — default. Low-rate palm watch later (WP1).
    Sleep,
    /// Gesture active — virtual cursor later (WP2).
    GestureActive,
    /// Recording — G07 long-hold later (WP3).
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

    pub fn cycle(self) -> Self {
        match self {
            Self::Sleep => Self::GestureActive,
            Self::GestureActive => Self::Recording,
            Self::Recording => Self::Sleep,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeState {
    pub mode: AppMode,
    pub recording_seconds: u32,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            mode: AppMode::Sleep,
            recording_seconds: 0,
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
}
