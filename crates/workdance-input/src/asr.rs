//! Offline ASR trait (WP3 / WP-A1).
//!
//! Production: `SherpaAsr` behind feature `sherpa-asr` when the local model is
//! present; otherwise `UnavailableAsr` (never the StubAsr fixed sentence).
//! StubAsr is only for `cfg(test)` or `WORKDANCE_ASR_STUB=1`.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// 16 kHz mono i16 LE — max duration aligned with G07.
pub const ASR_SAMPLE_RATE_HZ: u32 = 16_000;
pub const ASR_MAX_SECONDS: u32 = 60;
pub const ASR_MAX_PCM_BYTES: usize =
    (ASR_SAMPLE_RATE_HZ as usize) * (ASR_MAX_SECONDS as usize) * 2;

/// Fixed stub sentence — CI / explicit stub only; never production focus text.
pub const STUB_ASR_TEXT: &str = "实验记录已追加。";

/// Result of a local dictation pass — never uploaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrResult {
    pub text: String,
    /// True when audio bytes were discarded (must be true for WP3 / A1 policy).
    pub audio_discarded: bool,
}

pub trait AsrBackend: Send {
    fn name(&self) -> &'static str;
    /// Transcribe PCM/raw bytes in-memory; must not write audio to disk.
    fn transcribe_zh(&mut self, pcm_or_opaque: &[u8]) -> AsrResult;
}

/// CI / explicit stub: returns fixed Chinese text; ignores audio; never persists files.
#[derive(Debug, Default)]
pub struct StubAsr;

impl AsrBackend for StubAsr {
    fn name(&self) -> &'static str {
        "stub-zh"
    }

    fn transcribe_zh(&mut self, _pcm_or_opaque: &[u8]) -> AsrResult {
        AsrResult {
            text: STUB_ASR_TEXT.into(),
            audio_discarded: true,
        }
    }
}

/// Production fallback when sherpa feature/model is unavailable.
/// Never emits [`STUB_ASR_TEXT`].
#[derive(Debug, Default)]
pub struct UnavailableAsr;

impl AsrBackend for UnavailableAsr {
    fn name(&self) -> &'static str {
        "unavailable"
    }

    fn transcribe_zh(&mut self, _pcm_or_opaque: &[u8]) -> AsrResult {
        AsrResult {
            text: String::new(),
            audio_discarded: true,
        }
    }
}

/// Settings / tray readiness for the ASR model (never silent fake success).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AsrBackendStatus {
    pub backend: String,
    pub ok: bool,
    pub model_installed: bool,
    pub model_dir: String,
    /// Human-readable status for settings banner (Chinese OK).
    pub message: String,
}

impl AsrBackendStatus {
    pub fn probe() -> Self {
        let dir = default_asr_model_dir();
        let installed = asr_model_files_present(&dir);
        let dir_s = dir.display().to_string();

        #[cfg(feature = "sherpa-asr")]
        {
            if installed {
                return Self {
                    backend: "sherpa-asr".into(),
                    ok: true,
                    model_installed: true,
                    model_dir: dir_s,
                    message: "听写模型已安装（Paraformer-zh-small / sherpa-onnx）".into(),
                };
            }
            return Self {
                backend: "sherpa-asr".into(),
                ok: false,
                model_installed: false,
                model_dir: dir_s.clone(),
                message: format!(
                    "听写模型缺失。请运行 scripts/download-asr-model.sh（安装到 {dir_s}）。启用 feature sherpa-asr 后可真转写。"
                ),
            };
        }

        #[cfg(not(feature = "sherpa-asr"))]
        {
            if installed {
                Self {
                    backend: "unavailable".into(),
                    ok: false,
                    model_installed: true,
                    model_dir: dir_s,
                    message: "模型文件已在本地，但本构建未启用 sherpa-asr feature（不会用 Stub 假听写）。".into(),
                }
            } else {
                Self {
                    backend: "unavailable".into(),
                    ok: false,
                    model_installed: false,
                    model_dir: dir_s.clone(),
                    message: format!(
                        "听写模型未安装。请运行 scripts/download-asr-model.sh → {dir_s}；并以 --features sherpa-asr 构建。"
                    ),
                }
            }
        }
    }
}

/// Optional whisper.cpp path (feature `whisper`). Quarantined from production factory (WP-A1).
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
        // Placeholder — not selected by create_default_asr (WP-A1 quarantine).
        AsrResult {
            text: String::new(),
            audio_discarded: true,
        }
    }
}

/// sherpa-onnx Paraformer-zh-small backend (feature `sherpa-asr`).
#[cfg(feature = "sherpa-asr")]
pub struct SherpaAsr {
    recognizer: sherpa_onnx::OfflineRecognizer,
}

#[cfg(feature = "sherpa-asr")]
impl SherpaAsr {
    pub fn try_open(model_dir: &Path) -> Result<Self, String> {
        if !asr_model_files_present(model_dir) {
            return Err(format!(
                "ASR model missing under {} (need model.int8.onnx + tokens.txt) — run scripts/download-asr-model.sh",
                model_dir.display()
            ));
        }
        let model = model_dir.join("model.int8.onnx");
        let tokens = model_dir.join("tokens.txt");

        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        config.model_config.paraformer = sherpa_onnx::OfflineParaformerModelConfig {
            model: Some(model.to_string_lossy().into_owned()),
        };
        config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
        config.model_config.provider = Some("cpu".into());
        config.model_config.num_threads = 2;
        config.model_config.debug = false;

        let recognizer = sherpa_onnx::OfflineRecognizer::create(&config).ok_or_else(|| {
            format!(
                "failed to create sherpa Offline recognizer from {}",
                model_dir.display()
            )
        })?;
        Ok(Self { recognizer })
    }
}

#[cfg(feature = "sherpa-asr")]
impl AsrBackend for SherpaAsr {
    fn name(&self) -> &'static str {
        "sherpa-paraformer-zh"
    }

    fn transcribe_zh(&mut self, pcm_or_opaque: &[u8]) -> AsrResult {
        // In-memory only — never write wav/mp3/raw.
        let samples = pcm_i16le_mono_to_f32(pcm_or_opaque);
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(ASR_SAMPLE_RATE_HZ as i32, &samples);
        self.recognizer.decode(&stream);
        let text = stream
            .get_result()
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default();
        AsrResult {
            text,
            audio_discarded: true,
        }
    }
}

/// Convert 16 kHz mono i16 LE PCM to f32 samples; truncates to ≤60 s.
pub fn pcm_i16le_mono_to_f32(pcm: &[u8]) -> Vec<f32> {
    let byte_len = pcm.len().min(ASR_MAX_PCM_BYTES);
    let even = byte_len & !1;
    let n = even / 2;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let s = i16::from_le_bytes([pcm[i * 2], pcm[i * 2 + 1]]);
        out.push(s as f32 / 32768.0);
    }
    out
}

pub fn default_asr_model_dir() -> PathBuf {
    if let Ok(p) = std::env::var("WORKDANCE_ASR_MODEL_DIR") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("workdance")
        .join("models")
        .join("asr")
        .join("paraformer-zh-small")
}

pub fn asr_model_files_present(dir: &Path) -> bool {
    dir.join("model.int8.onnx").is_file() && dir.join("tokens.txt").is_file()
}

fn asr_stub_env_enabled() -> bool {
    std::env::var("WORKDANCE_ASR_STUB")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Whether StubAsr is allowed (tests or explicit env).
pub fn asr_stub_allowed() -> bool {
    cfg!(test) || asr_stub_env_enabled()
}

/// Selection knobs for unit tests (production uses [`create_default_asr`]).
#[derive(Debug, Clone, Copy)]
pub struct AsrFactoryOptions {
    pub stub_allowed: bool,
}

impl Default for AsrFactoryOptions {
    fn default() -> Self {
        Self {
            stub_allowed: asr_stub_allowed(),
        }
    }
}

/// Resolve ASR backend per WP-A1 §2.2.
pub fn create_asr_with_options(opts: AsrFactoryOptions) -> Box<dyn AsrBackend> {
    #[cfg(feature = "sherpa-asr")]
    {
        let dir = default_asr_model_dir();
        if asr_model_files_present(&dir) {
            match SherpaAsr::try_open(&dir) {
                Ok(asr) => return Box::new(asr),
                Err(e) => {
                    eprintln!("[workdance-input] sherpa-asr open failed: {e}");
                }
            }
        }
    }

    if opts.stub_allowed {
        return Box::new(StubAsr);
    }

    // Production: never emit StubAsr fixed sentence (whisper placeholder also quarantined).
    Box::new(UnavailableAsr)
}

pub fn create_default_asr() -> Box<dyn AsrBackend> {
    create_asr_with_options(AsrFactoryOptions::default())
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
    use std::fs;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn stub_returns_chinese_and_discards_audio() {
        let mut asr = StubAsr;
        let r = asr.transcribe_zh(b"fake-pcm");
        assert_eq!(r.text, STUB_ASR_TEXT);
        assert!(r.text.chars().any(|c| c > '\u{7f}'));
        assert!(r.audio_discarded);
    }

    #[test]
    fn stub_does_not_create_audio_files() {
        let tmp = std::env::temp_dir().join("workdance-asr-stub-probe");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let before: Vec<_> = fs::read_dir(&tmp).unwrap().collect();
        let mut asr = StubAsr;
        let _ = asr.transcribe_zh(&[0u8; 64]);
        let after: Vec<_> = fs::read_dir(&tmp).unwrap().collect();
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn unavailable_never_emits_stub_sentence_and_discards() {
        let mut asr = UnavailableAsr;
        let r = asr.transcribe_zh(&[0u8; 128]);
        assert_eq!(asr.name(), "unavailable");
        assert!(r.text.is_empty());
        assert_ne!(r.text, STUB_ASR_TEXT);
        assert!(r.audio_discarded);
    }

    #[test]
    fn unavailable_does_not_write_wav_mp3_or_raw() {
        let tmp = std::env::temp_dir().join("workdance-asr-unavailable-probe");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let before: Vec<_> = fs::read_dir(&tmp).unwrap().map(|e| e.unwrap().path()).collect();
        let mut asr = UnavailableAsr;
        let _ = asr.transcribe_zh(&[1u8, 2, 3, 4, 5, 6, 7, 8]);
        let after: Vec<_> = fs::read_dir(&tmp).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(before, after);
        for ext in ["wav", "mp3", "raw", "pcm"] {
            assert!(
                !tmp.join(format!("out.{ext}")).exists(),
                "must not create audio file with .{ext}"
            );
        }
    }

    #[test]
    fn factory_production_path_uses_unavailable_not_stub() {
        let _g = env_lock().lock().unwrap();
        let tmp = std::env::temp_dir().join("workdance-asr-factory-unavailable");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("WORKDANCE_ASR_MODEL_DIR", &tmp);
        let mut asr = create_asr_with_options(AsrFactoryOptions {
            stub_allowed: false,
        });
        assert_eq!(asr.name(), "unavailable");
        let r = asr.transcribe_zh(&[0u8; 32]);
        assert_ne!(r.text, STUB_ASR_TEXT);
        assert!(r.text.is_empty());
        assert!(r.audio_discarded);
        std::env::remove_var("WORKDANCE_ASR_MODEL_DIR");
    }

    #[test]
    fn factory_test_default_allows_stub() {
        let _g = env_lock().lock().unwrap();
        let tmp = std::env::temp_dir().join("workdance-asr-factory-stub");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("WORKDANCE_ASR_MODEL_DIR", &tmp);
        // Under cfg(test), create_default_asr must still yield Stub for CI (no model).
        let asr = create_default_asr();
        assert_eq!(asr.name(), "stub-zh");
        std::env::remove_var("WORKDANCE_ASR_MODEL_DIR");
    }

    #[test]
    fn pcm_i16le_converts_and_caps_at_60s() {
        // one sample: 0x00 0x40 → 16384 / 32768 = 0.5
        let pcm = [0x00u8, 0x40];
        let s = pcm_i16le_mono_to_f32(&pcm);
        assert_eq!(s.len(), 1);
        assert!((s[0] - 0.5).abs() < 1e-4);

        let huge = vec![0u8; ASR_MAX_PCM_BYTES + 100];
        let capped = pcm_i16le_mono_to_f32(&huge);
        assert_eq!(capped.len(), ASR_SAMPLE_RATE_HZ as usize * ASR_MAX_SECONDS as usize);
    }

    #[test]
    fn default_model_dir_respects_env_override() {
        let _g = env_lock().lock().unwrap();
        let tmp = std::env::temp_dir().join("workdance-asr-model-dir-override");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("WORKDANCE_ASR_MODEL_DIR", &tmp);
        assert_eq!(default_asr_model_dir(), tmp);
        std::env::remove_var("WORKDANCE_ASR_MODEL_DIR");
    }

    #[test]
    fn asr_status_reports_missing_model() {
        let _g = env_lock().lock().unwrap();
        let tmp = std::env::temp_dir().join("workdance-asr-status-missing");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("WORKDANCE_ASR_MODEL_DIR", &tmp);
        let st = AsrBackendStatus::probe();
        assert!(!st.model_installed);
        assert!(!st.ok);
        assert!(st.message.contains("download-asr-model.sh"));
        std::env::remove_var("WORKDANCE_ASR_MODEL_DIR");
    }

    #[test]
    fn asr_status_reports_installed_when_files_present() {
        let _g = env_lock().lock().unwrap();
        let tmp = std::env::temp_dir().join("workdance-asr-status-present");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("model.int8.onnx"), b"fake").unwrap();
        fs::write(tmp.join("tokens.txt"), b"a").unwrap();
        std::env::set_var("WORKDANCE_ASR_MODEL_DIR", &tmp);
        let st = AsrBackendStatus::probe();
        assert!(st.model_installed);
        assert!(st.message.contains("模型"));
        std::env::remove_var("WORKDANCE_ASR_MODEL_DIR");
    }

    #[cfg(feature = "sherpa-asr")]
    #[test]
    fn sherpa_try_open_errors_without_model_files() {
        let tmp = std::env::temp_dir().join("workdance-sherpa-missing");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        match SherpaAsr::try_open(&tmp) {
            Ok(_) => panic!("expected missing model error"),
            Err(err) => assert!(err.contains("download-asr-model.sh")),
        }
    }
}
