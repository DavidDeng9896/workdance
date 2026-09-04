//! Focus lock (WP3): dwell 0.4s on a target; gesture move does not unlock.

use serde::{Deserialize, Serialize};

/// Hover dwell before focus locks (locked spec).
pub const FOCUS_DWELL_MS: u64 = 400;

/// Abstract focusable control (AX/UIA id on Win/Mac; stub id on Linux).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FocusTarget {
    pub id: String,
    pub label: String,
    pub is_editable: bool,
}

impl FocusTarget {
    pub fn stub_at(x: i32, y: i32) -> Self {
        Self {
            id: format!("stub:{x},{y}"),
            label: "stub-input".into(),
            is_editable: true,
        }
    }
}

/// Resolves which control is under the cursor.
pub trait FocusProbe: Send {
    fn hit_test(&self, screen_x: i32, screen_y: i32) -> Option<FocusTarget>;
}

/// Linux / CI: grid-snapped stub targets (no AX/UIA).
#[derive(Debug, Default)]
pub struct StubFocusProbe;

impl FocusProbe for StubFocusProbe {
    fn hit_test(&self, screen_x: i32, screen_y: i32) -> Option<FocusTarget> {
        // Snap to 40px cells so dwell can accumulate while cursor jitters slightly.
        let sx = (screen_x / 40) * 40;
        let sy = (screen_y / 40) * 40;
        Some(FocusTarget::stub_at(sx, sy))
    }
}

/// Windows UIA hit-test placeholder (WP3): falls back to stub until full UIA lands.
#[cfg(target_os = "windows")]
#[derive(Debug, Default)]
pub struct UiaFocusProbe;

#[cfg(target_os = "windows")]
impl FocusProbe for UiaFocusProbe {
    fn hit_test(&self, screen_x: i32, screen_y: i32) -> Option<FocusTarget> {
        StubFocusProbe.hit_test(screen_x, screen_y)
    }
}

/// macOS AX hit-test placeholder (WP3): falls back to stub until full AX lands.
#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
pub struct AxFocusProbe;

#[cfg(target_os = "macos")]
impl FocusProbe for AxFocusProbe {
    fn hit_test(&self, screen_x: i32, screen_y: i32) -> Option<FocusTarget> {
        StubFocusProbe.hit_test(screen_x, screen_y)
    }
}

/// Platform probe: UIA/AX behind cfg; Linux stub.
pub fn create_default_focus_probe() -> Box<dyn FocusProbe> {
    #[cfg(target_os = "windows")]
    {
        return Box::new(UiaFocusProbe);
    }
    #[cfg(target_os = "macos")]
    {
        return Box::new(AxFocusProbe);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Box::new(StubFocusProbe)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Dwell {
    None,
    Pending {
        target: FocusTarget,
        since_ms: u64,
    },
}

/// Keeps locked focus across WP2 cursor moves until explicitly cleared / leave app.
#[derive(Debug)]
pub struct FocusLock {
    dwell: Dwell,
    locked: Option<FocusTarget>,
}

impl Default for FocusLock {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusLock {
    pub fn new() -> Self {
        Self {
            dwell: Dwell::None,
            locked: None,
        }
    }

    pub fn locked(&self) -> Option<&FocusTarget> {
        self.locked.as_ref()
    }

    /// Fist reconfirm: if hovering same target as locked, keep; if new target pending, lock now.
    pub fn reconfirm(&mut self) {
        if let Dwell::Pending { target, .. } = &self.dwell {
            self.locked = Some(target.clone());
        }
    }

    pub fn clear(&mut self) {
        self.dwell = Dwell::None;
        self.locked = None;
    }

    /// Update with current hit under cursor. Does **not** clear an existing lock on move.
    pub fn on_hover(&mut self, now_ms: u64, hit: Option<FocusTarget>) {
        match hit {
            None => {
                self.dwell = Dwell::None;
                // Spec: gesture move does not unlock — keep `locked`.
            }
            Some(target) => {
                if self.locked.as_ref() == Some(&target) {
                    self.dwell = Dwell::None;
                    return;
                }
                match &self.dwell {
                    Dwell::Pending {
                        target: pending,
                        since_ms,
                    } if pending == &target => {
                        if now_ms.saturating_sub(*since_ms) >= FOCUS_DWELL_MS {
                            self.locked = Some(target);
                            self.dwell = Dwell::None;
                        }
                    }
                    _ => {
                        self.dwell = Dwell::Pending {
                            target,
                            since_ms: now_ms,
                        };
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dwell_0_4s_locks_focus() {
        let mut fl = FocusLock::new();
        let t = FocusTarget::stub_at(100, 200);
        fl.on_hover(0, Some(t.clone()));
        assert!(fl.locked().is_none());
        fl.on_hover(399, Some(t.clone()));
        assert!(fl.locked().is_none());
        fl.on_hover(400, Some(t.clone()));
        assert_eq!(fl.locked(), Some(&t));
    }

    #[test]
    fn cursor_move_does_not_unlock() {
        let mut fl = FocusLock::new();
        let t = FocusTarget::stub_at(100, 200);
        fl.on_hover(0, Some(t.clone()));
        fl.on_hover(400, Some(t.clone()));
        assert!(fl.locked().is_some());
        // Move to another cell — lock stays.
        fl.on_hover(500, Some(FocusTarget::stub_at(400, 400)));
        assert_eq!(fl.locked().map(|x| x.id.as_str()), Some("stub:100,200"));
    }

    #[test]
    fn fist_reconfirm_locks_pending() {
        let mut fl = FocusLock::new();
        let t = FocusTarget::stub_at(10, 10);
        fl.on_hover(0, Some(t.clone()));
        fl.reconfirm();
        assert_eq!(fl.locked(), Some(&t));
    }
}
