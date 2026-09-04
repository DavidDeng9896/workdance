//! WP1 / WP-M1 vision pipeline: camera frames → hand detector → dual-tier scheduler.
//!
//! Inference runs on a dedicated Rust thread. JS never sees raw frames.

mod camera;
mod detector;
mod mirror;
mod worker;

#[cfg(feature = "mediapipe-hands")]
mod mediapipe_hands;

pub use camera::{CameraCapture, CameraError, FrameBuffer};
pub use detector::{
    create_default_detector, create_default_detector_with_status, default_mediapipe_model_path,
    default_model_path, default_ort_model_path, DetectDetail, HandPresenceDetector,
    OrtHandLandmarker, ScriptedStubDetector, StubEvent, StubScript, VisionBackendStatus,
    DETECTOR_BACKEND_NAME,
};
#[cfg(feature = "mediapipe-hands")]
pub use mediapipe_hands::{
    MediaPipeHandLandmarker, HAND_LANDMARKER_SHA256, HAND_LANDMARKER_URL,
};
pub use worker::{VisionEvent, VisionWorker, VisionWorkerConfig, WorkerError};
