//! MediaPipe Tasks Hand Landmarker. Feature `mediapipe`.
//!
//! Loads `libmediapipe` at runtime (`MEDIAPIPE_LIB` or a known cache path).
//! The feature compiles without the native library present.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;
use std::time::Instant;

use libloading::Library;

use crate::frame::{FrameBuffer, HandObservation, Landmark, HAND_LANDMARKS};

pub const HAND_LANDMARKER_URL: &str =
    "https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task";

pub const HAND_LANDMARKER_SHA256: &str =
    "fbc2a30080c3c557093b5ddfc334698132eb341044ccee322ccf8bcf3607cde1";

/// Directory of the running executable (`.app/Contents/MacOS` on macOS).
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// Bundle-relative model locations (Tauri resource dir = `Contents/Resources`).
fn bundle_model_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let res = exe_dir.join("..").join("Resources");
    vec![
        res.join("models").join("hand_landmarker.task"),
        res.join("hand_landmarker.task"),
        exe_dir.join("models").join("hand_landmarker.task"),
    ]
}

/// Lookup order: `WORKDANCE_HAND_MODEL` → `WORKDANCE_MODEL_DIR` → bundled
/// resource (first that exists) → per-user data dir. Explicit env wins even
/// when missing so a wrong override fails loudly in `open`.
fn resolve_model_path(
    env_model: Option<String>,
    env_dir: Option<String>,
    exe_dir: Option<&Path>,
    data_local: Option<PathBuf>,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(p) = env_model {
        return PathBuf::from(p);
    }
    if let Some(dir) = env_dir {
        return PathBuf::from(dir).join("hand_landmarker.task");
    }
    if let Some(dir) = exe_dir {
        if let Some(hit) = bundle_model_candidates(dir).into_iter().find(|p| exists(p)) {
            return hit;
        }
    }
    data_local
        .unwrap_or_else(|| PathBuf::from("."))
        .join("workdance")
        .join("models")
        .join("hand_landmarker.task")
}

pub fn default_hand_model_path() -> PathBuf {
    resolve_model_path(
        non_empty_env("WORKDANCE_HAND_MODEL"),
        non_empty_env("WORKDANCE_MODEL_DIR"),
        exe_dir().as_deref(),
        dirs::data_local_dir(),
        &|p: &Path| p.is_file(),
    )
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MpBaseOptionsV35 {
    model_asset_buffer: *const c_char,
    model_asset_buffer_count: c_uint,
    model_asset_path: *const c_char,
    delegate: c_int,
    host_environment: c_int,
    host_system: c_int,
    host_version: *const c_char,
    ca_bundle_path: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MpHandLandmarkerOptionsV35 {
    base_options: MpBaseOptionsV35,
    running_mode: c_int,
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
    *const c_void,
    i64,
    *mut MpHandLandmarkerResult,
    *mut *mut c_char,
) -> MpStatus;
type FnCloseResult = unsafe extern "C" fn(*mut MpHandLandmarkerResult);
type FnClose = unsafe extern "C" fn(MpHandLandmarkerPtr, *mut *mut c_char) -> MpStatus;
type FnImageCreate = unsafe extern "C" fn(
    c_int,
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

impl MpApi {
    unsafe fn load(path: &Path) -> Result<Self, String> {
        let lib = Library::new(path).map_err(|e| format!("dlopen {}: {e}", path.display()))?;
        Ok(Self {
            create: *lib
                .get(b"MpHandLandmarkerCreate\0")
                .map_err(|e| format!("MpHandLandmarkerCreate: {e}"))?,
            detect_for_video: *lib
                .get(b"MpHandLandmarkerDetectForVideo\0")
                .map_err(|e| format!("MpHandLandmarkerDetectForVideo: {e}"))?,
            close_result: *lib
                .get(b"MpHandLandmarkerCloseResult\0")
                .map_err(|e| format!("MpHandLandmarkerCloseResult: {e}"))?,
            close: *lib
                .get(b"MpHandLandmarkerClose\0")
                .map_err(|e| format!("MpHandLandmarkerClose: {e}"))?,
            image_create: *lib
                .get(b"MpImageCreateFromUint8Data\0")
                .map_err(|e| format!("MpImageCreateFromUint8Data: {e}"))?,
            image_free: *lib
                .get(b"MpImageFree\0")
                .map_err(|e| format!("MpImageFree: {e}"))?,
            error_free: lib.get(b"MpErrorFree\0").ok().map(|s| *s),
            _lib: lib,
        })
    }

    fn free_err(&self, msg: *mut c_char) {
        if msg.is_null() {
            return;
        }
        if let Some(f) = self.error_free {
            unsafe { f(msg) };
        } else {
            unsafe { free(msg as *mut c_void) };
        }
    }
}

fn lib_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "mediapipe.dll"
    } else if cfg!(target_os = "macos") {
        "libmediapipe.dylib"
    } else {
        "libmediapipe.so"
    }
}

/// Order: `MEDIAPIPE_LIB` → `<exe>/../Frameworks` → `<exe>/../Resources` →
/// `<exe>/` → cache dirs → system paths.
fn lib_candidates_from(
    env_lib: Option<String>,
    exe_dir: Option<&Path>,
    cache: Option<PathBuf>,
) -> Vec<PathBuf> {
    let name = lib_file_name();
    let mut out = Vec::new();
    if let Some(p) = env_lib {
        out.push(PathBuf::from(p));
    }
    if let Some(dir) = exe_dir {
        out.push(dir.join("..").join("Frameworks").join(name));
        out.push(dir.join("..").join("Resources").join(name));
        out.push(dir.join(name));
    }
    if let Some(cache) = cache {
        out.push(cache.join("mediapipe-rs").join("0.10.35").join(name));
        out.push(cache.join("mediapipe-rs").join(name));
        out.push(cache.join("mediapipe").join(name));
    }
    if cfg!(target_os = "macos") {
        out.push(PathBuf::from("/usr/local/lib/libmediapipe.dylib"));
        out.push(PathBuf::from("/opt/homebrew/lib/libmediapipe.dylib"));
    } else if cfg!(target_os = "windows") {
        out.push(PathBuf::from("mediapipe.dll"));
    } else {
        out.push(PathBuf::from("/usr/local/lib/libmediapipe.so"));
        out.push(PathBuf::from("libmediapipe.so"));
    }
    out
}

fn candidate_lib_paths() -> Vec<PathBuf> {
    lib_candidates_from(non_empty_env("MEDIAPIPE_LIB"), exe_dir().as_deref(), dirs::cache_dir())
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
    let tried: Vec<String> = candidate_lib_paths()
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    Err(format!(
        "libmediapipe not found (set MEDIAPIPE_LIB). Tried: {}. {}",
        tried.join(", "),
        errors.join("; ")
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

pub struct MediaPipeHands {
    api: Arc<MpApi>,
    landmarker: MpHandLandmarkerPtr,
    _model_path: CString,
    started: Instant,
    last_timestamp_ms: i64,
}

impl MediaPipeHands {
    pub fn open(model_path: &Path) -> Result<Self, String> {
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
                delegate: 0,
                host_environment: 0,
                host_system: 0,
                host_version: ptr::null(),
                ca_bundle_path: ptr::null(),
            },
            running_mode: 2,
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

    /// Continuous 21-landmark detect. `t_ms` is the gate clock, not the MediaPipe timestamp.
    pub fn detect(&mut self, frame: &FrameBuffer, t_ms: u64) -> HandObservation {
        let mut image: MpImagePtr = ptr::null_mut();
        let mut err: *mut c_char = ptr::null_mut();
        let status = unsafe {
            (self.api.image_create)(
                1,
                frame.width as c_int,
                frame.height as c_int,
                frame.rgb.as_ptr(),
                frame.rgb.len() as c_int,
                &mut image,
                &mut err,
            )
        };
        if status != 0 || image.is_null() {
            log_err("MpImageCreate", &self.api, err);
            return HandObservation::absent(t_ms);
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
            log_err("DetectForVideo", &self.api, err);
            return HandObservation::absent(t_ms);
        }
        let obs = observation_from_result(&result, t_ms);
        unsafe { (self.api.close_result)(&mut result) };
        obs
    }
}

fn log_err(what: &str, api: &MpApi, err: *mut c_char) {
    eprintln!("[workdance-gesture] {what}: {}", take_c_err(api, err));
}

fn observation_from_result(result: &MpHandLandmarkerResult, t_ms: u64) -> HandObservation {
    let Some(landmarks) = extract_landmarks(result) else {
        return HandObservation::absent(t_ms);
    };
    let confidence = handedness_score(result).unwrap_or(0.9);
    HandObservation::with_landmarks(t_ms, landmarks, confidence.clamp(0.0, 1.0))
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

fn extract_landmarks(result: &MpHandLandmarkerResult) -> Option<[Landmark; HAND_LANDMARKS]> {
    if result.hand_landmarks_count == 0 || result.hand_landmarks.is_null() {
        return None;
    }
    unsafe {
        let set = &*result.hand_landmarks;
        if set.landmarks.is_null() || set.landmarks_count < HAND_LANDMARKS as u32 {
            return None;
        }
        let mut out = [Landmark::new(0.0, 0.0, 0.0); HAND_LANDMARKS];
        for i in 0..HAND_LANDMARKS {
            let lm = &*set.landmarks.add(i);
            out[i] = Landmark::new(lm.x, lm.y, lm.z);
        }
        Some(out)
    }
}

impl Drop for MediaPipeHands {
    fn drop(&mut self) {
        if self.landmarker.is_null() {
            return;
        }
        let mut err: *mut c_char = ptr::null_mut();
        unsafe { (self.api.close)(self.landmarker, &mut err) };
        if !err.is_null() {
            self.api.free_err(err);
        }
        self.landmarker = ptr::null_mut();
    }
}

unsafe impl Send for MediaPipeHands {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_is_explicit() {
        let err = match MediaPipeHands::open(Path::new("/no/such/hand_landmarker.task")) { Err(e) => e, Ok(_) => panic!("expected error") };
        assert!(err.contains("missing"), "{err}");
    }

    #[test]
    fn pinned_model_constants() {
        assert!(HAND_LANDMARKER_URL.contains("hand_landmarker.task"));
        assert_eq!(HAND_LANDMARKER_SHA256.len(), 64);
    }

    #[test]
    fn lib_lookup_order_env_then_bundle_then_cache() {
        let exe = Path::new("/A.app/Contents/MacOS");
        let v = lib_candidates_from(
            Some("/env/libmp".into()),
            Some(exe),
            Some(PathBuf::from("/cache")),
        );
        let n = lib_file_name();
        assert_eq!(v[0], PathBuf::from("/env/libmp"));
        assert_eq!(v[1], exe.join("..").join("Frameworks").join(n));
        assert_eq!(v[2], exe.join("..").join("Resources").join(n));
        assert_eq!(v[3], exe.join(n));
        assert_eq!(v[4], PathBuf::from("/cache/mediapipe-rs/0.10.35").join(n));
    }

    #[test]
    fn lib_lookup_without_env_starts_at_frameworks() {
        let exe = Path::new("/A.app/Contents/MacOS");
        let v = lib_candidates_from(None, Some(exe), None);
        assert_eq!(v[0], exe.join("..").join("Frameworks").join(lib_file_name()));
    }

    #[test]
    fn model_env_override_wins_even_if_bundle_exists() {
        let exe = Path::new("/A.app/Contents/MacOS");
        let p = resolve_model_path(Some("/x.task".into()), None, Some(exe), None, &|_| true);
        assert_eq!(p, PathBuf::from("/x.task"));
        let p = resolve_model_path(None, Some("/d".into()), Some(exe), None, &|_| true);
        assert_eq!(p, PathBuf::from("/d/hand_landmarker.task"));
    }

    #[test]
    fn model_bundle_resource_before_data_dir() {
        let exe = Path::new("/A.app/Contents/MacOS");
        let want = exe.join("..").join("Resources").join("models").join("hand_landmarker.task");
        let w = want.clone();
        let p = resolve_model_path(None, None, Some(exe), Some("/data".into()), &move |q| q == w);
        assert_eq!(p, want);
    }

    #[test]
    fn model_falls_back_to_data_dir_when_bundle_missing() {
        let exe = Path::new("/A.app/Contents/MacOS");
        let p = resolve_model_path(None, None, Some(exe), Some("/data".into()), &|_| false);
        assert_eq!(p, PathBuf::from("/data/workdance/models/hand_landmarker.task"));
    }
}
