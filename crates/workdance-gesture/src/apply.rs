//! Apply gate decisions: pointer under the shared lock, dictation edges outside it.

use crate::gate::Decision;
use crate::lock::{io_lock_held, with_io_serial};
use crate::pointer::screen_point;

#[derive(Debug, Default)]
pub struct EdgeSession {
    press_id: Option<u64>,
}

impl EdgeSession {
    pub fn press_id(&self) -> Option<u64> {
        self.press_id
    }
}

pub trait DictationEdges {
    fn alloc_press_id(&mut self) -> u64;
    /// `pressed == true` is a hotkey press edge. Return whether a session is live afterwards.
    fn edge(&mut self, pressed: bool, press_id: u64) -> bool;
}

pub trait PointerInject {
    fn move_to(&mut self, x: i32, y: i32);
    fn left_click(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub width: i32,
    pub height: i32,
    pub mirror_x: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    /// The press edge did not open a session. The gate must leave recording.
    AbandonRecording,
}

pub fn apply_decision<D, P>(
    decision: Decision,
    edges: &mut D,
    pointer: &mut P,
    screen: Screen,
    session: &mut EdgeSession,
) -> ApplyOutcome
where
    D: DictationEdges,
    P: PointerInject,
{
    match decision {
        Decision::Move { x, y } => {
            if let Some((px, py)) = screen_point(x, y, screen.width, screen.height, screen.mirror_x)
            {
                with_io_serial(|| pointer.move_to(px, py));
            }
            ApplyOutcome::Applied
        }
        Decision::Click => {
            with_io_serial(|| pointer.left_click());
            ApplyOutcome::Applied
        }
        Decision::DictationPressed => {
            debug_assert!(!io_lock_held());
            let id = edges.alloc_press_id();
            session.press_id = Some(id);
            if edges.edge(true, id) {
                ApplyOutcome::Applied
            } else {
                session.press_id = None;
                ApplyOutcome::AbandonRecording
            }
        }
        Decision::DictationReleased => {
            debug_assert!(!io_lock_held());
            if let Some(id) = session.press_id.take() {
                let _ = edges.edge(false, id);
            }
            ApplyOutcome::Applied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::io_lock_held;

    struct Edges {
        next: u64,
        log: Vec<(bool, u64)>,
        live: bool,
    }

    impl DictationEdges for Edges {
        fn alloc_press_id(&mut self) -> u64 {
            self.next += 1;
            self.next
        }

        fn edge(&mut self, pressed: bool, press_id: u64) -> bool {
            assert!(!io_lock_held());
            self.log.push((pressed, press_id));
            if pressed {
                self.live
            } else {
                self.live = false;
                false
            }
        }
    }

    struct Pointer {
        moves: Vec<(i32, i32)>,
        clicks: u32,
    }

    impl PointerInject for Pointer {
        fn move_to(&mut self, x: i32, y: i32) {
            assert!(io_lock_held());
            self.moves.push((x, y));
        }

        fn left_click(&mut self) {
            assert!(io_lock_held());
            self.clicks += 1;
        }
    }

    #[test]
    fn wires_move_click_and_matching_edges() {
        let mut edges = Edges {
            next: 0,
            log: Vec::new(),
            live: true,
        };
        let mut pointer = Pointer {
            moves: Vec::new(),
            clicks: 0,
        };
        let mut session = EdgeSession::default();
        let screen = Screen {
            width: 1001,
            height: 501,
            mirror_x: true,
        };
        apply_decision(
            Decision::Move { x: 0.25, y: 0.4 },
            &mut edges,
            &mut pointer,
            screen,
            &mut session,
        );
        assert_eq!(pointer.moves, vec![(750, 200)]);
        apply_decision(
            Decision::Click,
            &mut edges,
            &mut pointer,
            screen,
            &mut session,
        );
        assert_eq!(pointer.clicks, 1);
        assert_eq!(
            apply_decision(
                Decision::DictationPressed,
                &mut edges,
                &mut pointer,
                screen,
                &mut session,
            ),
            ApplyOutcome::Applied
        );
        apply_decision(
            Decision::DictationReleased,
            &mut edges,
            &mut pointer,
            screen,
            &mut session,
        );
        assert_eq!(edges.log, vec![(true, 1), (false, 1)]);
        assert!(session.press_id().is_none());
    }

    #[test]
    fn rejected_press_asks_the_gate_to_abandon() {
        let mut edges = Edges {
            next: 0,
            log: Vec::new(),
            live: false,
        };
        let mut pointer = Pointer {
            moves: Vec::new(),
            clicks: 0,
        };
        let mut session = EdgeSession::default();
        let screen = Screen {
            width: 100,
            height: 100,
            mirror_x: false,
        };
        assert_eq!(
            apply_decision(
                Decision::DictationPressed,
                &mut edges,
                &mut pointer,
                screen,
                &mut session,
            ),
            ApplyOutcome::AbandonRecording
        );
        apply_decision(
            Decision::DictationReleased,
            &mut edges,
            &mut pointer,
            screen,
            &mut session,
        );
        assert_eq!(edges.log, vec![(true, 1)]);
    }
}
