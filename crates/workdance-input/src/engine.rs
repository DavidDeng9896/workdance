use workdance_core::VisionTier;

use crate::asr::{create_default_asr, AsrBackend};
use crate::focus::{create_default_focus_probe, FocusLock, FocusProbe};
use crate::g07::{G07Event, G07Machine};

/// Max fist hold that still counts as G03 click.
pub const SHORT_FIST_MAX: u64 = 300;
/// G08 double short-fist window (second short release within this after the first).
pub const G08_WINDOW_MS: u64 = 500;

/// Openness below this ⇒ fist; at/above OPEN_PALM ⇒ open palm.
const FIST_MAX_OPENNESS: f32 = 0.35;
const OPEN_PALM_MIN_OPENNESS: f32 = 0.55;

/// Normalized dy (frame space) for G05 swipe-down (raw sample delta).
const SWIPE_DOWN_DY: f32 = 0.22;
/// Min |delta| in norm space before move/scroll after dead-zone.
const MIN_DELTA: f32 = 0.004;

/// One hand sample in normalized camera/selfie space (x,y in 0..1).
///
/// `x`/`y` track the **index fingertip** (G02 cursor). `palm_x`/`palm_y` track
/// palm-center motion for G05 swipe. Stub helpers set palm == tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandSample {
    pub present: bool,
    pub confidence: f32,
    pub x: f32,
    pub y: f32,
    /// Palm center (WP-M2 landmarks); equals `x`/`y` for stub samples.
    pub palm_x: f32,
    pub palm_y: f32,
    /// 0.0 = fist, 1.0 = open palm.
    pub openness: f32,
}

impl HandSample {
    pub fn open_palm(x: f32, y: f32) -> Self {
        Self {
            present: true,
            confidence: 0.95,
            x,
            y,
            palm_x: x,
            palm_y: y,
            openness: 0.95,
        }
    }

    pub fn fist(x: f32, y: f32) -> Self {
        Self {
            present: true,
            confidence: 0.95,
            x,
            y,
            palm_x: x,
            palm_y: y,
            openness: 0.1,
        }
    }

    pub fn absent() -> Self {
        Self {
            present: false,
            confidence: 0.0,
            x: 0.0,
            y: 0.0,
            palm_x: 0.0,
            palm_y: 0.0,
            openness: 0.0,
        }
    }

    pub fn is_fist(self) -> bool {
        self.present && self.openness < FIST_MAX_OPENNESS
    }

    pub fn is_open_palm(self) -> bool {
        self.present && self.openness >= OPEN_PALM_MIN_OPENNESS
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InjectCommand {
    MoveAbs { x: i32, y: i32 },
    MoveDelta { dx: i32, dy: i32 },
    ClickLeft,
    Scroll { dy: i32 },
    KeyEscape,
    /// Append Unicode text into the locked focus (WP3 G07).
    AppendText { text: String },
}

/// WP4 G08: request to persist a markdown memo (no audio).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoEvent {
    SaveRequested { body: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FistPhase {
    Idle,
    /// Fist started at `since_ms`; click pending until release or timeout.
    Holding { since_ms: u64 },
    /// Held past SHORT_FIST_MAX — G04 scroll mode until open palm.
    LongHold,
}

/// Result of one engine tick (inject intents + G07/G08 signals).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct EngineTick {
    pub commands: Vec<InjectCommand>,
    pub g07_events: Vec<G07Event>,
    pub memo_events: Vec<MemoEvent>,
}

/// Maps hand samples + vision tier → inject intents (no OS calls).
pub struct GestureEngine {
    screen_w: i32,
    screen_h: i32,
    sensitivity: f32,
    dead_zone: f32,
    tier: VisionTier,
    last_pos: Option<(f32, f32)>,
    last_raw: Option<(f32, f32)>,
    last_screen: Option<(i32, i32)>,
    smooth_x: crate::smoother::ExpSmoother,
    smooth_y: crate::smoother::ExpSmoother,
    fist: FistPhase,
    swipe_armed: bool,
    focus: FocusLock,
    probe: Box<dyn FocusProbe>,
    g07: G07Machine,
    asr: Box<dyn AsrBackend>,
    /// After a short-fist release, second short fist before this deadline ⇒ G08.
    g08_arm_until: Option<u64>,
    /// Last finished G07 transcript (in-memory only; never audio).
    last_transcript: String,
    /// When false (voice-only), suppress cursor/scroll/key inject; dictation append kept.
    cursor_enabled: bool,
}

impl GestureEngine {
    pub fn new(screen_w: i32, screen_h: i32, sensitivity: f32, dead_zone: f32) -> Self {
        let alpha = (0.25 + sensitivity.clamp(0.0, 1.0) * 0.55).clamp(0.15, 0.85);
        Self {
            screen_w: screen_w.max(1),
            screen_h: screen_h.max(1),
            sensitivity: sensitivity.clamp(0.05, 1.0),
            dead_zone: dead_zone.clamp(0.0, 0.4),
            tier: VisionTier::Sleep,
            last_pos: None,
            last_raw: None,
            last_screen: None,
            smooth_x: crate::smoother::ExpSmoother::new(alpha),
            smooth_y: crate::smoother::ExpSmoother::new(alpha),
            fist: FistPhase::Idle,
            swipe_armed: true,
            focus: FocusLock::new(),
            probe: create_default_focus_probe(),
            g07: G07Machine::new(),
            asr: create_default_asr(),
            g08_arm_until: None,
            last_transcript: String::new(),
            cursor_enabled: true,
        }
    }

    pub fn with_probe(mut self, probe: Box<dyn FocusProbe>) -> Self {
        self.probe = probe;
        self
    }

    pub fn with_asr(mut self, asr: Box<dyn AsrBackend>) -> Self {
        self.asr = asr;
        self
    }

    /// WP5 voice-only: disable cursor/scroll/key inject while keeping AppendText path.
    pub fn set_cursor_enabled(&mut self, enabled: bool) {
        self.cursor_enabled = enabled;
        if !enabled {
            self.reset_motion();
        }
    }

    pub fn cursor_enabled(&self) -> bool {
        self.cursor_enabled
    }

    fn strip_cursor_commands(commands: &mut Vec<InjectCommand>) {
        commands.retain(|c| {
            matches!(
                c,
                InjectCommand::AppendText { .. }
            )
        });
    }

    pub fn focus_lock(&self) -> &FocusLock {
        &self.focus
    }

    pub fn is_recording(&self) -> bool {
        self.g07.is_recording()
    }

    pub fn last_transcript(&self) -> &str {
        &self.last_transcript
    }

    pub fn set_tier(&mut self, tier: VisionTier) {
        if tier == VisionTier::Sleep {
            self.reset_motion();
        }
        self.tier = tier;
    }

    fn reset_motion(&mut self) {
        self.last_pos = None;
        self.last_raw = None;
        self.smooth_x.reset();
        self.smooth_y.reset();
        self.fist = FistPhase::Idle;
        self.swipe_armed = true;
        self.g08_arm_until = None;
        // Focus lock + last_transcript intentionally kept.
    }

    fn screen_from_norm(&self, sx: f32, sy: f32) -> (i32, i32) {
        let screen_x = (sx * self.screen_w as f32)
            .clamp(0.0, (self.screen_w - 1) as f32)
            .round() as i32;
        // Mirror X for selfie front camera.
        let screen_x = self.screen_w - 1 - screen_x;
        let screen_y = (sy * self.screen_h as f32)
            .clamp(0.0, (self.screen_h - 1) as f32)
            .round() as i32;
        (screen_x, screen_y)
    }

    fn update_focus_hover(&mut self, now_ms: u64, screen: (i32, i32)) {
        let hit = self.probe.hit_test(screen.0, screen.1);
        self.focus.on_hover(now_ms, hit);
    }

    fn remember_transcript_from_g07(&mut self, events: &[G07Event]) {
        for ev in events {
            if let G07Event::DictationReady { text } = ev {
                self.last_transcript = text.clone();
            }
        }
    }

    /// G08: stop recording if active, then request memo save from last/just-finished transcript.
    fn fire_g08(&mut self, now_ms: u64, out: &mut EngineTick) {
        if self.g07.is_recording()
            || matches!(self.g07.phase(), crate::g07::G07Phase::Arming { .. })
        {
            self.apply_g07(now_ms, true, false, out);
        }
        let body = if self.last_transcript.trim().is_empty() {
            "(无听写内容)".to_string()
        } else {
            self.last_transcript.clone()
        };
        out.memo_events
            .push(MemoEvent::SaveRequested { body });
    }

    fn apply_g07(
        &mut self,
        now_ms: u64,
        present: bool,
        fist: bool,
        out: &mut EngineTick,
    ) {
        let events = self.g07.on_sample(now_ms, present, fist, self.asr.as_mut());
        self.remember_transcript_from_g07(&events);
        for ev in events {
            match &ev {
                G07Event::DictationReady { text } => {
                    out.commands.push(InjectCommand::AppendText {
                        text: text.clone(),
                    });
                }
                G07Event::RecordingStarted | G07Event::RecordingAborted => {}
            }
            out.g07_events.push(ev);
        }
    }

    pub fn on_sample(&mut self, now_ms: u64, sample: HandSample) -> EngineTick {
        let mut out = EngineTick::default();

        if self.tier != VisionTier::Active {
            // If we somehow leave Active while armed/recording, abort capture.
            if self.g07.is_recording()
                || matches!(self.g07.phase(), crate::g07::G07Phase::Arming { .. })
            {
                self.apply_g07(now_ms, false, false, &mut out);
            }
            return out;
        }

        if !sample.present || sample.confidence < workdance_core::MIN_PALM_CONFIDENCE {
            self.apply_g07(now_ms, false, false, &mut out);
            self.reset_motion();
            return out;
        }

        let sx = self.smooth_x.push(sample.x);
        let sy = self.smooth_y.push(sample.y);
        let screen = self.screen_from_norm(sx, sy);
        self.last_screen = Some(screen);
        self.update_focus_hover(now_ms, screen);

        // Keep raw previous palm for G05 swipe (smoothing would shrink fast flicks).
        let raw_prev = self.last_raw;
        self.last_raw = Some((sample.palm_x, sample.palm_y));

        // --- Fist state machine (G03 / G04 / G08 double short-fist) ---
        // Heuristic: two short-fist releases (<SHORT_FIST_MAX) within G08_WINDOW_MS
        // ⇒ G08 memo (second release suppresses ClickLeft). Distinct from G05 swipe-down
        // and G07 long-fist ≥1s.
        match self.fist {
            FistPhase::Idle => {
                if sample.is_fist() {
                    self.fist = FistPhase::Holding { since_ms: now_ms };
                    // Fist reconfirm of pending / locked focus target.
                    self.focus.reconfirm();
                }
            }
            FistPhase::Holding { since_ms } => {
                if sample.is_fist() {
                    if now_ms.saturating_sub(since_ms) >= SHORT_FIST_MAX {
                        self.fist = FistPhase::LongHold;
                        self.g08_arm_until = None;
                    }
                } else if sample.is_open_palm() {
                    let held = now_ms.saturating_sub(since_ms);
                    if held < SHORT_FIST_MAX {
                        let is_g08 = self
                            .g08_arm_until
                            .map(|until| now_ms <= until)
                            .unwrap_or(false);
                        if is_g08 {
                            self.g08_arm_until = None;
                            self.fire_g08(now_ms, &mut out);
                        } else {
                            out.commands.push(InjectCommand::ClickLeft);
                            self.g08_arm_until = Some(now_ms.saturating_add(G08_WINDOW_MS));
                        }
                    } else {
                        self.g08_arm_until = None;
                    }
                    self.fist = FistPhase::Idle;
                } else {
                    self.fist = FistPhase::Idle;
                }
            }
            FistPhase::LongHold => {
                if sample.is_open_palm() {
                    self.fist = FistPhase::Idle;
                    self.g08_arm_until = None;
                }
            }
        }

        // G07: long fist ≥1s → record; leave/release handled inside machine.
        // Skip feeding fist=true into G07 on the same tick we already force-released for G08.
        let skip_g07_fist = out
            .memo_events
            .iter()
            .any(|e| matches!(e, MemoEvent::SaveRequested { .. }));
        if !skip_g07_fist {
            self.apply_g07(now_ms, true, sample.is_fist(), &mut out);
        }
        let recording = self.g07.is_recording();

        // --- Motion: G02 move vs G04 scroll vs G05 swipe ---
        // Spec: gesture move does not unlock focus or interrupt voice.
        if let Some((px, py)) = self.last_pos {
            let mut dx = sx - px;
            let mut dy = sy - py;
            if dx.abs() < self.dead_zone {
                dx = 0.0;
            }
            if dy.abs() < self.dead_zone {
                dy = 0.0;
            }

            let scrolling = matches!(self.fist, FistPhase::LongHold)
                && sample.is_fist()
                && !recording;
            let moving = sample.is_open_palm() && matches!(self.fist, FistPhase::Idle);

            if scrolling {
                if dy.abs() >= MIN_DELTA {
                    let scroll = (dy * 800.0 * self.sensitivity).round() as i32;
                    if scroll != 0 {
                        out.commands.push(InjectCommand::Scroll { dy: scroll });
                    }
                }
            } else if moving {
                if let Some((rx0, ry0)) = raw_prev {
                    let raw_dy = sample.palm_y - ry0;
                    let raw_dx = sample.palm_x - rx0;
                    if self.swipe_armed && raw_dy >= SWIPE_DOWN_DY && raw_dx.abs() < 0.12 {
                        out.commands.push(InjectCommand::KeyEscape);
                        self.swipe_armed = false;
                    }
                }
                if !out
                    .commands
                    .iter()
                    .any(|c| matches!(c, InjectCommand::KeyEscape))
                    && (dx.abs() >= MIN_DELTA || dy.abs() >= MIN_DELTA)
                {
                    out.commands.push(InjectCommand::MoveAbs {
                        x: screen.0,
                        y: screen.1,
                    });
                }
            }
        }

        if sample.is_open_palm() && !self.swipe_armed && sample.palm_y < 0.35 {
            self.swipe_armed = true;
        }

        self.last_pos = Some((sx, sy));
        if !self.cursor_enabled {
            Self::strip_cursor_commands(&mut out.commands);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::StubAsr;
    use crate::focus::FocusTarget;
    use crate::g07::G07Event;

    fn engine() -> GestureEngine {
        GestureEngine::new(1920, 1080, 0.7, 0.02)
    }

    #[test]
    fn short_fist_under_300ms_emits_click() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(200, HandSample::open_palm(0.5, 0.5));
        assert!(
            out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::ClickLeft)),
            "expected ClickLeft for short fist, got {:?}",
            out.commands
        );
    }

    #[test]
    fn fist_held_over_300ms_is_not_a_click() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let mid = e.on_sample(400, HandSample::fist(0.5, 0.5));
        assert!(!mid
            .commands
            .iter()
            .any(|c| matches!(c, InjectCommand::ClickLeft)));
        let out = e.on_sample(450, HandSample::open_palm(0.5, 0.5));
        assert!(
            !out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::ClickLeft)),
            "long fist must not click, got {:?}",
            out.commands
        );
    }

    #[test]
    fn sleep_tier_suppresses_all_inject() {
        let mut e = engine();
        e.set_tier(VisionTier::Sleep);
        let out = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        assert!(
            out.commands.is_empty(),
            "sleep must not move cursor: {:?}",
            out.commands
        );
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(100, HandSample::open_palm(0.5, 0.5));
        assert!(
            out.commands.is_empty(),
            "sleep must not click: {:?}",
            out.commands
        );
    }

    #[test]
    fn open_palm_translate_moves_not_scrolls() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.4, 0.4));
        let out = e.on_sample(32, HandSample::open_palm(0.55, 0.4));
        assert!(
            out.commands.iter().any(|c| matches!(
                c,
                InjectCommand::MoveAbs { .. } | InjectCommand::MoveDelta { .. }
            )),
            "G02 should move, got {:?}",
            out.commands
        );
        assert!(!out
            .commands
            .iter()
            .any(|c| matches!(c, InjectCommand::Scroll { .. })));
    }

    #[test]
    fn fist_translate_scrolls_not_moves() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let _ = e.on_sample(400, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(432, HandSample::fist(0.5, 0.65));
        assert!(
            out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::Scroll { .. })),
            "G04 fist+translate should scroll, got {:?}",
            out.commands
        );
        assert!(!out.commands.iter().any(|c| matches!(
            c,
            InjectCommand::MoveAbs { .. } | InjectCommand::MoveDelta { .. }
        )));
    }

    #[test]
    fn palm_swipe_down_emits_escape() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.2));
        let out = e.on_sample(80, HandSample::open_palm(0.5, 0.55));
        assert!(
            out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::KeyEscape)),
            "G05 swipe down → Esc, got {:?}",
            out.commands
        );
    }

    #[test]
    fn dwell_0_4s_locks_via_engine_hover() {
        struct FixedProbe(FocusTarget);
        impl FocusProbe for FixedProbe {
            fn hit_test(&self, _x: i32, _y: i32) -> Option<FocusTarget> {
                Some(self.0.clone())
            }
        }
        let t = FocusTarget {
            id: "edit-1".into(),
            label: "input".into(),
            is_editable: true,
        };
        let mut e = engine().with_probe(Box::new(FixedProbe(t.clone())));
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        assert!(e.focus_lock().locked().is_none());
        let _ = e.on_sample(400, HandSample::open_palm(0.51, 0.5));
        assert_eq!(e.focus_lock().locked(), Some(&t));
    }

    #[test]
    fn g07_under_1s_no_record_no_append() {
        let mut e = engine().with_asr(Box::new(StubAsr));
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let mid = e.on_sample(500, HandSample::fist(0.5, 0.5));
        assert!(!e.is_recording());
        assert!(!mid
            .g07_events
            .iter()
            .any(|ev| matches!(ev, G07Event::RecordingStarted)));
        let out = e.on_sample(600, HandSample::open_palm(0.5, 0.5));
        assert!(!out
            .commands
            .iter()
            .any(|c| matches!(c, InjectCommand::AppendText { .. })));
    }

    #[test]
    fn g07_at_1s_starts_recording() {
        let mut e = engine().with_asr(Box::new(StubAsr));
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(1010, HandSample::fist(0.5, 0.5));
        assert!(e.is_recording());
        assert!(out
            .g07_events
            .iter()
            .any(|ev| matches!(ev, G07Event::RecordingStarted)));
    }

    #[test]
    fn g07_leave_frame_aborts() {
        let mut e = engine().with_asr(Box::new(StubAsr));
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let _ = e.on_sample(1010, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(1200, HandSample::absent());
        assert!(!e.is_recording());
        assert!(out
            .g07_events
            .iter()
            .any(|ev| matches!(ev, G07Event::RecordingAborted)));
        assert!(!out
            .commands
            .iter()
            .any(|c| matches!(c, InjectCommand::AppendText { .. })));
    }

    #[test]
    fn g07_release_appends_stub_asr_text() {
        let mut e = engine().with_asr(Box::new(StubAsr));
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let _ = e.on_sample(1010, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(1500, HandSample::open_palm(0.5, 0.5));
        let append = out
            .commands
            .iter()
            .find_map(|c| match c {
                InjectCommand::AppendText { text } => Some(text.as_str()),
                _ => None,
            });
        let text = append.expect("AppendText on release");
        assert!(!text.is_empty());
        assert!(text.chars().any(|c| c > '\u{7f}'));
        assert!(out
            .g07_events
            .iter()
            .any(|ev| matches!(ev, G07Event::DictationReady { .. })));
    }

    #[test]
    fn gesture_move_does_not_clear_focus_or_stop_recording() {
        struct FixedProbe(FocusTarget);
        impl FocusProbe for FixedProbe {
            fn hit_test(&self, _x: i32, _y: i32) -> Option<FocusTarget> {
                Some(self.0.clone())
            }
        }
        let t = FocusTarget {
            id: "edit-1".into(),
            label: "input".into(),
            is_editable: true,
        };
        let mut e = engine()
            .with_probe(Box::new(FixedProbe(t.clone())))
            .with_asr(Box::new(StubAsr));
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.4, 0.4));
        let _ = e.on_sample(400, HandSample::open_palm(0.4, 0.4));
        assert_eq!(e.focus_lock().locked(), Some(&t));
        let _ = e.on_sample(410, HandSample::fist(0.4, 0.4));
        let _ = e.on_sample(1410, HandSample::fist(0.5, 0.5));
        assert!(e.is_recording());
        // Fist translate while recording must not unlock focus.
        let _ = e.on_sample(1450, HandSample::fist(0.55, 0.6));
        assert_eq!(e.focus_lock().locked(), Some(&t));
        assert!(e.is_recording());
    }

    #[test]
    fn g08_double_short_fist_requests_memo() {
        let mut e = engine().with_asr(Box::new(StubAsr));
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        // First short fist → click + arm G08 window.
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let first = e.on_sample(100, HandSample::open_palm(0.5, 0.5));
        assert!(first
            .commands
            .iter()
            .any(|c| matches!(c, InjectCommand::ClickLeft)));
        assert!(first.memo_events.is_empty());
        // Second short fist within 500ms → G08, no click.
        let _ = e.on_sample(150, HandSample::fist(0.5, 0.5));
        let second = e.on_sample(250, HandSample::open_palm(0.5, 0.5));
        assert!(!second
            .commands
            .iter()
            .any(|c| matches!(c, InjectCommand::ClickLeft)));
        assert!(
            second
                .memo_events
                .iter()
                .any(|m| matches!(m, MemoEvent::SaveRequested { .. })),
            "expected G08 SaveRequested, got {:?}",
            second.memo_events
        );
    }

    #[test]
    fn g08_after_g07_includes_transcript_body() {
        let mut e = engine().with_asr(Box::new(StubAsr));
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let _ = e.on_sample(1010, HandSample::fist(0.5, 0.5));
        let _ = e.on_sample(1500, HandSample::open_palm(0.5, 0.5));
        assert!(!e.last_transcript().is_empty());
        // Double short fist for G08.
        let _ = e.on_sample(1600, HandSample::fist(0.5, 0.5));
        let _ = e.on_sample(1700, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(1750, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(1850, HandSample::open_palm(0.5, 0.5));
        match out.memo_events.as_slice() {
            [MemoEvent::SaveRequested { body }] => {
                assert!(body.chars().any(|c| c > '\u{7f}'));
                assert_eq!(body, e.last_transcript());
            }
            other => panic!("expected SaveRequested with transcript, got {other:?}"),
        }
    }

    #[test]
    fn single_short_fist_is_not_g08() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(100, HandSample::open_palm(0.5, 0.5));
        assert!(out.memo_events.is_empty());
        assert!(out
            .commands
            .iter()
            .any(|c| matches!(c, InjectCommand::ClickLeft)));
    }

    #[test]
    fn voice_only_suppresses_cursor_inject() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        e.set_cursor_enabled(false);
        let _ = e.on_sample(0, HandSample::open_palm(0.4, 0.4));
        let out = e.on_sample(32, HandSample::open_palm(0.55, 0.4));
        assert!(
            out.commands.is_empty(),
            "voice-only must not move cursor: {:?}",
            out.commands
        );
        let _ = e.on_sample(40, HandSample::fist(0.55, 0.4));
        let out = e.on_sample(100, HandSample::open_palm(0.55, 0.4));
        assert!(
            !out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::ClickLeft)),
            "voice-only must not click: {:?}",
            out.commands
        );
    }

    #[test]
    fn voice_only_still_allows_append_text_commands() {
        let mut e = engine().with_asr(Box::new(StubAsr));
        e.set_tier(VisionTier::Active);
        e.set_cursor_enabled(false);
        // Manually push path: G07 still runs if samples arrive (fist ≥1s).
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let _ = e.on_sample(1010, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(1500, HandSample::open_palm(0.5, 0.5));
        assert!(
            out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::AppendText { .. })),
            "dictation append must survive voice-only filter: {:?}",
            out.commands
        );
        assert!(
            !out.commands.iter().any(|c| matches!(
                c,
                InjectCommand::MoveAbs { .. }
                    | InjectCommand::ClickLeft
                    | InjectCommand::Scroll { .. }
                    | InjectCommand::KeyEscape
            )),
            "no cursor side-effects: {:?}",
            out.commands
        );
    }
}
