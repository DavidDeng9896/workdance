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
    Quiet,
    Tracking,
    Recording,
    MisTouchPause,
}

impl TrayState {
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Quiet => "安静",
            Self::Tracking => "跟踪",
            Self::Recording => "听写",
            Self::MisTouchPause => "误触暂停",
        }
    }

    pub fn label_en(self) -> &'static str {
        match self {
            Self::Quiet => "Quiet",
            Self::Tracking => "Tracking",
            Self::Recording => "Recording",
            Self::MisTouchPause => "Mis-touch pause",
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
#[serde(rename_all = "camelCase")]
pub struct EventCounts {
    pub move_events: u64,
    pub click: u64,
    /// Reserved. Palm-flick scroll is not implemented; this stays 0.
    pub scroll: u64,
    pub dictation_edge: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateSnapshot {
    pub enabled: bool,
    pub tray: TrayState,
    pub pose: Pose,
    pub recording: bool,
    pub landmark_count: u8,
    pub fist_held_ms: u64,
    pub mis_touch_remaining_ms: u64,
    pub last_decision: DecisionKind,
    pub counts: EventCounts,
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
            tray: TrayState::Quiet,
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
            enabled: self.enabled,
            tray: self.tray,
            pose: self.pose,
            recording: self.recording,
            landmark_count: self.landmark_count,
            fist_held_ms: self.fist_held_ms,
            mis_touch_remaining_ms: remaining,
            last_decision: self.last_decision,
            counts: self.counts,
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
        self.tray = TrayState::Quiet;
        out
    }

    /// OpenLess already owns a dictation this gate did not start.
    /// Drop the owned-recording flag without emitting another edge.
    pub fn abandon_recording(&mut self) {
        self.recording = false;
        self.fist_started_ms = None;
        self.fist_held_ms = 0;
        if self.tray == TrayState::Recording {
            self.tray = TrayState::Quiet;
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
            self.tray = TrayState::Quiet;
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
                    TrayState::MisTouchPause
                } else {
                    TrayState::Quiet
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
                    self.tray = TrayState::MisTouchPause;
                }
            }
            Pose::OpenPalm => {
                self.on_fist_release(obs.t_ms, out);
                if self.in_pause(obs.t_ms) {
                    self.tray = TrayState::MisTouchPause;
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
                    self.tray = TrayState::MisTouchPause;
                } else if self.tray != TrayState::MisTouchPause {
                    self.tray = TrayState::Quiet;
                } else if self.mis_touch_until_ms.is_none() {
                    self.tray = TrayState::Quiet;
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
        self.tray = TrayState::MisTouchPause;
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
                self.counts.move_events = self.counts.move_events.saturating_add(1)
            }
            Decision::Click => self.counts.click = self.counts.click.saturating_add(1),
            Decision::DictationPressed | Decision::DictationReleased => {
                self.counts.dictation_edge = self.counts.dictation_edge.saturating_add(1);
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
        assert_eq!(g.snapshot().tray, TrayState::Tracking);
        // A large jump on the next frame is still a move (no "slowly" filter).
        let second = see(&mut g, 16, Some(open_palm(0.25, -0.1)));
        assert!(matches!(second.last(), Some(Decision::Move { .. })));
        assert_eq!(g.snapshot().counts.scroll, 0);
        assert_eq!(g.snapshot().landmark_count, 21);
    }

    #[test]
    fn absent_hand_emits_nothing() {
        let mut g = gate();
        let out = see(&mut g, 0, None);
        assert!(out.is_empty());
        assert_eq!(g.snapshot().tray, TrayState::Quiet);
        assert_eq!(g.snapshot().counts.move_events, 0);
    }

    #[test]
    fn short_fist_clicks_on_release() {
        let mut g = gate();
        see(&mut g, 0, Some(open_palm(0.0, 0.0)));
        assert!(see(&mut g, 100, Some(fist_at(0.0, 0.0))).is_empty());
        let out = see(&mut g, 100 + 399, Some(open_palm(0.0, 0.0)));
        assert!(out.iter().any(|d| matches!(d, Decision::Click)));
        assert!(!out.iter().any(|d| matches!(d, Decision::DictationPressed)));
        assert_eq!(g.snapshot().counts.click, 1);
        assert_eq!(g.snapshot().tray, TrayState::MisTouchPause);
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
        assert_eq!(g.snapshot().tray, TrayState::MisTouchPause);
        assert_eq!(g.snapshot().counts.click, 0);

        let mut g = gate();
        see(&mut g, 0, Some(fist_at(0.0, 0.0)));
        let left = see(&mut g, 200, None);
        assert!(left.is_empty());
        assert_eq!(g.snapshot().counts.click, 0);
        assert_eq!(g.snapshot().tray, TrayState::Quiet);
    }

    #[test]
    fn long_fist_starts_and_release_or_absence_stops_without_pointer() {
        let mut g = gate();
        see(&mut g, 0, Some(open_palm(0.0, 0.0)));
        let started = see(&mut g, 50, Some(fist_at(0.0, 0.0)));
        assert!(started.is_empty());
        let armed = see(&mut g, 50 + 800, Some(fist_at(0.02, 0.02)));
        assert_eq!(armed, vec![Decision::DictationPressed]);
        assert_eq!(g.snapshot().tray, TrayState::Recording);
        // Still closed, and a later open palm while recording does not move or click.
        assert!(see(&mut g, 900, Some(fist_at(0.1, 0.1))).is_empty());
        let during = see(&mut g, 1000, Some(open_palm(0.2, 0.2)));
        assert_eq!(during, vec![Decision::DictationReleased]);
        assert_eq!(g.snapshot().counts.move_events, 1);
        assert_eq!(g.snapshot().counts.click, 0);
        assert_eq!(g.snapshot().counts.dictation_edge, 2);
        assert_eq!(g.snapshot().counts.scroll, 0);

        let mut g = gate();
        see(&mut g, 0, Some(fist_at(0.0, 0.0)));
        see(&mut g, 800, Some(fist_at(0.0, 0.0)));
        let left = see(&mut g, 1200, None);
        assert_eq!(left, vec![Decision::DictationReleased]);
        assert_eq!(g.snapshot().tray, TrayState::Quiet);
        assert_eq!(g.snapshot().counts.click, 0);
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
        assert_eq!(g.snapshot().tray, TrayState::Tracking);
    }

    #[test]
    fn external_recording_suppresses_pointer_until_it_ends() {
        let mut g = gate();
        let blocked = g.push(
            HandObservation::with_landmarks(0, open_palm(0.0, 0.0), 0.9),
            true,
        );
        assert!(blocked.is_empty());
        assert_eq!(g.snapshot().tray, TrayState::Recording);
        let resumed = see(&mut g, 30, Some(open_palm(0.0, 0.0)));
        assert!(matches!(resumed[0], Decision::Move { .. }));
    }

    #[test]
    fn low_confidence_is_no_hand() {
        let mut g = gate();
        let out = g.push(
            HandObservation::with_landmarks(0, open_palm(0.0, 0.0), 0.2),
            false,
        );
        assert!(out.is_empty());
        assert_eq!(g.snapshot().tray, TrayState::Quiet);
    }
}
