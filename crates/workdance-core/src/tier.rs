use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Dual-tier vision cadence (locked spec §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisionTier {
    /// 3–5 FPS palm presence only.
    Sleep,
    /// 25–30 FPS after stable palm wake.
    Active,
}

/// One frame of palm-presence evidence from a detector backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PalmObservation {
    /// True when a palm/hand is considered present in frame.
    pub present: bool,
    /// Detector confidence in \[0, 1\]. Valid palm requires ≥ [`MIN_PALM_CONFIDENCE`].
    pub confidence: f32,
}

impl PalmObservation {
    pub fn valid_palm(self) -> bool {
        self.present && self.confidence >= MIN_PALM_CONFIDENCE
    }
}

/// Spec thresholds (2026-09-04 locked).
pub const MIN_PALM_CONFIDENCE: f32 = 0.6;
pub const WAKE_HOLD: Duration = Duration::from_millis(500);
pub const SLEEP_HOLD: Duration = Duration::from_millis(1200);

/// Midpoints of the allowed FPS bands.
pub const SLEEP_FPS: f32 = 4.0;
pub const ACTIVE_FPS: f32 = 27.5;

/// Pure dual-tier scheduler: feed observations with explicit timestamps (no hardware).
#[derive(Debug, Clone)]
pub struct DualTierMachine {
    tier: VisionTier,
    /// First instant of the current contiguous valid-palm streak (wake ramp).
    palm_streak_start: Option<Instant>,
    /// Last instant a valid palm was observed (sleep ramp).
    last_valid_palm: Option<Instant>,
}

impl Default for DualTierMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl DualTierMachine {
    pub fn new() -> Self {
        Self {
            tier: VisionTier::Sleep,
            palm_streak_start: None,
            last_valid_palm: None,
        }
    }

    pub fn tier(&self) -> VisionTier {
        self.tier
    }

    pub fn target_fps(&self) -> f32 {
        match self.tier {
            VisionTier::Sleep => SLEEP_FPS,
            VisionTier::Active => ACTIVE_FPS,
        }
    }

    /// Ingest one observation at `now`. Returns the tier after this update.
    pub fn observe(&mut self, now: Instant, obs: PalmObservation) -> VisionTier {
        if obs.valid_palm() {
            self.last_valid_palm = Some(now);
            if self.palm_streak_start.is_none() {
                self.palm_streak_start = Some(now);
            }
            if let Some(start) = self.palm_streak_start {
                if now.saturating_duration_since(start) >= WAKE_HOLD {
                    self.tier = VisionTier::Active;
                }
            }
        } else {
            // Break contiguous wake streak.
            self.palm_streak_start = None;
            if self.tier == VisionTier::Active {
                if let Some(last) = self.last_valid_palm {
                    if now.saturating_duration_since(last) >= SLEEP_HOLD {
                        self.tier = VisionTier::Sleep;
                        self.last_valid_palm = None;
                    }
                }
            }
        }
        self.tier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn palm(conf: f32) -> PalmObservation {
        PalmObservation {
            present: true,
            confidence: conf,
        }
    }

    fn no_palm() -> PalmObservation {
        PalmObservation {
            present: false,
            confidence: 0.0,
        }
    }

    #[test]
    fn starts_in_sleep_at_low_fps() {
        let m = DualTierMachine::new();
        assert_eq!(m.tier(), VisionTier::Sleep);
        assert!((m.target_fps() - SLEEP_FPS).abs() < f32::EPSILON);
    }

    #[test]
    fn conf_below_threshold_does_not_wake() {
        let mut m = DualTierMachine::new();
        let t = t0();
        // 0.59 for a full second — still sleep.
        assert_eq!(m.observe(t, palm(0.59)), VisionTier::Sleep);
        assert_eq!(
            m.observe(t + Duration::from_millis(600), palm(0.59)),
            VisionTier::Sleep
        );
    }

    #[test]
    fn wakes_only_after_half_second_stable_palm() {
        let mut m = DualTierMachine::new();
        let t = t0();
        assert_eq!(m.observe(t, palm(0.9)), VisionTier::Sleep);
        assert_eq!(
            m.observe(t + Duration::from_millis(499), palm(0.9)),
            VisionTier::Sleep
        );
        assert_eq!(
            m.observe(t + Duration::from_millis(500), palm(0.9)),
            VisionTier::Active
        );
        assert!((m.target_fps() - ACTIVE_FPS).abs() < f32::EPSILON);
    }

    #[test]
    fn brief_drop_resets_wake_timer() {
        let mut m = DualTierMachine::new();
        let t = t0();
        m.observe(t, palm(0.9));
        m.observe(t + Duration::from_millis(400), palm(0.9));
        // Drop breaks streak.
        m.observe(t + Duration::from_millis(410), no_palm());
        // New streak starts here — 400ms later still asleep.
        assert_eq!(
            m.observe(t + Duration::from_millis(810), palm(0.9)),
            VisionTier::Sleep
        );
        assert_eq!(
            m.observe(t + Duration::from_millis(1200), palm(0.9)),
            VisionTier::Sleep
        );
        assert_eq!(
            m.observe(t + Duration::from_millis(1310), palm(0.9)),
            VisionTier::Active
        );
    }

    #[test]
    fn falls_asleep_after_1_2s_without_valid_palm() {
        let mut m = DualTierMachine::new();
        let t = t0();
        m.observe(t, palm(0.95));
        assert_eq!(
            m.observe(t + Duration::from_millis(500), palm(0.95)),
            VisionTier::Active
        );
        // Lost palm — still active until 1.2s.
        assert_eq!(
            m.observe(t + Duration::from_millis(500 + 1199), no_palm()),
            VisionTier::Active
        );
        assert_eq!(
            m.observe(t + Duration::from_millis(500 + 1200), no_palm()),
            VisionTier::Sleep
        );
    }

    #[test]
    fn low_conf_counts_as_no_valid_palm_for_sleep() {
        let mut m = DualTierMachine::new();
        let t = t0();
        m.observe(t, palm(0.8));
        m.observe(t + Duration::from_millis(500), palm(0.8));
        assert_eq!(m.tier(), VisionTier::Active);
        // Present but conf 0.5 — invalid.
        assert_eq!(
            m.observe(t + Duration::from_millis(500 + 1200), palm(0.5)),
            VisionTier::Sleep
        );
    }
}
