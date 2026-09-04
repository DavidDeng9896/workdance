use workdance_core::VisionTier;

/// Max fist hold that still counts as G03 click.
pub const SHORT_FIST_MAX: u64 = 300;

/// Openness below this ⇒ fist; at/above OPEN_PALM ⇒ open palm.
const FIST_MAX_OPENNESS: f32 = 0.35;
const OPEN_PALM_MIN_OPENNESS: f32 = 0.55;

/// Normalized dy (frame space) for G05 swipe-down (raw sample delta).
const SWIPE_DOWN_DY: f32 = 0.22;
/// Min |delta| in norm space before move/scroll after dead-zone.
const MIN_DELTA: f32 = 0.004;

/// One hand sample in normalized camera/selfie space (x,y in 0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandSample {
    pub present: bool,
    pub confidence: f32,
    pub x: f32,
    pub y: f32,
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
            openness: 0.95,
        }
    }

    pub fn fist(x: f32, y: f32) -> Self {
        Self {
            present: true,
            confidence: 0.95,
            x,
            y,
            openness: 0.1,
        }
    }

    pub fn absent() -> Self {
        Self {
            present: false,
            confidence: 0.0,
            x: 0.0,
            y: 0.0,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FistPhase {
    Idle,
    /// Fist started at `since_ms`; click pending until release or timeout.
    Holding { since_ms: u64 },
    /// Held past SHORT_FIST_MAX — G04 scroll mode until open palm.
    LongHold,
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
    smooth_x: crate::smoother::ExpSmoother,
    smooth_y: crate::smoother::ExpSmoother,
    fist: FistPhase,
    swipe_armed: bool,
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
            smooth_x: crate::smoother::ExpSmoother::new(alpha),
            smooth_y: crate::smoother::ExpSmoother::new(alpha),
            fist: FistPhase::Idle,
            swipe_armed: true,
        }
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
    }

    pub fn on_sample(&mut self, now_ms: u64, sample: HandSample) -> Vec<InjectCommand> {
        if self.tier != VisionTier::Active {
            return Vec::new();
        }
        if !sample.present || sample.confidence < workdance_core::MIN_PALM_CONFIDENCE {
            self.reset_motion();
            return Vec::new();
        }

        let sx = self.smooth_x.push(sample.x);
        let sy = self.smooth_y.push(sample.y);
        let mut out = Vec::new();

        // Keep raw previous for swipe (smoothing would shrink fast flicks).
        let raw_prev = self.last_raw;
        self.last_raw = Some((sample.x, sample.y));

        // --- Fist state machine (G03 / G04) ---
        match self.fist {
            FistPhase::Idle => {
                if sample.is_fist() {
                    self.fist = FistPhase::Holding { since_ms: now_ms };
                }
            }
            FistPhase::Holding { since_ms } => {
                if sample.is_fist() {
                    if now_ms.saturating_sub(since_ms) >= SHORT_FIST_MAX {
                        self.fist = FistPhase::LongHold;
                    }
                } else if sample.is_open_palm() {
                    let held = now_ms.saturating_sub(since_ms);
                    if held < SHORT_FIST_MAX {
                        out.push(InjectCommand::ClickLeft);
                    }
                    self.fist = FistPhase::Idle;
                } else {
                    self.fist = FistPhase::Idle;
                }
            }
            FistPhase::LongHold => {
                if sample.is_open_palm() {
                    self.fist = FistPhase::Idle;
                }
            }
        }

        // --- Motion: G02 move vs G04 scroll vs G05 swipe ---
        if let Some((px, py)) = self.last_pos {
            let mut dx = sx - px;
            let mut dy = sy - py;
            if dx.abs() < self.dead_zone {
                dx = 0.0;
            }
            if dy.abs() < self.dead_zone {
                dy = 0.0;
            }

            let scrolling = matches!(self.fist, FistPhase::LongHold) && sample.is_fist();
            let moving = sample.is_open_palm() && matches!(self.fist, FistPhase::Idle);

            if scrolling {
                if dy.abs() >= MIN_DELTA {
                    let scroll = (dy * 800.0 * self.sensitivity).round() as i32;
                    if scroll != 0 {
                        out.push(InjectCommand::Scroll { dy: scroll });
                    }
                }
            } else if moving {
                if let Some((rx0, ry0)) = raw_prev {
                    let raw_dy = sample.y - ry0;
                    let raw_dx = sample.x - rx0;
                    if self.swipe_armed && raw_dy >= SWIPE_DOWN_DY && raw_dx.abs() < 0.12 {
                        out.push(InjectCommand::KeyEscape);
                        self.swipe_armed = false;
                    }
                }
                if !out.iter().any(|c| matches!(c, InjectCommand::KeyEscape))
                    && (dx.abs() >= MIN_DELTA || dy.abs() >= MIN_DELTA)
                {
                    let screen_x = (sx * self.screen_w as f32)
                        .clamp(0.0, (self.screen_w - 1) as f32)
                        .round() as i32;
                    // Mirror X for selfie front camera.
                    let screen_x = self.screen_w - 1 - screen_x;
                    let screen_y = (sy * self.screen_h as f32)
                        .clamp(0.0, (self.screen_h - 1) as f32)
                        .round() as i32;
                    out.push(InjectCommand::MoveAbs {
                        x: screen_x,
                        y: screen_y,
                    });
                }
            }
        }

        if sample.is_open_palm() && !self.swipe_armed && sample.y < 0.35 {
            self.swipe_armed = true;
        }

        self.last_pos = Some((sx, sy));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            out.iter().any(|c| matches!(c, InjectCommand::ClickLeft)),
            "expected ClickLeft for short fist, got {out:?}"
        );
    }

    #[test]
    fn fist_held_over_300ms_is_not_a_click() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let mid = e.on_sample(400, HandSample::fist(0.5, 0.5));
        assert!(!mid.iter().any(|c| matches!(c, InjectCommand::ClickLeft)));
        let out = e.on_sample(450, HandSample::open_palm(0.5, 0.5));
        assert!(
            !out.iter().any(|c| matches!(c, InjectCommand::ClickLeft)),
            "long fist must not click, got {out:?}"
        );
    }

    #[test]
    fn sleep_tier_suppresses_all_inject() {
        let mut e = engine();
        e.set_tier(VisionTier::Sleep);
        let out = e.on_sample(0, HandSample::open_palm(0.5, 0.5));
        assert!(out.is_empty(), "sleep must not move cursor: {out:?}");
        let _ = e.on_sample(10, HandSample::fist(0.5, 0.5));
        let out = e.on_sample(100, HandSample::open_palm(0.5, 0.5));
        assert!(out.is_empty(), "sleep must not click: {out:?}");
    }

    #[test]
    fn open_palm_translate_moves_not_scrolls() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let _ = e.on_sample(0, HandSample::open_palm(0.4, 0.4));
        let out = e.on_sample(32, HandSample::open_palm(0.55, 0.4));
        assert!(
            out.iter().any(|c| matches!(
                c,
                InjectCommand::MoveAbs { .. } | InjectCommand::MoveDelta { .. }
            )),
            "G02 should move, got {out:?}"
        );
        assert!(!out.iter().any(|c| matches!(c, InjectCommand::Scroll { .. })));
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
            out.iter().any(|c| matches!(c, InjectCommand::Scroll { .. })),
            "G04 fist+translate should scroll, got {out:?}"
        );
        assert!(!out.iter().any(|c| matches!(
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
            out.iter().any(|c| matches!(c, InjectCommand::KeyEscape)),
            "G05 swipe down → Esc, got {out:?}"
        );
    }
}
