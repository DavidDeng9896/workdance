//! WP2 gesture classify + serial OS inject (path A).

mod engine;
mod inject;
mod smoother;

pub use engine::{GestureEngine, HandSample, InjectCommand, SHORT_FIST_MAX};
pub use inject::{
    create_default_injector, InjectBackend, InjectError, InjectQueue, NullInjector, RecordingInjector,
};
pub use smoother::ExpSmoother;
