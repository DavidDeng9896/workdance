use workdance_core::PalmObservation;

use crate::camera::FrameBuffer;

/// Backend-agnostic hand presence. Implementations must be `Send` for the vision thread.
pub trait HandPresenceDetector: Send {
    fn name(&self) -> &'static str;
    fn process_frame(&mut self, frame: &FrameBuffer) -> PalmObservation;
}

pub const DETECTOR_BACKEND_NAME: &str = {
    #[cfg(feature = "ort-hands")]
    {
        "ort-hands"
    }
    #[cfg(not(feature = "ort-hands"))]
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

    fn process_frame(&mut self, _frame: &FrameBuffer) -> PalmObservation {
        let elapsed = self.started.elapsed().as_millis() as u64;
        let mut obs = PalmObservation {
            present: false,
            confidence: 0.0,
        };
        for ev in &self.script.events {
            if elapsed >= ev.at_ms {
                obs = PalmObservation {
                    present: ev.present,
                    confidence: ev.confidence,
                };
            }
        }
        obs
    }
}

/// ONNX Runtime hand-landmarker façade (feature `ort-hands`).
///
/// Without a model file on disk this backend refuses to construct; the worker
/// falls back to the stub. Model download: `scripts/download-hand-landmarker.sh`.
pub struct OrtHandLandmarker {
    #[cfg(feature = "ort-hands")]
    _marker: (),
    #[cfg(not(feature = "ort-hands"))]
    _priv: (),
}

impl OrtHandLandmarker {
    pub fn try_open(model_path: &std::path::Path) -> Result<Self, String> {
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
            // Session wiring is intentionally thin in WP1: presence uses a
            // confidence head when the graph is available; until a checked-in
            // tiny fixture exists, construction succeeds only if the file is
            // present and we still return conservative observations.
            let _ = ort::init().commit().map_err(|e| e.to_string())?;
            Ok(Self { _marker: () })
        }
    }
}

impl HandPresenceDetector for OrtHandLandmarker {
    fn name(&self) -> &'static str {
        "ort-hands"
    }

    fn process_frame(&mut self, frame: &FrameBuffer) -> PalmObservation {
        // WP1 real-backend placeholder: without a fully wired graph, treat a
        // non-black frame as weak presence so camera smoke tests can proceed.
        // Production Win/Mac builds should swap in a downloaded landmarker model
        // (see README) — this path never leaves the Rust worker thread.
        let mean = if frame.rgb.is_empty() {
            0u64
        } else {
            frame.rgb.iter().map(|&b| b as u64).sum::<u64>() / frame.rgb.len() as u64
        };
        if mean > 12 {
            PalmObservation {
                present: true,
                confidence: 0.65,
            }
        } else {
            PalmObservation {
                present: false,
                confidence: 0.0,
            }
        }
    }
}

/// Prefer ORT model when feature + file exist; otherwise scripted stub.
pub fn create_default_detector() -> Box<dyn HandPresenceDetector> {
    let model = default_model_path();
    #[cfg(feature = "ort-hands")]
    {
        if let Ok(det) = OrtHandLandmarker::try_open(&model) {
            return Box::new(det);
        }
        eprintln!(
            "[workdance-vision] ort-hands model missing at {}; using stub",
            model.display()
        );
    }
    #[cfg(not(feature = "ort-hands"))]
    {
        let _ = model;
    }
    Box::new(ScriptedStubDetector::with_default_script())
}

/// Expected on-disk model location for `ort-hands` (not committed; see download script).
pub fn default_model_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("workdance")
        .join("models")
        .join("hand_landmarker.onnx")
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
        let a = det.process_frame(&frame);
        assert!(!a.present);
        std::thread::sleep(Duration::from_millis(60));
        let b = det.process_frame(&frame);
        assert!(b.present);
        assert!(b.confidence >= 0.9);
    }
}
