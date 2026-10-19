//! Hand classification and the temporal gate for the OpenLess host.
//!
//! This crate does not own dictation, ASR, or the tray. The host feeds
//! [`DictationHotkeyEdge`](https://github.com/Open-Less/openless) press/release
//! edges and shares [`with_io_serial`] with `TextInserter`.

mod apply;
mod classify;
mod frame;
mod gate;
mod lock;
mod pointer;

#[cfg(feature = "camera")]
mod camera;
#[cfg(feature = "mediapipe")]
mod mediapipe;

pub use apply::{apply_decision, ApplyOutcome, DictationEdges, EdgeSession, PointerInject, Screen};
pub use classify::{classify_pose, palm_center, Pose};
pub use frame::{FrameBuffer, HandObservation, Landmark, HAND_LANDMARKS};
pub use gate::{
    Decision, DecisionKind, EventCounts, GateSnapshot, GestureGate, TrayState,
    PROVISIONAL_LONG_FIST_MIN_MS, PROVISIONAL_MISTOUCH_PAUSE_MS, PROVISIONAL_SHORT_FIST_MAX_MS,
};
pub use lock::{io_lock_held, with_io_serial, IoGuard};
pub use pointer::{screen_point, MOVE_DEADZONE};

#[cfg(feature = "camera")]
pub use camera::{CameraCapture, CameraError};
#[cfg(feature = "mediapipe")]
pub use mediapipe::{
    default_hand_model_path, MediaPipeHands, HAND_LANDMARKER_SHA256, HAND_LANDMARKER_URL,
};

/// Minimum handedness / presence confidence treated as a real hand.
pub const MIN_HAND_CONFIDENCE: f32 = 0.5;
