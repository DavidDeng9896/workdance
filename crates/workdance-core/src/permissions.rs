use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Camera,
    Microphone,
    /// macOS Accessibility / Input Monitoring; Windows input-simulation note.
    AccessibilityOrInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsSnapshot {
    pub camera: PermissionStatus,
    pub microphone: PermissionStatus,
    pub accessibility_or_input: PermissionStatus,
    pub platform: String,
    /// Human-readable note; never claims a fake grant.
    pub notes: Vec<String>,
}

/// Probe OS permission state without requesting or faking grants.
///
/// WP0: returns `Unknown` for camera/mic (no capture yet) and platform-specific
/// notes for accessibility / input simulation. Later WPs can call real APIs.
pub fn probe_permissions() -> PermissionsSnapshot {
    let platform = std::env::consts::OS.to_string();
    let mut notes = Vec::new();

    let accessibility_or_input = match platform.as_str() {
        "macos" => {
            notes.push(
                "macOS：请在「系统设置 → 隐私与安全性 → 辅助功能 / 输入监控」中允许 WorkDance。"
                    .into(),
            );
            PermissionStatus::Unknown
        }
        "windows" => {
            notes.push(
                "Windows：后续注入使用 SendInput；WP0 不模拟输入。部分环境可能提示 UAC / 完整性级别。"
                    .into(),
            );
            PermissionStatus::Unknown
        }
        _ => {
            notes.push(
                "当前平台用于 CI / 开发：摄像头与输入注入不在此探测；状态保持未知。".into(),
            );
            PermissionStatus::Unknown
        }
    };

    notes.push("摄像头 / 麦克风：WP0 未打开设备，状态为未知（不伪造已允许）。".into());

    PermissionsSnapshot {
        camera: PermissionStatus::Unknown,
        microphone: PermissionStatus::Unknown,
        accessibility_or_input,
        platform,
        notes,
    }
}

impl PermissionKind {
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Camera => "摄像头",
            Self::Microphone => "麦克风",
            Self::AccessibilityOrInput => "辅助功能 / 输入模拟",
        }
    }
}

impl PermissionStatus {
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Granted => "已允许",
            Self::Missing => "未授权",
            Self::Unknown => "未知",
        }
    }
}
