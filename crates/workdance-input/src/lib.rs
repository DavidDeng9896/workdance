//! WP2–WP3: gesture classify, focus lock, G07 offline dictation, serial OS inject.

mod asr;
mod engine;
mod focus;
mod g07;
mod inject;
mod smoother;

pub use asr::{
    create_default_asr, default_whisper_model_path, AsrBackend, AsrResult, StubAsr,
};
pub use engine::{EngineTick, GestureEngine, HandSample, InjectCommand, SHORT_FIST_MAX};
pub use focus::{
    create_default_focus_probe, FocusLock, FocusProbe, FocusTarget, StubFocusProbe, FOCUS_DWELL_MS,
};
pub use g07::{G07Event, G07Machine, G07Phase, G07_ARM_MS, G07_MAX_MS};
pub use inject::{
    create_default_injector, InjectBackend, InjectError, InjectQueue, NullInjector,
    RecordingInjector,
};
pub use smoother::ExpSmoother;
