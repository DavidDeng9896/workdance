//! G07 long-fist dictation state machine (WP3).

use crate::asr::{AsrBackend, AsrResult};

/// Fist must be held this long before recording starts.
pub const G07_ARM_MS: u64 = 1000;
/// Max recording length.
pub const G07_MAX_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G07Phase {
    Idle,
    /// Fist held, not yet armed.
    Arming { since_ms: u64 },
    Recording { started_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum G07Event {
    RecordingStarted,
    /// Abort without ASR (leave frame / cancel).
    RecordingAborted,
    /// Finished; ASR text ready to append (audio already discarded by backend).
    DictationReady { text: String },
}

/// In-memory capture buffer — never flushed to disk in WP3.
#[derive(Debug, Default)]
struct CaptureBuf {
    bytes: Vec<u8>,
}

impl CaptureBuf {
    fn push_tick(&mut self) {
        // Opaque placeholder samples for stub ASR (not real mic).
        self.bytes.extend_from_slice(&[0u8; 32]);
    }

    fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }

    fn clear(&mut self) {
        self.bytes.clear();
    }
}

pub struct G07Machine {
    phase: G07Phase,
    buf: CaptureBuf,
}

impl Default for G07Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl G07Machine {
    pub fn new() -> Self {
        Self {
            phase: G07Phase::Idle,
            buf: CaptureBuf::default(),
        }
    }

    pub fn phase(&self) -> G07Phase {
        self.phase
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.phase, G07Phase::Recording { .. })
    }

    /// Feed fist/presence. `fist` true while closed; `present` false = leave frame.
    pub fn on_sample(
        &mut self,
        now_ms: u64,
        present: bool,
        fist: bool,
        asr: &mut dyn AsrBackend,
    ) -> Vec<G07Event> {
        let mut ev = Vec::new();

        if !present {
            if matches!(self.phase, G07Phase::Recording { .. } | G07Phase::Arming { .. }) {
                if matches!(self.phase, G07Phase::Recording { .. }) {
                    ev.push(G07Event::RecordingAborted);
                }
                self.buf.clear();
                self.phase = G07Phase::Idle;
            }
            return ev;
        }

        match self.phase {
            G07Phase::Idle => {
                if fist {
                    self.phase = G07Phase::Arming { since_ms: now_ms };
                }
            }
            G07Phase::Arming { since_ms } => {
                if !fist {
                    self.phase = G07Phase::Idle;
                } else if now_ms.saturating_sub(since_ms) >= G07_ARM_MS {
                    self.buf.clear();
                    self.buf.push_tick();
                    self.phase = G07Phase::Recording { started_ms: now_ms };
                    ev.push(G07Event::RecordingStarted);
                }
            }
            G07Phase::Recording { started_ms } => {
                if !fist {
                    let pcm = self.buf.take();
                    let result: AsrResult = asr.transcribe_zh(&pcm);
                    self.phase = G07Phase::Idle;
                    ev.push(G07Event::DictationReady {
                        text: result.text,
                    });
                } else {
                    self.buf.push_tick();
                    if now_ms.saturating_sub(started_ms) >= G07_MAX_MS {
                        let pcm = self.buf.take();
                        let result = asr.transcribe_zh(&pcm);
                        self.phase = G07Phase::Idle;
                        ev.push(G07Event::DictationReady {
                            text: result.text,
                        });
                    }
                }
            }
        }
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::StubAsr;

    #[test]
    fn fist_under_1s_does_not_start_recording() {
        let mut m = G07Machine::new();
        let mut asr = StubAsr;
        let ev = m.on_sample(0, true, true, &mut asr);
        assert!(ev.is_empty());
        let ev = m.on_sample(999, true, true, &mut asr);
        assert!(ev.is_empty());
        assert!(!m.is_recording());
        // Release before arm.
        let ev = m.on_sample(1000, true, false, &mut asr);
        assert!(ev.is_empty());
        assert_eq!(m.phase(), G07Phase::Idle);
    }

    #[test]
    fn fist_at_1s_starts_recording() {
        let mut m = G07Machine::new();
        let mut asr = StubAsr;
        m.on_sample(0, true, true, &mut asr);
        let ev = m.on_sample(1000, true, true, &mut asr);
        assert_eq!(ev, vec![G07Event::RecordingStarted]);
        assert!(m.is_recording());
    }

    #[test]
    fn leave_frame_aborts_recording() {
        let mut m = G07Machine::new();
        let mut asr = StubAsr;
        m.on_sample(0, true, true, &mut asr);
        m.on_sample(1000, true, true, &mut asr);
        let ev = m.on_sample(1500, false, false, &mut asr);
        assert_eq!(ev, vec![G07Event::RecordingAborted]);
        assert!(!m.is_recording());
    }

    #[test]
    fn release_runs_stub_asr_append_text() {
        let mut m = G07Machine::new();
        let mut asr = StubAsr;
        m.on_sample(0, true, true, &mut asr);
        m.on_sample(1000, true, true, &mut asr);
        let ev = m.on_sample(2000, true, false, &mut asr);
        match &ev[..] {
            [G07Event::DictationReady { text }] => {
                assert!(!text.is_empty());
                assert!(text.chars().any(|c| c > '\u{7f}'));
            }
            other => panic!("expected DictationReady, got {other:?}"),
        }
    }
}
