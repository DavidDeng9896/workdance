//! Bridges vision tier → gesture engine → serial inject queue.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use workdance_core::{load_config, VisionTier};
use workdance_input::{
    create_default_injector, GestureEngine, HandSample, InjectQueue,
};

/// Shared tier flag updated from vision_bridge.
pub struct InputBridge {
    tier: Arc<Mutex<VisionTier>>,
    stop: Arc<AtomicBool>,
    _queue_keep: Mutex<Option<InjectQueue>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl InputBridge {
    pub fn start() -> Self {
        let cfg = load_config().unwrap_or_default();
        let tier = Arc::new(Mutex::new(VisionTier::Sleep));
        let stop = Arc::new(AtomicBool::new(false));
        let queue = InjectQueue::spawn(create_default_injector());

        let force_stub = std::env::var("WORKDANCE_VISION_STUB")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
            || std::env::var("WORKDANCE_INPUT_STUB")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

        let tier_feed = tier.clone();
        let stop_flag = stop.clone();
        // Move queue into thread via channel-less ownership: recreate sender side —
        // InjectQueue isn't Clone; run engine loop with owned queue.
        let handle = thread::Builder::new()
            .name("workdance-gesture".into())
            .spawn(move || {
                gesture_loop(cfg.sensitivity, cfg.dead_zone, force_stub, tier_feed, stop_flag, queue);
            })
            .expect("spawn gesture thread");

        Self {
            tier,
            stop,
            _queue_keep: Mutex::new(None),
            handle: Mutex::new(Some(handle)),
        }
    }

    pub fn set_tier(&self, tier: VisionTier) {
        *self.tier.lock() = tier;
    }
}

impl Drop for InputBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.lock().take() {
            let _ = h.join();
        }
    }
}

fn gesture_loop(
    sensitivity: f32,
    dead_zone: f32,
    force_stub: bool,
    tier: Arc<Mutex<VisionTier>>,
    stop: Arc<AtomicBool>,
    queue: InjectQueue,
) {
    let mut engine = GestureEngine::new(1920, 1080, sensitivity, dead_zone);
    let start = Instant::now();
    let mut last_tier = VisionTier::Sleep;
    eprintln!(
        "[workdance-input] gesture loop stub_samples={}",
        force_stub
    );

    while !stop.load(Ordering::SeqCst) {
        let t = *tier.lock();
        if t != last_tier {
            engine.set_tier(t);
            last_tier = t;
        }

        if t == VisionTier::Active && force_stub {
            let ms = start.elapsed().as_millis() as u64;
            // Looping stub choreography for demo without landmarks.
            let sample = stub_sample(ms % 6000);
            let cmds = engine.on_sample(ms, sample);
            let _ = queue.enqueue_all(cmds);
            thread::sleep(Duration::from_millis(32)); // ~30 FPS active
        } else {
            // Sleep tier or no stub: do not invent motion (开局无误触).
            thread::sleep(Duration::from_millis(50));
        }
    }
    queue.stop();
}

/// Deterministic open-palm / fist / swipe script for CI & stub demos.
fn stub_sample(ms: u64) -> HandSample {
    // 0–800: open palm drift (G02)
    // 800–1000: short fist → click (G03)
    // 1000–1400: open again
    // 1400–2000: long fist hold
    // 2000–2600: fist scroll down (G04)
    // 2600–3200: open at top
    // 3200–3600: swipe down (G05)
    // 3600–6000: idle open
    if ms < 800 {
        let t = ms as f32 / 800.0;
        HandSample::open_palm(0.35 + t * 0.25, 0.45)
    } else if ms < 1000 {
        HandSample::fist(0.55, 0.45)
    } else if ms < 1400 {
        HandSample::open_palm(0.55, 0.45)
    } else if ms < 2000 {
        HandSample::fist(0.55, 0.45)
    } else if ms < 2600 {
        let t = (ms - 2000) as f32 / 600.0;
        HandSample::fist(0.55, 0.40 + t * 0.25)
    } else if ms < 3200 {
        HandSample::open_palm(0.5, 0.2)
    } else if ms < 3600 {
        HandSample::open_palm(0.5, 0.55)
    } else {
        HandSample::open_palm(0.5, 0.5)
    }
}
