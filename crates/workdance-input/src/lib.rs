//! WP2–WP3 / WP-M2: gesture classify, landmark→G02–G05, focus, G07, serial OS inject.

mod asr;
mod engine;
mod focus;
mod g07;
mod inject;
mod landmarks;
mod smoother;

pub use asr::{
    asr_model_files_present, asr_stub_allowed, create_asr_with_options, create_default_asr,
    default_asr_model_dir, default_whisper_model_path, pcm_i16le_mono_to_f32, AsrBackend,
    AsrBackendStatus, AsrFactoryOptions, AsrResult, StubAsr, UnavailableAsr, ASR_MAX_PCM_BYTES,
    ASR_MAX_SECONDS, ASR_SAMPLE_RATE_HZ, STUB_ASR_TEXT,
};

#[cfg(feature = "sherpa-asr")]
pub use asr::SherpaAsr;
pub use engine::{
    EngineTick, GestureEngine, HandSample, InjectCommand, MemoEvent, G08_WINDOW_MS, SHORT_FIST_MAX,
};
pub use landmarks::{hand_frame_to_sample, sample_from_landmarks};
pub use focus::{
    create_default_focus_probe, FocusLock, FocusProbe, FocusTarget, StubFocusProbe, FOCUS_DWELL_MS,
};
pub use g07::{G07Event, G07Machine, G07Phase, G07_ARM_MS, G07_MAX_MS};
pub use inject::{
    create_default_injector, InjectBackend, InjectError, InjectQueue, NullInjector,
    RecordingInjector,
};
pub use smoother::ExpSmoother;
