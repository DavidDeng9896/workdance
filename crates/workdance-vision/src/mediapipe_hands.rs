//! MediaPipe Tasks Hand Landmarker backend (feature `mediapipe-hands`).
//!
//! Loads `libmediapipe` at runtime via `libloading` (no hard link), so the
//! feature compiles on CI without native libs. Win/Mac: set `MEDIAPIPE_LIB`
//! or rely on a PyPI-extracted `libmediapipe` (see README WP-M1).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;
use std::time::Instant;

use libloading::Library;
use workdance_core::{HandFrame, HandLandmark};

use crate::camera::FrameBuffer;
use crate::detector::{DetectDetail, HandPresenceDetector};
use crate::mirror::mirror_horizontal;

const HAND_LANDMARK_COUNT: usize = 21;

/// Official float16 Hand Landmarker task bundle (no stable lite `.task` on CDN).
pub const HAND_LANDMARKER_URL: &str =
    "https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task";

/// SHA-256 of the pinned URL above (verified 2026-09-04).
pub const HAND_LANDMARKER_SHA256: &str =
    "fbc2a30080c3c557093b5ddfc334698132eb341044ccee322ccf8bcf3607cde1";

pub fn default_mediapipe_model_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("workdance")
        .join("models")
        .join("hand_landmarker.task")
}

// ——— FFI layouts (MediaPipe Tasks C API, v0.10.x / PyPI wheel ABI) ———

#[repr(C)]
#[derive(Clone, Copy)]
struct MpBaseOptionsV35 {
    model_asset_buffer: *const c_char,
    model_asset_buffer_count: c_uint,
    model_asset_path: *const c_char,
    delegate: c_int, // MpDelegate::CPU = 0
    host_environment: c_int,
    host_system: c_int,
    host_version: *const c_char,
    ca_bundle_path: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MpHandLandmarkerOptionsV35 {
    base_options: MpBaseOptionsV35,
    running_mode: c_int, // VIDEO = 2
    num_hands: c_int,
    min_hand_detection_confidence: f32,
    min_hand_presence_confidence: f32,
    min_tracking_confidence: f32,
    result_callback: *mut c_void,
}

#[repr(C)]
struct MpNormalizedLandmark {
    x: f32,
    y: f32,
    z: f32,
    has_visibility: bool,
    visibility: f32,
    has_presence: bool,
    presence: f32,
    name: *mut c_char,
}

#[repr(C)]
struct MpNormalizedLandmarks {
    landmarks: *mut MpNormalizedLandmark,
    landmarks_count: u32,
}

#[repr(C)]
struct MpCategory {
    index: c_int,
    score: f32,
    category_name: *mut c_char,
    display_name: *mut c_char,
}

#[repr(C)]
struct MpCategories {
    categories: *mut MpCategory,
    categories_count: u32,
}

#[repr(C)]
struct MpLandmarks {
    landmarks: *mut c_void,
    landmarks_count: u32,
}

#[repr(C)]
struct MpHandLandmarkerResult {
    handedness: *mut MpCategories,
    handedness_count: u32,
    hand_landmarks: *mut MpNormalizedLandmarks,
    hand_landmarks_count: u32,
    hand_world_landmarks: *mut MpLandmarks,
    hand_world_landmarks_count: u32,
}

#[repr(C)]
struct MpImageProcessingOptions {
    // Unused; pass null to Detect*.
    _unused: [u8; 0],
}

type MpStatus = c_int;
type MpHandLandmarkerPtr = *mut c_void;
type MpImagePtr = *mut c_void;

type FnCreate = unsafe extern "C" fn(
    *mut MpHandLandmarkerOptionsV35,
    *mut MpHandLandmarkerPtr,
    *mut *mut c_char,
) -> MpStatus;
type FnDetectForVideo = unsafe extern "C" fn(
    MpHandLandmarkerPtr,
    MpImagePtr,
    *const MpImageProcessingOptions,
    i64,
    *mut MpHandLandmarkerResult,
    *mut *mut c_char,
) -> MpStatus;
type FnCloseResult = unsafe extern "C" fn(*mut MpHandLandmarkerResult);
type FnClose = unsafe extern "C" fn(MpHandLandmarkerPtr, *mut *mut c_char) -> MpStatus;
type FnImageCreate = unsafe extern "C" fn(
    c_int, // MpImageFormat::SRGB = 1
    c_int,
    c_int,
    *const u8,
    c_int,
    *mut MpImagePtr,
    *mut *mut c_char,
) -> MpStatus;
type FnImageFree = unsafe extern "C" fn(MpImagePtr);
type FnErrorFree = unsafe extern "C" fn(*mut c_char);

struct MpApi {
    _lib: Library,
    create: FnCreate,
    detect_for_video: FnDetectForVideo,
    close_result: FnCloseResult,
    close: FnClose,
    image_create: FnImageCreate,
    image_free: FnImageFree,
    error_free: Option<FnErrorFree>,
}

extern "C" {
    fn free(ptr: *mut c_void);
}

fn libc_free(ptr: *mut c_void) {
    // SAFETY: `ptr` was allocated by MediaPipe / libc `malloc`.
    unsafe { free(ptr) };
}

impl MpApi {
    unsafe fn load(path: &Path) -> Result<Self, String> {
        // SAFETY: caller supplies a MediaPipe shared library path.
        let lib = Library::new(path).map_err(|e| format!("dlopen {}: {e}", path.display()))?;
        let create = *lib
            .get::<FnCreate>(b"MpHandLandmarkerCreate\0")
            .map_err(|e| format!("MpHandLandmarkerCreate: {e}"))?;
        let detect_for_video = *lib
            .get::<FnDetectForVideo>(b"MpHandLandmarkerDetectForVideo\0")
            .map_err(|e| format!("MpHandLandmarkerDetectForVideo: {e}"))?;
        let close_result = *lib
            .get::<FnCloseResult>(b"MpHandLandmarkerCloseResult\0")
            .map_err(|e| format!("MpHandLandmarkerCloseResult: {e}"))?;
        let close = *lib
            .get::<FnClose>(b"MpHandLandmarkerClose\0")
            .map_err(|e| format!("MpHandLandmarkerClose: {e}"))?;
        let image_create = *lib
            .get::<FnImageCreate>(b"MpImageCreateFromUint8Data\0")
            .map_err(|e| format!("MpImageCreateFromUint8Data: {e}"))?;
        let image_free = *lib
            .get::<FnImageFree>(b"MpImageFree\0")
            .map_err(|e| format!("MpImageFree: {e}"))?;
        let error_free = lib.get::<FnErrorFree>(b"MpErrorFree\0").ok().map(|s| *s);
        Ok(Self {
            _lib: lib,
            create,
            detect_for_video,
            close_result,
            close,
            image_create,
            image_free,
            error_free,
        })
    }

    fn free_err(&self, msg: *mut c_char) {
        if msg.is_null() {
            return;
        }
        if let Some(f) = self.error_free {
            unsafe { f(msg) };
        } else {
            libc_free(msg as *mut c_void);
        }
    }
}

fn candidate_lib_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("MEDIAPIPE_LIB") {
        out.push(PathBuf::from(p));
    }
    if let Some(cache) = dirs::cache_dir() {
        // mediapipe-rs / common cache layouts
        out.push(
            cache
                .join("mediapipe-rs")
                .join("0.10.35")
                .join("libmediapipe.so"),
        );
        out.push(cache.join("mediapipe-rs").join("libmediapipe.so"));
        out.push(cache.join("mediapipe").join("libmediapipe.so"));
    }
    #[cfg(target_os = "macos")]
    {
        out.push(PathBuf::from("/usr/local/lib/libmediapipe.dylib"));
        out.push(PathBuf::from("/opt/homebrew/lib/libmediapipe.dylib"));
    }
    #[cfg(target_os = "windows")]
    {
        out.push(PathBuf::from("mediapipe.dll"));
        out.push(PathBuf::from("libmediapipe.dll"));
    }
    #[cfg(target_os = "linux")]
    {
        out.push(PathBuf::from("/usr/local/lib/libmediapipe.so"));
        out.push(PathBuf::from("/usr/lib/libmediapipe.so"));
        out.push(PathBuf::from("libmediapipe.so"));
    }
    out
}

fn load_api() -> Result<Arc<MpApi>, String> {
    let mut errors = Vec::new();
    for path in candidate_lib_paths() {
        if !path.exists() {
            continue;
        }
        match unsafe { MpApi::load(&path) } {
            Ok(api) => return Ok(Arc::new(api)),
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }
    Err(format!(
        "libmediapipe not found/loadable (set MEDIAPIPE_LIB). Tried: {}",
        if errors.is_empty() {
            candidate_lib_paths()
                .into_iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            errors.join("; ")
        }
    ))
}

fn take_c_err(api: &MpApi, msg: *mut c_char) -> String {
    if msg.is_null() {
        return "unknown MediaPipe error".into();
    }
    let s = unsafe { CStr::from_ptr(msg) }
        .to_string_lossy()
        .into_owned();
    api.free_err(msg);
    s
}

/// MediaPipe Hand Landmarker implementing [`HandPresenceDetector`].
pub struct MediaPipeHandLandmarker {
    api: Arc<MpApi>,
    landmarker: MpHandLandmarkerPtr,
    /// Keep model path CString alive for the landmarker lifetime.
    _model_path: CString,
    started: Instant,
    last_timestamp_ms: i64,
}

impl MediaPipeHandLandmarker {
    pub fn try_open(model_path: &Path) -> Result<Self, String> {
        if !model_path.is_file() {
            return Err(format!(
                "MediaPipe hand model missing at {} — run scripts/download-hand-landmarker.sh",
                model_path.display()
            ));
        }
        let api = load_api()?;
        let model_c = CString::new(model_path.to_string_lossy().as_bytes())
            .map_err(|_| "model path contains NUL".to_string())?;

        let mut options = MpHandLandmarkerOptionsV35 {
            base_options: MpBaseOptionsV35 {
                model_asset_buffer: ptr::null(),
                model_asset_buffer_count: 0,
                model_asset_path: model_c.as_ptr(),
                delegate: 0, // CPU
                host_environment: 0,
                host_system: 0,
                host_version: ptr::null(),
                ca_bundle_path: ptr::null(),
            },
            running_mode: 2, // VIDEO
            num_hands: 1,
            min_hand_detection_confidence: 0.5,
            min_hand_presence_confidence: 0.5,
            min_tracking_confidence: 0.5,
            result_callback: ptr::null_mut(),
        };

        let mut landmarker: MpHandLandmarkerPtr = ptr::null_mut();
        let mut err: *mut c_char = ptr::null_mut();
        let status = unsafe { (api.create)(&mut options, &mut landmarker, &mut err) };
        if status != 0 || landmarker.is_null() {
            return Err(format!(
                "MpHandLandmarkerCreate failed: {}",
                take_c_err(&api, err)
            ));
        }

        Ok(Self {
            api,
            landmarker,
            _model_path: model_c,
            started: Instant::now(),
            last_timestamp_ms: -1,
        })
    }

    fn next_timestamp_ms(&mut self) -> i64 {
        let ms = self.started.elapsed().as_millis() as i64;
        if ms <= self.last_timestamp_ms {
            self.last_timestamp_ms += 1;
        } else {
            self.last_timestamp_ms = ms;
        }
        self.last_timestamp_ms
    }

    fn infer(&mut self, frame: &FrameBuffer, detail: DetectDetail) -> HandFrame {
        let mirrored = mirror_horizontal(frame);
        let mut image: MpImagePtr = ptr::null_mut();
        let mut err: *mut c_char = ptr::null_mut();
        let status = unsafe {
            (self.api.image_create)(
                1, // SRGB
                mirrored.width as c_int,
                mirrored.height as c_int,
                mirrored.rgb.as_ptr(),
                mirrored.rgb.len() as c_int,
                &mut image,
                &mut err,
            )
        };
        if status != 0 || image.is_null() {
            eprintln!(
                "[workdance-vision] MpImageCreate failed: {}",
                take_c_err(&self.api, err)
            );
            return HandFrame::absent();
        }

        let ts = self.next_timestamp_ms();
        let mut result = MpHandLandmarkerResult {
            handedness: ptr::null_mut(),
            handedness_count: 0,
            hand_landmarks: ptr::null_mut(),
            hand_landmarks_count: 0,
            hand_world_landmarks: ptr::null_mut(),
            hand_world_landmarks_count: 0,
        };
        err = ptr::null_mut();
        let status = unsafe {
            (self.api.detect_for_video)(
                self.landmarker,
                image,
                ptr::null(),
                ts,
                &mut result,
                &mut err,
            )
        };
        unsafe { (self.api.image_free)(image) };

        if status != 0 {
            eprintln!(
                "[workdance-vision] DetectForVideo failed: {}",
                take_c_err(&self.api, err)
            );
            return HandFrame::absent();
        }

        let hand = result_to_hand_frame(&result, detail);
        unsafe { (self.api.close_result)(&mut result) };
        hand
    }
}

fn result_to_hand_frame(result: &MpHandLandmarkerResult, detail: DetectDetail) -> HandFrame {
    if result.hand_landmarks_count == 0 || result.hand_landmarks.is_null() {
        return HandFrame::absent();
    }

    let conf = handedness_score(result).unwrap_or(0.92);
    let present = conf >= 0.5 || result.hand_landmarks_count > 0;

    let landmarks = match detail {
        DetectDetail::PresenceOnly => None,
        DetectDetail::FullLandmarks => extract_landmarks(result),
    };

    HandFrame {
        present,
        confidence: conf.clamp(0.0, 1.0),
        landmarks,
    }
}

fn handedness_score(result: &MpHandLandmarkerResult) -> Option<f32> {
    if result.handedness.is_null() || result.handedness_count == 0 {
        return None;
    }
    unsafe {
        let cats = &*result.handedness;
        if cats.categories.is_null() || cats.categories_count == 0 {
            return None;
        }
        Some((*cats.categories).score)
    }
}

fn extract_landmarks(result: &MpHandLandmarkerResult) -> Option<[HandLandmark; 21]> {
    unsafe {
        let set = &*result.hand_landmarks;
        if set.landmarks.is_null() || set.landmarks_count < HAND_LANDMARK_COUNT as u32 {
            return None;
        }
        let mut out = [HandLandmark {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }; 21];
        for i in 0..HAND_LANDMARK_COUNT {
            let lm = &*set.landmarks.add(i);
            out[i] = HandLandmark {
                x: lm.x,
                y: lm.y,
                z: lm.z,
            };
        }
        Some(out)
    }
}

impl HandPresenceDetector for MediaPipeHandLandmarker {
    fn name(&self) -> &'static str {
        "mediapipe-hands"
    }

    fn process_frame(&mut self, frame: &FrameBuffer, detail: DetectDetail) -> HandFrame {
        self.infer(frame, detail)
    }
}

impl Drop for MediaPipeHandLandmarker {
    fn drop(&mut self) {
        if !self.landmarker.is_null() {
            let mut err: *mut c_char = ptr::null_mut();
            unsafe {
                (self.api.close)(self.landmarker, &mut err);
            }
            if !err.is_null() {
                self.api.free_err(err);
            }
            self.landmarker = ptr::null_mut();
        }
    }
}

// SAFETY: landmarker is used only from the dedicated vision thread.
unsafe impl Send for MediaPipeHandLandmarker {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_errors_clearly() {
        let err = match MediaPipeHandLandmarker::try_open(Path::new("/no/such/hand_landmarker.task"))
        {
            Ok(_) => panic!("expected missing model error"),
            Err(e) => e,
        };
        assert!(err.contains("missing") || err.contains("download"), "{err}");
    }

    #[test]
    fn pinned_model_constants_are_nonempty() {
        assert!(HAND_LANDMARKER_URL.contains("hand_landmarker.task"));
        assert_eq!(HAND_LANDMARKER_SHA256.len(), 64);
    }
}
