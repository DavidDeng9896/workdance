//! Bridges vision tier → gesture engine → serial inject queue + G07 tray + G08 memos.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use workdance_core::{load_config, now_stamp, write_memo, AppMode, VisionTier};
use workdance_input::{
    create_default_injector, G07Event, GestureEngine, HandSample, InjectQueue, MemoEvent,
};

use crate::{tray, AppState};

/// Shared tier flag updated from vision_bridge.
pub struct InputBridge {
    tier: Arc<Mutex<VisionTier>>,
    stop: Arc<AtomicBool>,
    _queue_keep: Mutex<Option<InjectQueue>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl InputBridge {
    pub fn start(app: AppHandle) -> Self {
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
        let handle = thread::Builder::new()
            .name("workdance-gesture".into())
            .spawn(move || {
                gesture_loop(
                    app,
                    cfg.sensitivity,
                    cfg.dead_zone,
                    force_stub,
                    tier_feed,
                    stop_flag,
                    queue,
                );
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

    pub fn current_tier(&self) -> VisionTier {
        *self.tier.lock()
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
    app: AppHandle,
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
        "[workdance-input] gesture loop stub_samples={} (WP3 G07 + WP4 G08)",
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
            let sample = stub_sample(ms % 10_000);
            let tick = engine.on_sample(ms, sample);
            apply_g07_tray(&app, &tier, &tick.g07_events);
            apply_g08_memos(&app, &tick.memo_events);
            let _ = queue.enqueue_all(tick.commands);
            thread::sleep(Duration::from_millis(32)); // ~30 FPS active
        } else {
            if engine.is_recording() {
                let ms = start.elapsed().as_millis() as u64;
                let tick = engine.on_sample(ms, HandSample::absent());
                apply_g07_tray(&app, &tier, &tick.g07_events);
                apply_g08_memos(&app, &tick.memo_events);
                let _ = queue.enqueue_all(tick.commands);
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    queue.stop();
}

fn apply_g07_tray(app: &AppHandle, tier: &Arc<Mutex<VisionTier>>, events: &[G07Event]) {
    for ev in events {
        match ev {
            G07Event::RecordingStarted => {
                let state = app.state::<AppState>();
                {
                    let mut rt = state.runtime.lock();
                    if !rt.manual_override {
                        rt.mode = AppMode::Recording;
                        rt.recording_seconds = 0;
                    }
                }
                let _ = tray::refresh_tray(app);
            }
            G07Event::RecordingAborted | G07Event::DictationReady { .. } => {
                let state = app.state::<AppState>();
                {
                    let mut rt = state.runtime.lock();
                    if !rt.manual_override && rt.mode == AppMode::Recording {
                        rt.mode = AppMode::from_vision_tier(*tier.lock());
                        rt.recording_seconds = 0;
                    }
                }
                let _ = tray::refresh_tray(app);
            }
        }
    }
}

fn apply_g08_memos(app: &AppHandle, events: &[MemoEvent]) {
    for ev in events {
        match ev {
            MemoEvent::SaveRequested { body } => {
                let notes_path = app.state::<AppState>().config.lock().notes_path.clone();
                match write_memo(&notes_path, &now_stamp(), body) {
                    Ok(rec) => {
                        eprintln!(
                            "[workdance] G08 memo saved: {}",
                            rec.path.display()
                        );
                    }
                    Err(err) => {
                        eprintln!("[workdance] G08 memo save failed: {err}");
                    }
                }
            }
        }
    }
}

/// Deterministic stub choreography including G08 double short-fist.
fn stub_sample(ms: u64) -> HandSample {
    // 0–800: open palm drift (G02)
    // 800–1000: short fist → click (G03)
    // 1000–1400: open again
    // 1400–2000: long fist hold (G04 arm)
    // 2000–2600: fist scroll down (G04)
    // 2600–3200: open at top
    // 3200–3600: swipe down (G05)
    // 3600–4000: open settle
    // 4000–5200: fist hold ≥1s → G07 record
    // 5200–5600: release → stub ASR append
    // 5600–5800: open
    // 5800–5950: short fist 1 (G08 arm)
    // 5950–6100: open
    // 6100–6250: short fist 2 → G08 memo
    // 6250–10000: idle open
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
    } else if ms < 4000 {
        HandSample::open_palm(0.5, 0.5)
    } else if ms < 5200 {
        HandSample::fist(0.5, 0.5)
    } else if ms < 5600 {
        HandSample::open_palm(0.5, 0.5)
    } else if ms < 5800 {
        HandSample::open_palm(0.5, 0.5)
    } else if ms < 5950 {
        HandSample::fist(0.5, 0.5)
    } else if ms < 6100 {
        HandSample::open_palm(0.5, 0.5)
    } else if ms < 6250 {
        HandSample::fist(0.5, 0.5)
    } else {
        HandSample::open_palm(0.5, 0.5)
    }
}
