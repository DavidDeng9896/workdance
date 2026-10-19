//! Image-normalized palm → screen pixels.
//!
//! Front cameras mirror X only. Flick scroll is not implemented.

/// Ignore sub-pixel jitter. There is no max speed.
pub const MOVE_DEADZONE: f32 = 0.004;

pub fn screen_point(
    nx: f32,
    ny: f32,
    width: i32,
    height: i32,
    mirror_x: bool,
) -> Option<(i32, i32)> {
    if width <= 1 || height <= 1 {
        return None;
    }
    let x = if mirror_x { 1.0 - nx } else { nx };
    let x = x.clamp(0.0, 1.0);
    let y = ny.clamp(0.0, 1.0);
    let px = (x * (width - 1) as f32).round() as i32;
    let py = (y * (height - 1) as f32).round() as i32;
    Some((px, py))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_camera_mirrors_x_only() {
        let (x, y) = screen_point(0.25, 0.4, 1001, 501, true).unwrap();
        assert_eq!(x, 750);
        assert_eq!(y, 200);
        let (x2, y2) = screen_point(0.25, 0.4, 1001, 501, false).unwrap();
        assert_eq!((x2, y2), (250, 200));
    }
}
