//! WP-M2: MediaPipe 21-landmark → [`HandSample`] for G02–G05.
//!
//! Index tip drives the cursor; finger-curl heuristics drive fist/openness;
//! palm center motion drives G05 swipe. Presence-only frames (no landmarks)
//! return `None` so callers keep the stub/scripted path.

use workdance_core::{HandFrame, HandLandmark};

use crate::engine::HandSample;

/// MediaPipe Hands landmark indices.
pub const WRIST: usize = 0;
pub const INDEX_MCP: usize = 5;
pub const INDEX_PIP: usize = 6;
pub const INDEX_TIP: usize = 8;
pub const MIDDLE_MCP: usize = 9;
pub const MIDDLE_PIP: usize = 10;
pub const MIDDLE_TIP: usize = 12;
pub const RING_MCP: usize = 13;
pub const RING_PIP: usize = 14;
pub const RING_TIP: usize = 16;
pub const PINKY_MCP: usize = 17;
pub const PINKY_PIP: usize = 18;
pub const PINKY_TIP: usize = 20;

/// Map a [`HandFrame`] to a gesture sample when 21 landmarks are present.
///
/// - Absent hand → [`HandSample::absent`]
/// - Present but no landmarks → `None` (caller keeps stub choreography)
/// - Present + landmarks → tip / palm / openness from geometry
pub fn hand_frame_to_sample(frame: &HandFrame) -> Option<HandSample> {
    if !frame.present {
        return Some(HandSample::absent());
    }
    let landmarks = frame.landmarks.as_ref()?;
    Some(sample_from_landmarks(landmarks, frame.confidence))
}

/// Build a sample from a synthetic or detector landmark set.
pub fn sample_from_landmarks(lm: &[HandLandmark; 21], confidence: f32) -> HandSample {
    let tip = lm[INDEX_TIP];
    let palm = palm_center(lm);
    let openness = finger_openness(lm);
    HandSample {
        present: true,
        confidence: confidence.clamp(0.0, 1.0),
        x: tip.x.clamp(0.0, 1.0),
        y: tip.y.clamp(0.0, 1.0),
        palm_x: palm.0.clamp(0.0, 1.0),
        palm_y: palm.1.clamp(0.0, 1.0),
        openness: openness.clamp(0.0, 1.0),
    }
}

fn dist(a: HandLandmark, b: HandLandmark) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

/// Palm = mean of wrist + four finger MCPs.
fn palm_center(lm: &[HandLandmark; 21]) -> (f32, f32) {
    let pts = [
        lm[WRIST],
        lm[INDEX_MCP],
        lm[MIDDLE_MCP],
        lm[RING_MCP],
        lm[PINKY_MCP],
    ];
    let n = pts.len() as f32;
    let x = pts.iter().map(|p| p.x).sum::<f32>() / n;
    let y = pts.iter().map(|p| p.y).sum::<f32>() / n;
    (x, y)
}

/// Per-finger extension in \[0, 1\] (0 = curled, 1 = extended), then averaged.
fn finger_extension(lm: &[HandLandmark; 21], tip: usize, pip: usize, mcp: usize) -> f32 {
    let tip_mcp = dist(lm[tip], lm[mcp]);
    let pip_mcp = dist(lm[pip], lm[mcp]).max(1e-5);
    // Fully extended tip≈2×PIP–MCP length; curled tip near MCP → ~0.
    (tip_mcp / (2.0 * pip_mcp)).clamp(0.0, 1.0)
}

fn finger_openness(lm: &[HandLandmark; 21]) -> f32 {
    let fingers = [
        finger_extension(lm, INDEX_TIP, INDEX_PIP, INDEX_MCP),
        finger_extension(lm, MIDDLE_TIP, MIDDLE_PIP, MIDDLE_MCP),
        finger_extension(lm, RING_TIP, RING_PIP, RING_MCP),
        finger_extension(lm, PINKY_TIP, PINKY_PIP, PINKY_MCP),
    ];
    fingers.iter().sum::<f32>() / fingers.len() as f32
}

/// Test / fixture helpers: build a flat open-palm or fist landmark layout.
#[cfg(test)]
mod fixtures {
    use super::*;

    fn lm(x: f32, y: f32) -> HandLandmark {
        HandLandmark { x, y, z: 0.0 }
    }

    fn blank() -> [HandLandmark; 21] {
        [lm(0.5, 0.5); 21]
    }

    /// Open palm centered near `(cx, cy)` with index tip at `(tip_x, tip_y)`.
    pub fn open_palm_landmarks(cx: f32, cy: f32, tip_x: f32, tip_y: f32) -> [HandLandmark; 21] {
        let mut out = blank();
        out[WRIST] = lm(cx, cy + 0.12);
        out[INDEX_MCP] = lm(cx - 0.04, cy);
        out[INDEX_PIP] = lm(cx - 0.04, cy - 0.06);
        out[INDEX_TIP] = lm(tip_x, tip_y);
        out[MIDDLE_MCP] = lm(cx, cy);
        out[MIDDLE_PIP] = lm(cx, cy - 0.07);
        out[MIDDLE_TIP] = lm(cx, cy - 0.14);
        out[RING_MCP] = lm(cx + 0.04, cy);
        out[RING_PIP] = lm(cx + 0.04, cy - 0.06);
        out[RING_TIP] = lm(cx + 0.04, cy - 0.12);
        out[PINKY_MCP] = lm(cx + 0.07, cy + 0.01);
        out[PINKY_PIP] = lm(cx + 0.08, cy - 0.04);
        out[PINKY_TIP] = lm(cx + 0.09, cy - 0.09);
        out
    }

    /// Fist: tips curled near / past MCP so tip–MCP ≪ PIP–MCP.
    pub fn fist_landmarks(cx: f32, cy: f32) -> [HandLandmark; 21] {
        let mut out = blank();
        out[WRIST] = lm(cx, cy + 0.10);
        for &(mcp, pip, tip, dx) in &[
            (INDEX_MCP, INDEX_PIP, INDEX_TIP, -0.03f32),
            (MIDDLE_MCP, MIDDLE_PIP, MIDDLE_TIP, 0.0),
            (RING_MCP, RING_PIP, RING_TIP, 0.03),
            (PINKY_MCP, PINKY_PIP, PINKY_TIP, 0.055),
        ] {
            out[mcp] = lm(cx + dx, cy);
            // PIP still extended toward the finger axis…
            out[pip] = lm(cx + dx, cy - 0.05);
            // …but tip curls back beside the MCP (low tip–MCP / PIP–MCP).
            out[tip] = lm(cx + dx * 0.3, cy + 0.015);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::fixtures::{fist_landmarks, open_palm_landmarks};
    use crate::engine::{GestureEngine, InjectCommand};
    use workdance_core::VisionTier;

    fn engine() -> GestureEngine {
        GestureEngine::new(1920, 1080, 0.7, 0.02)
    }

    fn frame_with(lm: [HandLandmark; 21]) -> HandFrame {
        HandFrame {
            present: true,
            confidence: 0.95,
            landmarks: Some(lm),
        }
    }

    #[test]
    fn presence_only_returns_none_for_stub_fallback() {
        let frame = HandFrame::presence_only(true, 0.9);
        assert!(hand_frame_to_sample(&frame).is_none());
    }

    #[test]
    fn full_frame_maps_via_hand_frame_to_sample() {
        let s = hand_frame_to_sample(&frame_with(open_palm_landmarks(0.5, 0.5, 0.4, 0.3)))
            .expect("landmarks present");
        assert!(s.present);
        assert!((s.x - 0.4).abs() < 1e-4);
    }

    #[test]
    fn absent_frame_maps_to_absent_sample() {
        let s = hand_frame_to_sample(&HandFrame::absent()).expect("absent maps");
        assert!(!s.present);
    }

    #[test]
    fn open_landmarks_use_index_tip_and_high_openness() {
        let lm = open_palm_landmarks(0.5, 0.5, 0.42, 0.28);
        let s = sample_from_landmarks(&lm, 0.92);
        assert!((s.x - 0.42).abs() < 1e-4);
        assert!((s.y - 0.28).abs() < 1e-4);
        assert!(s.openness >= 0.55, "open palm openness={}", s.openness);
        assert!(s.is_open_palm());
        // Palm should sit near MCP cluster, not at the tip.
        assert!((s.palm_x - 0.5).abs() < 0.08);
        assert!((s.palm_y - 0.5).abs() < 0.1);
    }

    #[test]
    fn fist_landmarks_low_openness() {
        let lm = fist_landmarks(0.5, 0.5);
        let s = sample_from_landmarks(&lm, 0.9);
        assert!(s.openness < 0.35, "fist openness={}", s.openness);
        assert!(s.is_fist());
    }

    #[test]
    fn landmark_move_emits_cursor_via_index_tip() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let a = sample_from_landmarks(&open_palm_landmarks(0.4, 0.4, 0.35, 0.35), 0.95);
        let b = sample_from_landmarks(&open_palm_landmarks(0.55, 0.4, 0.55, 0.35), 0.95);
        let _ = e.on_sample(0, a);
        let out = e.on_sample(32, b);
        assert!(
            out.commands.iter().any(|c| matches!(
                c,
                InjectCommand::MoveAbs { .. } | InjectCommand::MoveDelta { .. }
            )),
            "G02 landmark move, got {:?}",
            out.commands
        );
    }

    #[test]
    fn landmark_short_fist_emits_click() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let open = sample_from_landmarks(&open_palm_landmarks(0.5, 0.5, 0.48, 0.35), 0.95);
        let fist = sample_from_landmarks(&fist_landmarks(0.5, 0.5), 0.95);
        let _ = e.on_sample(0, open);
        let _ = e.on_sample(10, fist);
        let out = e.on_sample(200, open);
        assert!(
            out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::ClickLeft)),
            "G03 landmark click, got {:?}",
            out.commands
        );
    }

    #[test]
    fn landmark_fist_translate_scrolls() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        let open = sample_from_landmarks(&open_palm_landmarks(0.5, 0.5, 0.5, 0.35), 0.95);
        let fist_a = sample_from_landmarks(&fist_landmarks(0.5, 0.45), 0.95);
        let fist_b = sample_from_landmarks(&fist_landmarks(0.5, 0.62), 0.95);
        let _ = e.on_sample(0, open);
        let _ = e.on_sample(10, fist_a);
        let _ = e.on_sample(400, fist_a);
        let out = e.on_sample(432, fist_b);
        assert!(
            out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::Scroll { .. })),
            "G04 landmark scroll, got {:?}",
            out.commands
        );
    }

    #[test]
    fn landmark_palm_swipe_down_emits_escape() {
        let mut e = engine();
        e.set_tier(VisionTier::Active);
        // Palm (not tip) moves down ≥ SWIPE_DOWN_DY.
        let top = sample_from_landmarks(&open_palm_landmarks(0.5, 0.18, 0.5, 0.05), 0.95);
        let bot = sample_from_landmarks(&open_palm_landmarks(0.5, 0.48, 0.5, 0.35), 0.95);
        let _ = e.on_sample(0, top);
        let out = e.on_sample(80, bot);
        assert!(
            out.commands
                .iter()
                .any(|c| matches!(c, InjectCommand::KeyEscape)),
            "G05 landmark swipe → Esc, got {:?}",
            out.commands
        );
    }

    #[test]
    fn sleep_suppresses_landmark_inject() {
        let mut e = engine();
        e.set_tier(VisionTier::Sleep);
        let a = sample_from_landmarks(&open_palm_landmarks(0.4, 0.4, 0.35, 0.35), 0.95);
        let b = sample_from_landmarks(&open_palm_landmarks(0.55, 0.4, 0.55, 0.35), 0.95);
        let out = e.on_sample(0, a);
        assert!(out.commands.is_empty());
        let _ = e.on_sample(10, sample_from_landmarks(&fist_landmarks(0.5, 0.5), 0.95));
        let out = e.on_sample(100, b);
        assert!(
            out.commands.is_empty(),
            "Sleep must not inject from landmarks: {:?}",
            out.commands
        );
    }
}
