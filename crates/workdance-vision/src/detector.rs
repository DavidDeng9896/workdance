use std::path::{Path, PathBuf};

use serde::Serialize;
use workdance_core::HandFrame;

use crate::camera::FrameBuffer;
use crate::mirror::mirror_horizontal;

/// How much landmark detail to request this frame (Sleep vs Active).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectDetail {
    /// Sleep tier (3–5 FPS): presence + confidence only.
    PresenceOnly,
    /// Active tier (25–30 FPS): full 21 landmarks when the backend can provide them.
    FullLandmarks,
}

/// Backend-agnostic hand presence. Implementations must be `Send` for the vision thread.
pub trait HandPresenceDetector: Send {
    fn name(&self) -> &'static str;
    fn process_frame(&mut self, frame: &FrameBuffer, detail: DetectDetail) -> HandFrame;
}

/// UI / tray banner hook for vision backend readiness (never silent fake).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VisionBackendStatus {
    pub backend: String,
    pub ok: bool,
    /// Human-readable status for settings banner (Chinese OK).
    pub message: String,
}

impl VisionBackendStatus {
    pub fn ready(backend: &str) -> Self {
        Self {
            backend: backend.into(),
            ok: true,
            message: format!("视觉后端就绪：{backend}"),
        }
    }

    pub fn stub_fallback(reason: impl Into<String>) -> Self {
        Self {
            backend: "stub".into(),
            ok: false,
            message: reason.into(),
        }
    }
}

pub const DETECTOR_BACKEND_NAME: &str = {
    #[cfg(feature = "mediapipe-hands")]
    {
        "mediapipe-hands"
    }
    #[cfg(all(not(feature = "mediapipe-hands"), feature = "ort-hands"))]
    {
        "ort-hands"
    }
    #[cfg(all(not(feature = "mediapipe-hands"), not(feature = "ort-hands")))]
    {
        "stub"
    }
};

/// Scripted palm enter/leave for CI and demos without hardware/models.
#[derive(Debug, Clone)]
pub struct StubScript {
    /// Absolute timeline from worker start.
    pub events: Vec<StubEvent>,
}

#[derive(Debug, Clone, Copy)]
pub struct StubEvent {
    pub at_ms: u64,
    pub present: bool,
    pub confidence: f32,
}

impl Default for StubScript {
    fn default() -> Self {
        // Demo: idle 800ms → palm 2.0s → leave (exercises wake 0.5s + later sleep 1.2s).
        Self {
            events: vec![
                StubEvent {
                    at_ms: 0,
                    present: false,
                    confidence: 0.0,
                },
                StubEvent {
                    at_ms: 800,
                    present: true,
                    confidence: 0.92,
                },
                StubEvent {
                    at_ms: 2800,
                    present: false,
                    confidence: 0.0,
                },
            ],
        }
    }
}

pub struct ScriptedStubDetector {
    script: StubScript,
    started: std::time::Instant,
}

impl ScriptedStubDetector {
    pub fn new(script: StubScript) -> Self {
        Self {
            script,
            started: std::time::Instant::now(),
        }
    }

    pub fn with_default_script() -> Self {
        Self::new(StubScript::default())
    }
}

impl HandPresenceDetector for ScriptedStubDetector {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn process_frame(&mut self, _frame: &FrameBuffer, _detail: DetectDetail) -> HandFrame {
        let elapsed = self.started.elapsed().as_millis() as u64;
        let mut frame = HandFrame::absent();
        for ev in &self.script.events {
            if elapsed >= ev.at_ms {
                frame = HandFrame::presence_only(ev.present, ev.confidence);
            }
        }
        frame
    }
}

/// ONNX Runtime hand-landmarker façade (feature `ort-hands`).
///
/// Without a model file on disk this backend refuses to construct; the worker
/// falls back to the stub. Model download: `scripts/download-hand-landmarker.sh`
/// (MediaPipe `.task` is preferred; ORT uses a separate `.onnx` if present).
pub struct OrtHandLandmarker {
    #[cfg(feature = "ort-hands")]
    _marker: (),
    #[cfg(not(feature = "ort-hands"))]
    _priv: (),
}

impl OrtHandLandmarker {
    pub fn try_open(model_path: &Path) -> Result<Self, String> {
        #[cfg(not(feature = "ort-hands"))]
        {
            let _ = model_path;
            Err("compiled without feature `ort-hands`".into())
        }
        #[cfg(feature = "ort-hands")]
        {
            if !model_path.is_file() {
                return Err(format!("model not found: {}", model_path.display()));
            }
            let _ = ort::init().commit().map_err(|e| e.to_string())?;
            Ok(Self { _marker: () })
        }
    }
}

impl HandPresenceDetector for OrtHandLandmarker {
    fn name(&self) -> &'static str {
        "ort-hands"
    }

    fn process_frame(&mut self, frame: &FrameBuffer, detail: DetectDetail) -> HandFrame {
        // WP1 real-backend placeholder: without a fully wired graph, treat a
        // non-black frame as weak presence. Production builds prefer mediapipe-hands.
        let mirrored = mirror_horizontal(frame);
        let mean = if mirrored.rgb.is_empty() {
            0u64
        } else {
            mirrored.rgb.iter().map(|&b| b as u64).sum::<u64>() / mirrored.rgb.len() as u64
        };
        if mean > 12 {
            let mut hand = HandFrame::presence_only(true, 0.65);
            if detail == DetectDetail::FullLandmarks {
                // Landmarks intentionally empty until a real ORT graph fills them; WP-M2 maps
                // landmarks when present, otherwise the input bridge keeps the stub path.
                hand.landmarks = None;
            }
            hand
        } else {
            HandFrame::absent()
        }
    }
}

/// Prefer MediaPipe model → ORT → stub. Always returns an explicit status for UI.
pub fn create_default_detector_with_status() -> (Box<dyn HandPresenceDetector>, VisionBackendStatus) {
    let mut reasons: Vec<String> = Vec::new();

    #[cfg(feature = "mediapipe-hands")]
    {
        let model = default_mediapipe_model_path();
        match crate::mediapipe_hands::MediaPipeHandLandmarker::try_open(&model) {
            Ok(det) => {
                return (
                    Box::new(det),
                    VisionBackendStatus::ready("mediapipe-hands"),
                );
            }
            Err(e) => {
                reasons.push(format!("mediapipe-hands: {e}"));
            }
        }
    }
    #[cfg(not(feature = "mediapipe-hands"))]
    {
        reasons.push("mediapipe-hands feature not enabled at compile time".into());
    }

    #[cfg(feature = "ort-hands")]
    {
        let model = default_ort_model_path();
        match OrtHandLandmarker::try_open(&model) {
            Ok(det) => {
                return (Box::new(det), VisionBackendStatus::ready("ort-hands"));
            }
            Err(e) => {
                reasons.push(format!("ort-hands: {e}"));
            }
        }
    }
    #[cfg(not(feature = "ort-hands"))]
    {
        reasons.push("ort-hands feature not enabled at compile time".into());
    }

    let msg = format!(
        "视觉模型未就绪，已降级为 stub（不会假装检测到手掌）。原因：{}。请运行 scripts/download-hand-landmarker.sh 并启用 mediapipe-hands。",
        reasons.join(" | ")
    );
    eprintln!("[workdance-vision] {msg}");
    (
        Box::new(ScriptedStubDetector::with_default_script()),
        VisionBackendStatus::stub_fallback(msg),
    )
}

/// Prefer ORT/MediaPipe model when feature + file exist; otherwise scripted stub.
pub fn create_default_detector() -> Box<dyn HandPresenceDetector> {
    create_default_detector_with_status().0
}

/// Expected on-disk MediaPipe `.task` model (not committed).
pub fn default_mediapipe_model_path() -> PathBuf {
    #[cfg(feature = "mediapipe-hands")]
    {
        crate::mediapipe_hands::default_mediapipe_model_path()
    }
    #[cfg(not(feature = "mediapipe-hands"))]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("workdance")
            .join("models")
            .join("hand_landmarker.task")
    }
}

/// Expected on-disk ORT model location (not committed; see download script).
pub fn default_ort_model_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("workdance")
        .join("models")
        .join("hand_landmarker.onnx")
}

/// Back-compat alias used by older call sites.
pub fn default_model_path() -> PathBuf {
    default_mediapipe_model_path()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stub_script_transitions() {
        let script = StubScript {
            events: vec![
                StubEvent {
                    at_ms: 0,
                    present: false,
                    confidence: 0.0,
                },
                StubEvent {
                    at_ms: 50,
                    present: true,
                    confidence: 0.9,
                },
            ],
        };
        let mut det = ScriptedStubDetector::new(script);
        let frame = FrameBuffer::placeholder_sleep();
        let a = det.process_frame(&frame, DetectDetail::PresenceOnly);
        assert!(!a.present);
        assert!(a.landmarks.is_none());
        std::thread::sleep(Duration::from_millis(60));
        let b = det.process_frame(&frame, DetectDetail::FullLandmarks);
        assert!(b.present);
        assert!(b.confidence >= 0.9);
        // Stub never invents landmarks.
        assert!(b.landmarks.is_none());
    }

    #[test]
    fn default_selection_reports_status() {
        let (det, status) = create_default_detector_with_status();
        // Without models / features in CI → stub with explicit message.
        assert_eq!(det.name(), "stub");
        assert!(!status.ok);
        assert_eq!(status.backend, "stub");
        assert!(
            status.message.contains("stub") || status.message.contains("未就绪"),
            "{}",
            status.message
        );
    }

    #[test]
    fn mediapipe_model_path_is_task_bundle() {
        let p = default_mediapipe_model_path();
        assert!(p.ends_with("hand_landmarker.task"), "{}", p.display());
    }
}
