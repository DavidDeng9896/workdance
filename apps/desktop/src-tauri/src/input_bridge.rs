//! Bridges vision tier / HandFrame → gesture engine → inject + G07/G08 + WP5 voice-only listen.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use workdance_core::{load_config, now_stamp, write_memo, AppMode, HandFrame, VisionTier};
use workdance_input::{
    create_default_asr, create_default_injector, hand_frame_to_sample, G07Event, GestureEngine,
    HandSample, InjectCommand, InjectQueue, MemoEvent,
};

use crate::{tray, AppState};

/// Shared policy + tier for the gesture / voice-listen thread.
pub struct InputBridge {
    tier: Arc<Mutex<VisionTier>>,
    /// Latest vision hand frame (landmarks when Active + landmarker).
    latest_hand: Arc<Mutex<Option<HandFrame>>>,
    /// When true, cursor inject is stripped (voice-only / gestures off).
    cursor_enabled: Arc<AtomicBool>,
    /// Software-armed mic listen (no fist required) for voice-only fallback.
    voice_listen: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    _queue_keep: Mutex<Option<InjectQueue>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl InputBridge {
    pub fn start(app: AppHandle) -> Self {
        let cfg = load_config().unwrap_or_default();
        let tier = Arc::new(Mutex::new(VisionTier::Sleep));
        let latest_hand = Arc::new(Mutex::new(None));
        let cursor_enabled = Arc::new(AtomicBool::new(
            cfg.gesture_enabled && !cfg.voice_only,
        ));
        let voice_listen = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let queue = InjectQueue::spawn(create_default_injector());

        let force_stub = std::env::var("WORKDANCE_VISION_STUB")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
            || std::env::var("WORKDANCE_INPUT_STUB")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

        let tier_feed = tier.clone();
        let hand_feed = latest_hand.clone();
        let cursor_feed = cursor_enabled.clone();
        let listen_feed = voice_listen.clone();
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
                    hand_feed,
                    cursor_feed,
                    listen_feed,
                    stop_flag,
                    queue,
                );
            })
            .expect("spawn gesture thread");

        Self {
            tier,
            latest_hand,
            cursor_enabled,
            voice_listen,
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

    /// WP-M2: push latest vision observation (may include 21 landmarks).
    pub fn push_hand_frame(&self, frame: HandFrame) {
        *self.latest_hand.lock() = Some(frame);
    }

    /// Apply gesture_enabled / voice_only from settings.
    pub fn apply_policy(&self, gesture_enabled: bool, voice_only: bool) {
        let cursor = gesture_enabled && !voice_only;
        self.cursor_enabled.store(cursor, Ordering::SeqCst);
        if !cursor {
            // Force sleep tier so stub choreography cannot move the pointer.
            *self.tier.lock() = VisionTier::Sleep;
        }
        if !voice_only {
            self.voice_listen.store(false, Ordering::SeqCst);
        }
    }

    /// Arm software listen (tray 「仅语音 · 开始听写」). No fist required.
    pub fn start_voice_listen(&self) {
        self.voice_listen.store(true, Ordering::SeqCst);
    }

    pub fn stop_voice_listen(&self) {
        self.voice_listen.store(false, Ordering::SeqCst);
    }

    pub fn is_voice_listening(&self) -> bool {
        self.voice_listen.load(Ordering::SeqCst)
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
    latest_hand: Arc<Mutex<Option<HandFrame>>>,
    cursor_enabled: Arc<AtomicBool>,
    voice_listen: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    queue: InjectQueue,
) {
    let mut engine = GestureEngine::new(1920, 1080, sensitivity, dead_zone);
    let mut asr = create_default_asr();
    let start = Instant::now();
    let mut last_tier = VisionTier::Sleep;
    let mut last_cursor = true;
    let mut listen_was_on = false;
    eprintln!(
        "[workdance-input] gesture loop stub_samples={} landmark_path=wp-m2",
        force_stub
    );

    while !stop.load(Ordering::SeqCst) {
        let cursor = cursor_enabled.load(Ordering::SeqCst);
        if cursor != last_cursor {
            engine.set_cursor_enabled(cursor);
            last_cursor = cursor;
        }

        let listening = voice_listen.load(Ordering::SeqCst);
        if listening && !listen_was_on {
            set_recording_tray(&app, true);
            eprintln!("[workdance] voice-only listen armed (no cursor)");
        }
        if !listening && listen_was_on {
            // Stop → whole-segment offline ASR → append; no audio files.
            let result = asr.transcribe_zh(&[0u8; 64]);
            debug_assert!(result.audio_discarded);
            if result.text.is_empty() {
                // UnavailableAsr / empty — never inject StubAsr fixed sentence.
                eprintln!(
                    "[workdance] voice-only listen stopped → ASR {} empty (no inject)",
                    asr.name()
                );
            } else {
                let _ = queue.enqueue(InjectCommand::AppendText {
                    text: result.text.clone(),
                });
                let notes_path = app.state::<AppState>().config.lock().notes_path.clone();
                let _ = write_memo(&notes_path, &now_stamp(), &result.text);
                eprintln!("[workdance] voice-only listen stopped → append + memo");
            }
            set_recording_tray(&app, false);
        }
        listen_was_on = listening;

        let t = *tier.lock();
        if t != last_tier {
            engine.set_tier(t);
            last_tier = t;
        }

        // Gesture path only when cursor enabled and Active (开局无误触 when Sleep / voice-only).
        if cursor && t == VisionTier::Active {
            let ms = start.elapsed().as_millis() as u64;
            let hand = latest_hand.lock().clone();
            let sample = match hand.as_ref().and_then(hand_frame_to_sample) {
                Some(s) => s,
                // Presence-only / no landmarks: keep scripted stub path for CI.
                None if force_stub => stub_sample(ms % 10_000),
                None => HandSample::absent(),
            };
            let tick = engine.on_sample(ms, sample);
            apply_g07_tray(&app, &tier, &tick.g07_events);
            apply_g08_memos(&app, &tick.memo_events);
            let _ = queue.enqueue_all(tick.commands);
            thread::sleep(Duration::from_millis(32));
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

fn set_recording_tray(app: &AppHandle, recording: bool) {
    let state = app.state::<AppState>();
    {
        let mut rt = state.runtime.lock();
        if rt.manual_override {
            return;
        }
        if recording {
            rt.mode = AppMode::Recording;
            rt.recording_seconds = 0;
        } else {
            let voice_only = state.config.lock().voice_only;
            rt.mode = if voice_only {
                AppMode::Sleep
            } else {
                AppMode::from_vision_tier(VisionTier::Sleep)
            };
            rt.recording_seconds = 0;
        }
    }
    let _ = tray::refresh_tray(app);
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
                        eprintln!("[workdance] G08 memo saved: {}", rec.path.display());
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
