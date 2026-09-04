//! Empty hooks for WP1–WP3. Inference and injection must stay on Rust threads —
//! never in a JS main loop.

/// Placeholder for the MediaPipe Hands / vision worker (WP1).
#[derive(Debug, Default)]
pub struct VisionHandle {
    pub started: bool,
}

impl VisionHandle {
    pub fn start_stub(&mut self) {
        // WP1: spawn capture + MediaPipe thread. No-op in WP0.
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
        // WP3: load local model, record on G07. No-op in WP0.
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
        // WP2: serialize click/move/scroll. Must remain empty in WP0.
        self.queue_len = self.queue_len.saturating_add(1);
    }

    pub fn clear(&mut self) {
        self.queue_len = 0;
    }
}
