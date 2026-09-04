//! Offline ASR trait (WP3). Stub for CI; optional whisper feature later.

use std::path::PathBuf;

/// Result of a local dictation pass — never uploaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrResult {
    pub text: String,
    /// True when audio bytes were discarded (must be true for WP3 policy).
    pub audio_discarded: bool,
}

pub trait AsrBackend: Send {
    fn name(&self) -> &'static str;
    /// Transcribe PCM/raw bytes in-memory; must not write audio to disk.
    fn transcribe_zh(&mut self, pcm_or_opaque: &[u8]) -> AsrResult;
}

/// CI / default: returns fixed Chinese text; ignores audio; never persists files.
#[derive(Debug, Default)]
pub struct StubAsr;

impl AsrBackend for StubAsr {
    fn name(&self) -> &'static str {
        "stub-zh"
    }

    fn transcribe_zh(&mut self, _pcm_or_opaque: &[u8]) -> AsrResult {
        AsrResult {
            text: "实验记录已追加。".into(),
            audio_discarded: true,
        }
    }
}

/// Optional whisper.cpp path (feature `whisper`). Model not committed.
#[cfg(feature = "whisper")]
pub struct WhisperAsr {
    #[allow(dead_code)]
    pub model_path: PathBuf,
}

#[cfg(feature = "whisper")]
impl AsrBackend for WhisperAsr {
    fn name(&self) -> &'static str {
        "whisper"
    }

    fn transcribe_zh(&mut self, _pcm_or_opaque: &[u8]) -> AsrResult {
        // Placeholder until a local model is supplied via download script.
        AsrResult {
            text: String::new(),
            audio_discarded: true,
        }
    }
}

pub fn create_default_asr() -> Box<dyn AsrBackend> {
    #[cfg(feature = "whisper")]
    {
        let path = default_whisper_model_path();
        if path.is_file() {
            return Box::new(WhisperAsr { model_path: path });
        }
    }
    Box::new(StubAsr)
}

pub fn default_whisper_model_path() -> PathBuf {
    if let Ok(p) = std::env::var("WORKDANCE_WHISPER_MODEL") {
        return PathBuf::from(p);
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("workdance")
        .join("models")
        .join("ggml-tiny.bin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_chinese_and_discards_audio() {
        let mut asr = StubAsr;
        let r = asr.transcribe_zh(b"fake-pcm");
        assert!(r.text.chars().any(|c| c > '\u{7f}'));
        assert!(r.audio_discarded);
    }

    #[test]
    fn stub_does_not_create_audio_files() {
        let tmp = std::env::temp_dir().join("workdance-asr-stub-probe");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let before: Vec<_> = std::fs::read_dir(&tmp).unwrap().collect();
        let mut asr = StubAsr;
        let _ = asr.transcribe_zh(&[0u8; 64]);
        let after: Vec<_> = std::fs::read_dir(&tmp).unwrap().collect();
        assert_eq!(before.len(), after.len());
    }
}
