use crate::camera::FrameBuffer;

/// Horizontally mirror an RGB frame (front-camera selfie view → model view).
pub fn mirror_horizontal(frame: &FrameBuffer) -> FrameBuffer {
    let w = frame.width as usize;
    let h = frame.height as usize;
    if w == 0 || h == 0 || frame.rgb.len() < w * h * 3 {
        return frame.clone();
    }
    let mut out = vec![0u8; frame.rgb.len()];
    for y in 0..h {
        let row = y * w * 3;
        for x in 0..w {
            let src = row + x * 3;
            let dst = row + (w - 1 - x) * 3;
            out[dst..dst + 3].copy_from_slice(&frame.rgb[src..src + 3]);
        }
    }
    FrameBuffer {
        width: frame.width,
        height: frame.height,
        rgb: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_left_right() {
        // 2x1 RGB: left pixel red, right pixel blue.
        let frame = FrameBuffer {
            width: 2,
            height: 1,
            rgb: vec![255, 0, 0, 0, 0, 255],
        };
        let mirrored = mirror_horizontal(&frame);
        assert_eq!(&mirrored.rgb[0..3], &[0, 0, 255]);
        assert_eq!(&mirrored.rgb[3..6], &[255, 0, 0]);
    }
}
