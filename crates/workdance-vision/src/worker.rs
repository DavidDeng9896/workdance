use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;
use workdance_core::{DualTierMachine, VisionTier};

use crate::camera::{CameraCapture, FrameBuffer};
use crate::detector::{create_default_detector, HandPresenceDetector, ScriptedStubDetector, StubScript};

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("vision worker already running")]
    AlreadyRunning,
}

#[derive(Debug, Clone)]
pub struct VisionWorkerConfig {
    /// Force stub detector + synthetic frames (CI / no camera).
    pub force_stub: bool,
    /// Optional override script when `force_stub`.
    pub stub_script: Option<StubScript>,
    /// Prefer low-res open when using a real camera (sleep-tier cost).
    pub low_res_camera: bool,
}

impl Default for VisionWorkerConfig {
    fn default() -> Self {
        Self {
            force_stub: std::env::var("WORKDANCE_VISION_STUB")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            stub_script: None,
            low_res_camera: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionEvent {
    TierChanged {
        from: VisionTier,
        to: VisionTier,
    },
}

/// Background capture + detect + dual-tier loop.
pub struct VisionWorker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl VisionWorker {
    pub fn spawn<F>(cfg: VisionWorkerConfig, mut on_event: F) -> Result<Self, WorkerError>
    where
        F: FnMut(VisionEvent) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();

        let handle = thread::Builder::new()
            .name("workdance-vision".into())
            .spawn(move || {
                run_loop(cfg, stop_flag, &mut on_event);
            })
            .expect("spawn vision thread");

        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for VisionWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run_loop<F>(cfg: VisionWorkerConfig, stop: Arc<AtomicBool>, on_event: &mut F)
where
    F: FnMut(VisionEvent),
{
    let mut camera = if cfg.force_stub {
        None
    } else {
        match CameraCapture::open_default(cfg.low_res_camera) {
            Ok(cam) => Some(cam),
            Err(err) => {
                eprintln!("[workdance-vision] camera unavailable ({err}); using stub frames");
                None
            }
        }
    };

    let mut detector: Box<dyn HandPresenceDetector> = if cfg.force_stub || camera.is_none() {
        let script = cfg
            .stub_script
            .clone()
            .unwrap_or_else(StubScript::default);
        Box::new(ScriptedStubDetector::new(script))
    } else {
        create_default_detector()
    };

    eprintln!(
        "[workdance-vision] detector={} camera={}",
        detector.name(),
        if camera.is_some() { "open" } else { "none/stub" }
    );

    let mut machine = DualTierMachine::new();
    let mut last_tier = machine.tier();
    let placeholder = FrameBuffer::placeholder_sleep();

    while !stop.load(Ordering::SeqCst) {
        let tick_start = Instant::now();
        let frame = if let Some(cam) = camera.as_mut() {
            match cam.grab() {
                Ok(f) => f,
                Err(err) => {
                    eprintln!("[workdance-vision] grab failed: {err}");
                    placeholder.clone()
                }
            }
        } else {
            placeholder.clone()
        };

        let obs = detector.process_frame(&frame);
        let tier = machine.observe(Instant::now(), obs);
        if tier != last_tier {
            on_event(VisionEvent::TierChanged {
                from: last_tier,
                to: tier,
            });
            last_tier = tier;
            // When waking, try bumping camera resolution request next open (best-effort).
            let _ = machine.target_fps();
        }

        let period = Duration::from_secs_f32(1.0 / machine.target_fps().max(1.0));
        let elapsed = tick_start.elapsed();
        if elapsed < period {
            thread::sleep(period - elapsed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn stub_worker_emits_wake_and_sleep() {
        let script = StubScript {
            events: vec![
                crate::detector::StubEvent {
                    at_ms: 0,
                    present: false,
                    confidence: 0.0,
                },
                crate::detector::StubEvent {
                    at_ms: 100,
                    present: true,
                    confidence: 0.95,
                },
                crate::detector::StubEvent {
                    at_ms: 900,
                    present: false,
                    confidence: 0.0,
                },
            ],
        };
        let (tx, rx) = mpsc::channel();
        let worker = VisionWorker::spawn(
            VisionWorkerConfig {
                force_stub: true,
                stub_script: Some(script),
                low_res_camera: true,
            },
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .unwrap();

        let mut saw_active = false;
        let mut saw_sleep = false;
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline && !(saw_active && saw_sleep) {
            if let Ok(VisionEvent::TierChanged { to, .. }) = rx.recv_timeout(Duration::from_millis(50))
            {
                match to {
                    VisionTier::Active => saw_active = true,
                    VisionTier::Sleep => {
                        if saw_active {
                            saw_sleep = true;
                        }
                    }
                }
            }
        }
        worker.stop();
        assert!(saw_active, "expected wake to Active from stub palm");
        assert!(saw_sleep, "expected fall back to Sleep after palm leave");
    }
}
