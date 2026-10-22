//! Temporal gate: open-palm follow, short-fist click, long-fist dictation.
//!
//! Durations are provisional integration targets, not a tuned user study.
//! Palm-flick scroll is intentionally absent (`scroll` stays 0).

use serde::{Deserialize, Serialize};

use crate::classify::{classify_pose, palm_center, Pose};
use crate::frame::HandObservation;
use crate::pointer::MOVE_DEADZONE;
use crate::MIN_HAND_CONFIDENCE;

/// Release before this is a left click. Provisional.
pub const PROVISIONAL_SHORT_FIST_MAX_MS: u64 = 400;
/// Still closed at this age starts dictation. Provisional.
pub const PROVISIONAL_LONG_FIST_MIN_MS: u64 = 800;
/// After an ignored fist, keep the pointer quiet. Provisional.
pub const PROVISIONAL_MISTOUCH_PAUSE_MS: u64 = 350;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayState {
    Idle,
    Tracking,
    Recording,
    RejectCooldown,
    /// MediaPipe lib or hand model missing / failed to load. Never shown as Idle.
    NotInstalled,
    /// macOS Accessibility (or equivalent) denied — pointer inject cannot work.
    NeedsAccessibility,
}

impl TrayState {
    /// Tray state to display: tracker load error and accessibility denial override the gate.
    pub fn effective(
        gate_state: TrayState,
        tracker_error: Option<&str>,
        accessibility_denied: bool,
    ) -> TrayState {
        if tracker_error.is_some() {
            TrayState::NotInstalled
        } else if accessibility_denied {
            TrayState::NeedsAccessibility
        } else {
            gate_state
        }
    }

    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Idle => "安静",
            Self::Tracking => "正在听手",
            Self::Recording => "正在录音",
            Self::RejectCooldown => "刚误触停一下",
            Self::NotInstalled => "手势没装好",
            Self::NeedsAccessibility => "需要辅助功能",
        }
    }

    pub fn label_en(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Tracking => "Tracking",
            Self::Recording => "Recording",
            Self::RejectCooldown => "Reject cooldown",
            Self::NotInstalled => "Gesture not installed",
            Self::NeedsAccessibility => "Needs Accessibility",
        }
    }

    pub fn tooltip(self) -> String {
        format!("WorkDance · {} / {}", self.label_zh(), self.label_en())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    None,
    Move,
    Click,
    DictationPressed,
    DictationReleased,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    Move { x: f32, y: f32 },
    Click,
    DictationPressed,
    DictationReleased,
}

impl Decision {
    pub fn kind(self) -> DecisionKind {
        match self {
            Self::Move { .. } => DecisionKind::Move,
            Self::Click => DecisionKind::Click,
            Self::DictationPressed => DecisionKind::DictationPressed,
            Self::DictationReleased => DecisionKind::DictationReleased,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct EventCounts {
    /// Open-palm pointer moves. Serialized as `move`.
    #[serde(rename = "move")]
    pub move_count: u64,
    pub click: u64,
    /// Reserved. Palm-flick scroll is not implemented; this stays 0.
    pub scroll: u64,
    pub dictation_pressed: u64,
    pub dictation_released: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GateSnapshot {
    /// Aegis assertion: gesture loop allowed.
    pub gesture_enabled: bool,
    /// Aegis assertion: idle / tracking / recording / reject_cooldown.
    pub gate_state: TrayState,
    pub pose: Pose,
    pub recording: bool,
    pub landmark_count: u8,
    pub fist_held_ms: u64,
    pub mis_touch_remaining_ms: u64,
    pub last_decision: DecisionKind,
    /// Aegis assertion: move / click / scroll / dictation_pressed / dictation_released.
    pub session_counts: EventCounts,
    pub short_fist_max_ms: u64,
    pub long_fist_min_ms: u64,
    pub mis_touch_pause_ms: u64,
    pub thresholds_provisional: bool,
    pub scroll_implemented: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct GateConfig {
    pub short_max_ms: u64,
    pub long_min_ms: u64,
    pub pause_ms: u64,
    pub min_confidence: f32,
    pub move_deadzone: f32,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            short_max_ms: PROVISIONAL_SHORT_FIST_MAX_MS,
            long_min_ms: PROVISIONAL_LONG_FIST_MIN_MS,
            pause_ms: PROVISIONAL_MISTOUCH_PAUSE_MS,
            min_confidence: MIN_HAND_CONFIDENCE,
            move_deadzone: MOVE_DEADZONE,
        }
    }
}

#[derive(Debug)]
enum FistRelease {
    None,
    Click,
    Ignore,
}

pub struct GestureGate {
    cfg: GateConfig,
    enabled: bool,
    recording: bool,
    fist_started_ms: Option<u64>,
    mis_touch_until_ms: Option<u64>,
    last_palm: Option<(f32, f32)>,
    last_t_ms: u64,
    pose: Pose,
    tray: TrayState,
    landmark_count: u8,
    fist_held_ms: u64,
    last_decision: DecisionKind,
    counts: EventCounts,
}

impl Default for GestureGate {
    fn default() -> Self {
        Self::new(GateConfig::default())
    }
}

impl GestureGate {
    pub fn new(cfg: GateConfig) -> Self {
        Self {
            cfg,
            enabled: true,
            recording: false,
            fist_started_ms: None,
            mis_touch_until_ms: None,
            last_palm: None,
            last_t_ms: 0,
            pose: Pose::Absent,
            tray: TrayState::Idle,
            landmark_count: 0,
            fist_held_ms: 0,
            last_decision: DecisionKind::None,
            counts: EventCounts::default(),
        }
    }

    pub fn snapshot(&self) -> GateSnapshot {
        let remaining = self
            .mis_touch_until_ms
            .map(|until| until.saturating_sub(self.last_t_ms))
            .unwrap_or(0);
        GateSnapshot {
            gesture_enabled: self.enabled,
            gate_state: self.tray,
            pose: self.pose,
            recording: self.recording,
            landmark_count: self.landmark_count,
            fist_held_ms: self.fist_held_ms,
            mis_touch_remaining_ms: remaining,
            last_decision: self.last_decision,
            session_counts: self.counts,
            short_fist_max_ms: self.cfg.short_max_ms,
            long_fist_min_ms: self.cfg.long_min_ms,
            mis_touch_pause_ms: self.cfg.pause_ms,
            thresholds_provisional: true,
            scroll_implemented: false,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) -> Vec<Decision> {
        self.enabled = enabled;
        if enabled {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.recording {
            self.finish_recording(&mut out);
        }
        self.fist_started_ms = None;
        self.fist_held_ms = 0;
        self.mis_touch_until_ms = None;
        self.last_palm = None;
        self.pose = Pose::Absent;
        self.landmark_count = 0;
        self.tray = TrayState::Idle;
        out
    }

    /// OpenLess already owns a dictation this gate did not start.
    /// Drop the owned-recording flag without emitting another edge.
    pub fn abandon_recording(&mut self) {
        self.recording = false;
        self.fist_started_ms = None;
        self.fist_held_ms = 0;
        if self.tray == TrayState::Recording {
            self.tray = TrayState::Idle;
        }
    }

    pub fn push(&mut self, obs: HandObservation, external_busy: bool) -> Vec<Decision> {
        self.last_t_ms = obs.t_ms;
        if !self.enabled {
            return self.set_enabled(false);
        }

        let pose = self.pose_of(&obs);
        self.pose = pose;
        self.landmark_count = if matches!(pose, Pose::Absent) { 0 } else { 21 };

        if external_busy && !self.recording {
            self.fist_started_ms = None;
            self.fist_held_ms = 0;
            self.last_palm = None;
            self.tray = TrayState::Recording;
            return Vec::new();
        }

        let mut out = Vec::new();
        if self.recording {
            self.step_recording(pose, obs.t_ms, &mut out);
        } else {
            self.step_idle(pose, &obs, &mut out);
        }
        out
    }

    fn pose_of(&self, obs: &HandObservation) -> Pose {
        match obs.landmarks.as_ref() {
            Some(lm) if obs.confidence >= self.cfg.min_confidence => classify_pose(lm),
            _ => Pose::Absent,
        }
    }

    fn step_recording(&mut self, pose: Pose, t_ms: u64, out: &mut Vec<Decision>) {
        if pose == Pose::Fist {
            self.tray = TrayState::Recording;
            self.fist_held_ms = self
                .fist_started_ms
                .map(|start| t_ms.saturating_sub(start))
                .unwrap_or(0);
            return;
        }
        // Release, another shape, or the hand left the frame.
        self.finish_recording(out);
        self.fist_started_ms = None;
        self.fist_held_ms = 0;
        self.last_palm = None;
        if pose == Pose::Absent {
            self.tray = TrayState::Idle;
            self.mis_touch_until_ms = None;
        } else {
            self.arm_pause(t_ms);
        }
    }

    fn step_idle(&mut self, pose: Pose, obs: &HandObservation, out: &mut Vec<Decision>) {
        match pose {
            Pose::Absent => {
                self.fist_started_ms = None;
                self.fist_held_ms = 0;
                self.last_palm = None;
                self.tray = if self.in_pause(obs.t_ms) {
                    TrayState::RejectCooldown
                } else {
                    TrayState::Idle
                };
            }
            Pose::Fist => {
                if self.fist_started_ms.is_none() {
                    self.fist_started_ms = Some(obs.t_ms);
                }
                let held = obs
                    .t_ms
                    .saturating_sub(self.fist_started_ms.unwrap_or(obs.t_ms));
                self.fist_held_ms = held;
                self.last_palm = None;
                if held >= self.cfg.long_min_ms {
                    self.recording = true;
                    self.tray = TrayState::Recording;
                    self.emit(Decision::DictationPressed, out);
                } else {
                    self.tray = TrayState::RejectCooldown;
                }
            }
            Pose::OpenPalm => {
                self.on_fist_release(obs.t_ms, out);
                if self.in_pause(obs.t_ms) {
                    self.tray = TrayState::RejectCooldown;
                    return;
                }
                self.tray = TrayState::Tracking;
                if let Some(lm) = obs.landmarks.as_ref() {
                    let (x, y) = palm_center(lm);
                    if self.should_move(x, y) {
                        self.last_palm = Some((x, y));
                        self.emit(Decision::Move { x, y }, out);
                    }
                }
            }
            Pose::Other => {
                self.on_fist_release(obs.t_ms, out);
                self.last_palm = None;
                if self.in_pause(obs.t_ms) {
                    self.tray = TrayState::RejectCooldown;
                } else if self.tray != TrayState::RejectCooldown {
                    self.tray = TrayState::Idle;
                } else if self.mis_touch_until_ms.is_none() {
                    self.tray = TrayState::Idle;
                }
            }
        }
    }

    fn on_fist_release(&mut self, t_ms: u64, out: &mut Vec<Decision>) {
        match self.take_fist_release(t_ms) {
            FistRelease::None => {}
            FistRelease::Click => {
                self.emit(Decision::Click, out);
                self.arm_pause(t_ms);
            }
            FistRelease::Ignore => self.arm_pause(t_ms),
        }
    }

    fn take_fist_release(&mut self, t_ms: u64) -> FistRelease {
        let Some(start) = self.fist_started_ms.take() else {
            return FistRelease::None;
        };
        let held = t_ms.saturating_sub(start);
        self.fist_held_ms = held;
        if held < self.cfg.short_max_ms {
            FistRelease::Click
        } else if held < self.cfg.long_min_ms {
            FistRelease::Ignore
        } else {
            FistRelease::Ignore
        }
    }

    fn arm_pause(&mut self, t_ms: u64) {
        self.mis_touch_until_ms = Some(t_ms.saturating_add(self.cfg.pause_ms));
        self.tray = TrayState::RejectCooldown;
        self.last_palm = None;
    }

    fn in_pause(&self, t_ms: u64) -> bool {
        self.mis_touch_until_ms.is_some_and(|until| t_ms < until)
    }

    fn should_move(&self, x: f32, y: f32) -> bool {
        match self.last_palm {
            None => true,
            Some((px, py)) => {
                let dx = x - px;
                let dy = y - py;
                (dx * dx + dy * dy).sqrt() >= self.cfg.move_deadzone
            }
        }
    }

    fn finish_recording(&mut self, out: &mut Vec<Decision>) {
        self.recording = false;
        self.emit(Decision::DictationReleased, out);
    }

    fn emit(&mut self, decision: Decision, out: &mut Vec<Decision>) {
        self.last_decision = decision.kind();
        match decision {
            Decision::Move { .. } => {
                self.counts.move_count = self.counts.move_count.saturating_add(1)
            }
            Decision::Click => self.counts.click = self.counts.click.saturating_add(1),
            Decision::DictationPressed => {
                self.counts.dictation_pressed = self.counts.dictation_pressed.saturating_add(1)
            }
            Decision::DictationReleased => {
                self.counts.dictation_released = self.counts.dictation_released.saturating_add(1)
            }
        }
        debug_assert_eq!(self.counts.scroll, 0);
        out.push(decision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::fixtures::{fist_at, open_palm};
    use crate::frame::HandObservation;

    fn gate() -> GestureGate {
        GestureGate::default()
    }

    fn see(g: &mut GestureGate, t: u64, lm: Option<[crate::frame::Landmark; 21]>) -> Vec<Decision> {
        g.push(
            HandObservation {
                t_ms: t,
                landmarks: lm,
                confidence: 0.95,
            },
            false,
        )
    }

    #[test]
    fn open_palm_follows_at_any_speed_without_a_wake_hold() {
        let mut g = gate();
        let first = see(&mut g, 0, Some(open_palm(0.0, 0.0)));
        assert!(matches!(first[0], Decision::Move { .. }));
        assert_eq!(g.snapshot().gate_state, TrayState::Tracking);
        // A large jump on the next frame is still a move (no "slowly" filter).
        let second = see(&mut g, 16, Some(open_palm(0.25, -0.1)));
        assert!(matches!(second.last(), Some(Decision::Move { .. })));
        assert_eq!(g.snapshot().session_counts.scroll, 0);
        assert_eq!(g.snapshot().landmark_count, 21);
    }

    #[test]
    fn absent_hand_emits_nothing() {
        let mut g = gate();
        let out = see(&mut g, 0, None);
        assert!(out.is_empty());
        assert_eq!(g.snapshot().gate_state, TrayState::Idle);
        assert_eq!(g.snapshot().session_counts.move_count, 0);
    }

    #[test]
    fn short_fist_clicks_on_release() {
        let mut g = gate();
        see(&mut g, 0, Some(open_palm(0.0, 0.0)));
        assert!(see(&mut g, 100, Some(fist_at(0.0, 0.0))).is_empty());
        let out = see(&mut g, 100 + 399, Some(open_palm(0.0, 0.0)));
        assert!(out.iter().any(|d| matches!(d, Decision::Click)));
        assert!(!out.iter().any(|d| matches!(d, Decision::DictationPressed)));
        assert_eq!(g.snapshot().session_counts.click, 1);
        assert_eq!(g.snapshot().gate_state, TrayState::RejectCooldown);
    }

    #[test]
    fn mid_hold_is_ignored_and_hand_leave_does_not_click() {
        let mut g = gate();
        see(&mut g, 0, Some(fist_at(0.0, 0.0)));
        let ignored = see(&mut g, 400, Some(open_palm(0.0, 0.0)));
        assert!(
            ignored.is_empty()
                || ignored
                    .iter()
                    .all(|d| !matches!(d, Decision::Click | Decision::DictationPressed))
        );
        assert_eq!(g.snapshot().gate_state, TrayState::RejectCooldown);
        assert_eq!(g.snapshot().session_counts.click, 0);

        let mut g = gate();
        see(&mut g, 0, Some(fist_at(0.0, 0.0)));
        let left = see(&mut g, 200, None);
        assert!(left.is_empty());
        assert_eq!(g.snapshot().session_counts.click, 0);
        assert_eq!(g.snapshot().gate_state, TrayState::Idle);
    }

    #[test]
    fn long_fist_starts_and_release_or_absence_stops_without_pointer() {
        let mut g = gate();
        see(&mut g, 0, Some(open_palm(0.0, 0.0)));
        let started = see(&mut g, 50, Some(fist_at(0.0, 0.0)));
        assert!(started.is_empty());
        let armed = see(&mut g, 50 + 800, Some(fist_at(0.02, 0.02)));
        assert_eq!(armed, vec![Decision::DictationPressed]);
        assert_eq!(g.snapshot().gate_state, TrayState::Recording);
        // Still closed, and a later open palm while recording does not move or click.
        assert!(see(&mut g, 900, Some(fist_at(0.1, 0.1))).is_empty());
        let during = see(&mut g, 1000, Some(open_palm(0.2, 0.2)));
        assert_eq!(during, vec![Decision::DictationReleased]);
        assert_eq!(g.snapshot().session_counts.move_count, 1);
        assert_eq!(g.snapshot().session_counts.click, 0);
        assert_eq!(g.snapshot().session_counts.dictation_pressed, 1);
        assert_eq!(g.snapshot().session_counts.dictation_released, 1);
        assert_eq!(g.snapshot().session_counts.scroll, 0);

        let mut g = gate();
        see(&mut g, 0, Some(fist_at(0.0, 0.0)));
        see(&mut g, 800, Some(fist_at(0.0, 0.0)));
        let left = see(&mut g, 1200, None);
        assert_eq!(left, vec![Decision::DictationReleased]);
        assert_eq!(g.snapshot().gate_state, TrayState::Idle);
        assert_eq!(g.snapshot().session_counts.click, 0);
    }

    #[test]
    fn pause_blocks_the_follow_through_move() {
        let mut g = gate();
        see(&mut g, 0, Some(fist_at(0.0, 0.0)));
        see(&mut g, 100, Some(open_palm(0.0, 0.0)));
        let during = see(&mut g, 200, Some(open_palm(0.2, 0.0)));
        assert!(during.iter().all(|d| !matches!(d, Decision::Move { .. })));
        let after = see(
            &mut g,
            100 + PROVISIONAL_MISTOUCH_PAUSE_MS,
            Some(open_palm(0.2, 0.0)),
        );
        assert!(after.iter().any(|d| matches!(d, Decision::Move { .. })));
        assert_eq!(g.snapshot().gate_state, TrayState::Tracking);
    }

    #[test]
    fn external_recording_suppresses_pointer_until_it_ends() {
        let mut g = gate();
        let blocked = g.push(
            HandObservation::with_landmarks(0, open_palm(0.0, 0.0), 0.9),
            true,
        );
        assert!(blocked.is_empty());
        assert_eq!(g.snapshot().gate_state, TrayState::Recording);
        let resumed = see(&mut g, 30, Some(open_palm(0.0, 0.0)));
        assert!(matches!(resumed[0], Decision::Move { .. }));
    }

    #[test]
    fn aegis_json_fields_use_gate_state_and_session_counts() {
        let mut g = gate();
        see(&mut g, 0, Some(open_palm(0.0, 0.0)));
        see(&mut g, 50, Some(fist_at(0.0, 0.0)));
        see(&mut g, 50 + 800, Some(fist_at(0.0, 0.0)));
        let json = serde_json::to_value(g.snapshot()).expect("snapshot json");
        assert_eq!(json["gate_state"], "recording");
        assert_eq!(json["gesture_enabled"], true);
        assert_eq!(json["last_decision"], "dictation_pressed");
        assert_eq!(json["session_counts"]["move"], 1);
        assert_eq!(json["session_counts"]["click"], 0);
        assert_eq!(json["session_counts"]["scroll"], 0);
        assert_eq!(json["session_counts"]["dictation_pressed"], 1);
        assert_eq!(json["session_counts"]["dictation_released"], 0);
        assert!(json.get("tray").is_none());
        assert!(json.get("enabled").is_none());
        assert!(json.get("counts").is_none());
        assert_eq!(TrayState::Idle.label_zh(), "安静");
        assert_eq!(TrayState::Tracking.label_zh(), "正在听手");
        assert_eq!(TrayState::Recording.label_zh(), "正在录音");
        assert_eq!(TrayState::RejectCooldown.label_zh(), "刚误触停一下");
    }

    #[test]
    fn low_confidence_is_no_hand() {
        let mut g = gate();
        let out = g.push(
            HandObservation::with_landmarks(0, open_palm(0.0, 0.0), 0.2),
            false,
        );
        assert!(out.is_empty());
        assert_eq!(g.snapshot().gate_state, TrayState::Idle);
    }

    /// G-07 skeleton: 20 synthetic short-vs-long fist trials, no camera.
    /// Thresholds stay provisional; wrong count must be ≤1 for the batch to pass.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum G07Kind {
        Short,
        Long,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)] // kept for fail diagnostics / Aegis
    struct G07Trial {
        index: usize,
        kind: G07Kind,
        hold_ms: u64,
        got_click: bool,
        got_dictation_pressed: bool,
        wrong: bool,
    }

    fn run_short_trial(hold_ms: u64) -> (bool, bool) {
        let mut g = gate();
        let _ = see(&mut g, 0, Some(open_palm(0.0, 0.0)));
        let _ = see(&mut g, 10, Some(fist_at(0.0, 0.0)));
        let out = see(&mut g, 10 + hold_ms, Some(open_palm(0.0, 0.0)));
        let got_click = out.iter().any(|d| matches!(d, Decision::Click));
        let got_dictation = out
            .iter()
            .any(|d| matches!(d, Decision::DictationPressed));
        (got_click, got_dictation)
    }

    fn run_long_trial(hold_ms: u64) -> (bool, bool) {
        let mut g = gate();
        let _ = see(&mut g, 0, Some(open_palm(0.0, 0.0)));
        let _ = see(&mut g, 10, Some(fist_at(0.0, 0.0)));
        // Still closed at long threshold → pressed; then release.
        let armed = see(&mut g, 10 + hold_ms, Some(fist_at(0.01, 0.01)));
        let released = see(&mut g, 10 + hold_ms + 50, Some(open_palm(0.0, 0.0)));
        let got_click = armed
            .iter()
            .chain(released.iter())
            .any(|d| matches!(d, Decision::Click));
        let got_dictation = armed
            .iter()
            .any(|d| matches!(d, Decision::DictationPressed));
        (got_click, got_dictation)
    }

    /// Drive N short/long sequences through [`GestureGate`] and collect mistakes.
    fn g07_batch(n: usize) -> (usize, Vec<usize>, Vec<G07Trial>) {
        assert!(n >= 2 && n % 2 == 0, "G-07 batch expects even N≥2");
        let short_holds: [u64; 10] = [50, 100, 150, 200, 250, 300, 350, 380, 390, 399];
        let long_holds: [u64; 10] = [
            PROVISIONAL_LONG_FIST_MIN_MS,
            PROVISIONAL_LONG_FIST_MIN_MS + 20,
            PROVISIONAL_LONG_FIST_MIN_MS + 40,
            PROVISIONAL_LONG_FIST_MIN_MS + 80,
            PROVISIONAL_LONG_FIST_MIN_MS + 120,
            PROVISIONAL_LONG_FIST_MIN_MS + 160,
            PROVISIONAL_LONG_FIST_MIN_MS + 200,
            PROVISIONAL_LONG_FIST_MIN_MS + 250,
            PROVISIONAL_LONG_FIST_MIN_MS + 300,
            PROVISIONAL_LONG_FIST_MIN_MS + 400,
        ];
        let mut trials = Vec::with_capacity(n);
        let mut fail_idx = Vec::new();
        for i in 0..n {
            let (kind, hold_ms) = if i % 2 == 0 {
                (G07Kind::Short, short_holds[(i / 2) % short_holds.len()])
            } else {
                (G07Kind::Long, long_holds[(i / 2) % long_holds.len()])
            };
            let (got_click, got_dictation) = match kind {
                G07Kind::Short => run_short_trial(hold_ms),
                G07Kind::Long => run_long_trial(hold_ms),
            };
            let wrong = match kind {
                // Short must click and must not start dictation.
                G07Kind::Short => !got_click || got_dictation,
                // Long must press dictation and must not click.
                G07Kind::Long => !got_dictation || got_click,
            };
            if wrong {
                fail_idx.push(i);
            }
            trials.push(G07Trial {
                index: i,
                kind,
                hold_ms,
                got_click,
                got_dictation_pressed: got_dictation,
                wrong,
            });
        }
        (fail_idx.len(), fail_idx, trials)
    }

    #[test]
    fn g07_short_vs_long_batch_n20_wrong_at_most_one() {
        const N: usize = 20;
        const MAX_WRONG: usize = 1;
        let (wrong, fail_idx, trials) = g07_batch(N);
        let short_ok = trials
            .iter()
            .filter(|t| t.kind == G07Kind::Short && !t.wrong)
            .count();
        let long_ok = trials
            .iter()
            .filter(|t| t.kind == G07Kind::Long && !t.wrong)
            .count();
        assert!(
            wrong <= MAX_WRONG,
            "G-07 batch failed: wrong={wrong} (max {MAX_WRONG}); fail_indices={fail_idx:?}; short_ok={short_ok}/10 long_ok={long_ok}/10; trials={trials:?}"
        );
        // Sanity: both kinds exercised.
        assert_eq!(trials.iter().filter(|t| t.kind == G07Kind::Short).count(), 10);
        assert_eq!(trials.iter().filter(|t| t.kind == G07Kind::Long).count(), 10);
        assert!(
            trials.iter().all(|t| t.hold_ms < PROVISIONAL_SHORT_FIST_MAX_MS
                || t.hold_ms >= PROVISIONAL_LONG_FIST_MIN_MS),
            "holds must stay in short or long band (provisional)"
        );
    }

    /// G-09 gate layer: while OpenLess owns insertion/recording (`external_busy`),
    /// open-palm follow and short-fist click must not fire (禁点禁移); scroll stays 0.
    #[test]
    fn g09_external_busy_blocks_move_and_click() {
        let mut g = gate();
        // Wave: no move while busy.
        let waved = g.push(
            HandObservation::with_landmarks(0, open_palm(0.0, 0.0), 0.95),
            true,
        );
        assert!(waved.is_empty());
        assert_eq!(g.snapshot().gate_state, TrayState::Recording);
        assert_eq!(g.snapshot().session_counts.move_count, 0);
        assert_eq!(g.snapshot().session_counts.click, 0);
        assert_eq!(g.snapshot().session_counts.scroll, 0);

        // Short fist attempt while still busy: still no click / dictation.
        let fist = g.push(
            HandObservation::with_landmarks(20, fist_at(0.0, 0.0), 0.95),
            true,
        );
        assert!(fist.is_empty());
        let release = g.push(
            HandObservation::with_landmarks(20 + 200, open_palm(0.1, 0.0), 0.95),
            true,
        );
        assert!(release.is_empty());
        let snap = g.snapshot();
        assert_eq!(snap.session_counts.click, 0);
        assert_eq!(snap.session_counts.move_count, 0);
        assert_eq!(snap.session_counts.scroll, 0);
        assert_eq!(snap.session_counts.dictation_pressed, 0);
        assert_eq!(snap.last_decision, DecisionKind::None);

        // Busy cleared → follow resumes (not asserting product pass for host insert).
        let resumed = see(&mut g, 300, Some(open_palm(0.0, 0.0)));
        assert!(resumed.iter().any(|d| matches!(d, Decision::Move { .. })));
    }


    #[test]
    fn missing_tracker_shows_not_installed_not_idle() {
        let t = TrayState::effective(TrayState::Idle, Some("libmediapipe not found"), false);
        assert_eq!(t, TrayState::NotInstalled);
        assert_eq!(t.label_zh(), "手势没装好");
        assert_ne!(t.label_zh(), TrayState::Idle.label_zh());
        assert_eq!(TrayState::effective(TrayState::Idle, None, false), TrayState::Idle);
    }

    #[test]
    fn accessibility_denied_shows_needs_accessibility() {
        let t = TrayState::effective(TrayState::Tracking, None, true);
        assert_eq!(t, TrayState::NeedsAccessibility);
        assert_eq!(t.label_zh(), "需要辅助功能");
        // Tracker error still wins over accessibility.
        let t2 = TrayState::effective(TrayState::Tracking, Some("no camera"), true);
        assert_eq!(t2, TrayState::NotInstalled);
    }
}
