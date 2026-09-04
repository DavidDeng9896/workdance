//! Exponential smoother (One Euro–lite): low lag, stable at sleep→active ramp.

#[derive(Debug, Clone)]
pub struct ExpSmoother {
    alpha: f32,
    value: Option<f32>,
}

impl ExpSmoother {
    pub fn new(alpha: f32) -> Self {
        Self {
            alpha: alpha.clamp(0.01, 1.0),
            value: None,
        }
    }

    pub fn reset(&mut self) {
        self.value = None;
    }

    pub fn push(&mut self, raw: f32) -> f32 {
        let next = match self.value {
            None => raw,
            Some(prev) => prev + self.alpha * (raw - prev),
        };
        self.value = Some(next);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_toward_input() {
        let mut s = ExpSmoother::new(0.5);
        let a = s.push(0.0);
        let b = s.push(1.0);
        assert_eq!(a, 0.0);
        assert!((b - 0.5).abs() < f32::EPSILON);
    }
}
