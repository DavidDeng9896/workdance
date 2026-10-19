//! RGB frames and the 21 MediaPipe hand landmarks.

use serde::{Deserialize, Serialize};

pub const HAND_LANDMARKS: usize = 21;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Landmark {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Landmark {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// One detector tick. `landmarks == None` means no hand.
#[derive(Debug, Clone, PartialEq)]
pub struct HandObservation {
    pub t_ms: u64,
    pub landmarks: Option<[Landmark; HAND_LANDMARKS]>,
    pub confidence: f32,
}

impl HandObservation {
    pub fn absent(t_ms: u64) -> Self {
        Self {
            t_ms,
            landmarks: None,
            confidence: 0.0,
        }
    }

    pub fn with_landmarks(
        t_ms: u64,
        landmarks: [Landmark; HAND_LANDMARKS],
        confidence: f32,
    ) -> Self {
        Self {
            t_ms,
            landmarks: Some(landmarks),
            confidence,
        }
    }
}

/// Contiguous RGB8, row-major, tightly packed.
#[derive(Debug, Clone)]
pub struct FrameBuffer {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl FrameBuffer {
    pub fn new(width: u32, height: u32, rgb: Vec<u8>) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(3)?;
        if rgb.len() != expected || width == 0 || height == 0 {
            return None;
        }
        Some(Self { width, height, rgb })
    }
}
