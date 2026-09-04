//! WP1 vision pipeline: camera frames → hand-presence detector → dual-tier scheduler.
//!
//! Inference runs on a dedicated Rust thread. JS never sees raw frames.

mod camera;
mod detector;
mod worker;

pub use camera::{CameraCapture, CameraError, FrameBuffer};
pub use detector::{
    create_default_detector, HandPresenceDetector, OrtHandLandmarker, ScriptedStubDetector,
    StubScript, DETECTOR_BACKEND_NAME,
};
pub use worker::{VisionEvent, VisionWorker, VisionWorkerConfig, WorkerError};
