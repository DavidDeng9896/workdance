//! Five-finger open palm vs fist from 21 landmarks.
//!
//! Extension is tip-vs-PIP distance, not the old curvature / DualTier score.

use crate::frame::{Landmark, HAND_LANDMARKS};

pub const WRIST: usize = 0;
pub const THUMB_IP: usize = 3;
pub const THUMB_TIP: usize = 4;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pose {
    Absent,
    OpenPalm,
    Fist,
    Other,
}

pub fn palm_center(lm: &[Landmark; HAND_LANDMARKS]) -> (f32, f32) {
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

pub fn classify_pose(lm: &[Landmark; HAND_LANDMARKS]) -> Pose {
    let fingers = [
        (INDEX_TIP, INDEX_PIP, INDEX_MCP),
        (MIDDLE_TIP, MIDDLE_PIP, MIDDLE_MCP),
        (RING_TIP, RING_PIP, RING_MCP),
        (PINKY_TIP, PINKY_PIP, PINKY_MCP),
    ];
    let extended = fingers
        .iter()
        .all(|(tip, pip, mcp)| finger_extended(lm, *tip, *pip, *mcp));
    let curled = fingers
        .iter()
        .all(|(tip, pip, mcp)| finger_curled(lm, *tip, *pip, *mcp));
    if extended && thumb_extended(lm) {
        Pose::OpenPalm
    } else if curled && thumb_curled(lm) {
        Pose::Fist
    } else {
        Pose::Other
    }
}

fn dist(a: Landmark, b: Landmark) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

fn finger_extended(lm: &[Landmark; HAND_LANDMARKS], tip: usize, pip: usize, mcp: usize) -> bool {
    let tip_wrist = dist(lm[tip], lm[WRIST]);
    let pip_wrist = dist(lm[pip], lm[WRIST]);
    let tip_mcp = dist(lm[tip], lm[mcp]);
    let pip_mcp = dist(lm[pip], lm[mcp]).max(1e-4);
    tip_wrist > pip_wrist * 1.12 && tip_mcp > pip_mcp * 1.35
}

fn finger_curled(lm: &[Landmark; HAND_LANDMARKS], tip: usize, pip: usize, mcp: usize) -> bool {
    let tip_mcp = dist(lm[tip], lm[mcp]);
    let pip_mcp = dist(lm[pip], lm[mcp]).max(1e-4);
    tip_mcp < pip_mcp * 1.15
}

fn thumb_extended(lm: &[Landmark; HAND_LANDMARKS]) -> bool {
    dist(lm[THUMB_TIP], lm[WRIST]) > dist(lm[THUMB_IP], lm[WRIST]) * 1.08
}

fn thumb_curled(lm: &[Landmark; HAND_LANDMARKS]) -> bool {
    dist(lm[THUMB_TIP], lm[INDEX_MCP]) <= dist(lm[THUMB_IP], lm[INDEX_MCP]) * 1.05
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    fn pt(x: f32, y: f32) -> Landmark {
        Landmark::new(x, y, 0.0)
    }

    pub fn blank() -> [Landmark; 21] {
        [pt(0.5, 0.5); 21]
    }

    pub fn open_palm(dx: f32, dy: f32) -> [Landmark; 21] {
        let mut lm = blank();
        let shift = |x: f32, y: f32| pt(x + dx, y + dy);
        lm[WRIST] = shift(0.50, 0.72);
        lm[1] = shift(0.40, 0.66);
        lm[2] = shift(0.34, 0.60);
        lm[THUMB_IP] = shift(0.30, 0.54);
        lm[THUMB_TIP] = shift(0.24, 0.46);
        lm[INDEX_MCP] = shift(0.42, 0.52);
        lm[INDEX_PIP] = shift(0.41, 0.40);
        lm[7] = shift(0.40, 0.32);
        lm[INDEX_TIP] = shift(0.39, 0.22);
        lm[MIDDLE_MCP] = shift(0.50, 0.50);
        lm[MIDDLE_PIP] = shift(0.50, 0.36);
        lm[11] = shift(0.50, 0.26);
        lm[MIDDLE_TIP] = shift(0.50, 0.16);
        lm[RING_MCP] = shift(0.58, 0.52);
        lm[RING_PIP] = shift(0.59, 0.40);
        lm[15] = shift(0.60, 0.32);
        lm[RING_TIP] = shift(0.61, 0.22);
        lm[PINKY_MCP] = shift(0.66, 0.56);
        lm[PINKY_PIP] = shift(0.68, 0.46);
        lm[19] = shift(0.70, 0.38);
        lm[PINKY_TIP] = shift(0.72, 0.30);
        lm
    }

    pub fn fist_at(dx: f32, dy: f32) -> [Landmark; 21] {
        let mut lm = blank();
        let shift = |x: f32, y: f32| pt(x + dx, y + dy);
        lm[WRIST] = shift(0.50, 0.78);
        lm[1] = shift(0.44, 0.70);
        lm[2] = shift(0.40, 0.64);
        lm[THUMB_IP] = shift(0.46, 0.58);
        // Tip tucked next to the index MCP.
        lm[INDEX_MCP] = shift(0.42, 0.55);
        lm[THUMB_TIP] = shift(0.43, 0.56);
        lm[INDEX_PIP] = shift(0.42, 0.42);
        lm[7] = shift(0.42, 0.50);
        lm[INDEX_TIP] = shift(0.42, 0.54);
        lm[MIDDLE_MCP] = shift(0.50, 0.54);
        lm[MIDDLE_PIP] = shift(0.50, 0.40);
        lm[11] = shift(0.50, 0.48);
        lm[MIDDLE_TIP] = shift(0.50, 0.53);
        lm[RING_MCP] = shift(0.58, 0.55);
        lm[RING_PIP] = shift(0.58, 0.42);
        lm[15] = shift(0.58, 0.50);
        lm[RING_TIP] = shift(0.58, 0.54);
        lm[PINKY_MCP] = shift(0.66, 0.58);
        lm[PINKY_PIP] = shift(0.66, 0.46);
        lm[19] = shift(0.66, 0.54);
        lm[PINKY_TIP] = shift(0.66, 0.57);
        lm
    }

    #[test]
    fn open_five_and_fist_and_partial() {
        assert_eq!(classify_pose(&open_palm(0.0, 0.0)), Pose::OpenPalm);
        assert_eq!(classify_pose(&fist_at(0.0, 0.0)), Pose::Fist);
        let mut partial = open_palm(0.0, 0.0);
        // Curl the index only.
        partial[INDEX_TIP] = partial[INDEX_MCP];
        assert_eq!(classify_pose(&partial), Pose::Other);
    }
}
