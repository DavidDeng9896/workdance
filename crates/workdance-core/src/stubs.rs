//! Empty hooks for WP2–WP3. WP1 vision lives in `workdance-vision`.
//! Inference and injection must stay on Rust threads — never in a JS main loop.

/// Legacy WP0 placeholder; prefer [`workdance_vision::VisionWorker`].
#[derive(Debug, Default)]
pub struct VisionHandle {
    pub started: bool,
}

impl VisionHandle {
    pub fn start_stub(&mut self) {
        self.started = true;
    }

    pub fn stop_stub(&mut self) {
        self.started = false;
    }
}

/// Placeholder for offline Chinese ASR (WP3).
#[derive(Debug, Default)]
pub struct AsrHandle {
    pub started: bool,
}

impl AsrHandle {
    pub fn start_stub(&mut self) {
        self.started = true;
    }

    pub fn stop_stub(&mut self) {
        self.started = false;
    }
}

/// Serial input injection stub (WP2). Intentionally does not call SendInput / CGEvent.
#[derive(Debug, Default)]
pub struct InjectHandle {
    pub queue_len: usize,
}

impl InjectHandle {
    pub fn enqueue_noop(&mut self) {
        self.queue_len = self.queue_len.saturating_add(1);
    }

    pub fn clear(&mut self) {
        self.queue_len = 0;
    }
}
