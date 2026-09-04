use thiserror::Error;

/// Contiguous RGB8 frame buffer (row-major).
#[derive(Debug, Clone)]
pub struct FrameBuffer {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl FrameBuffer {
    pub fn new(width: u32, height: u32, rgb: Vec<u8>) -> Result<Self, CameraError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(3))
            .ok_or(CameraError::InvalidFrame)?;
        if rgb.len() != expected {
            return Err(CameraError::InvalidFrame);
        }
        Ok(Self { width, height, rgb })
    }

    /// Tiny black placeholder used when no camera is available.
    pub fn placeholder_sleep() -> Self {
        Self {
            width: 160,
            height: 120,
            rgb: vec![0; 160 * 120 * 3],
        }
    }
}

#[derive(Debug, Error)]
pub enum CameraError {
    #[error("camera feature disabled at compile time")]
    FeatureDisabled,
    #[error("no camera device available")]
    NoDevice,
    #[error("failed to open camera: {0}")]
    Open(String),
    #[error("failed to capture frame: {0}")]
    Capture(String),
    #[error("invalid frame buffer")]
    InvalidFrame,
}

/// Camera capture façade. Uses nokhwa when `camera` feature is enabled.
pub struct CameraCapture {
    #[cfg(feature = "camera")]
    inner: Option<NokhwaInner>,
    requested_width: u32,
    requested_height: u32,
}

#[cfg(feature = "camera")]
struct NokhwaInner {
    camera: nokhwa::Camera,
}

impl CameraCapture {
    /// Prefer built-in / default index 0. `low_res` targets sleep-tier capture cost.
    pub fn open_default(low_res: bool) -> Result<Self, CameraError> {
        let (w, h) = if low_res { (320, 240) } else { (640, 480) };
        Self::open_index(0, w, h)
    }

    pub fn open_index(index: u32, width: u32, height: u32) -> Result<Self, CameraError> {
        #[cfg(not(feature = "camera"))]
        {
            let _ = (index, width, height);
            Err(CameraError::FeatureDisabled)
        }
        #[cfg(feature = "camera")]
        {
            use nokhwa::pixel_format::RgbFormat;
            use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
            use nokhwa::Camera;

            let idx = CameraIndex::Index(index);
            let requested =
                RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
            let mut camera =
                Camera::new(idx, requested).map_err(|e| CameraError::Open(e.to_string()))?;
            // Best-effort resolution request; drivers may ignore.
            let _ = camera.set_resolution(nokhwa::utils::Resolution::new(width, height));
            camera
                .open_stream()
                .map_err(|e| CameraError::Open(e.to_string()))?;
            Ok(Self {
                inner: Some(NokhwaInner { camera }),
                requested_width: width,
                requested_height: height,
            })
        }
    }

    pub fn requested_size(&self) -> (u32, u32) {
        (self.requested_width, self.requested_height)
    }

    pub fn grab(&mut self) -> Result<FrameBuffer, CameraError> {
        #[cfg(not(feature = "camera"))]
        {
            Err(CameraError::FeatureDisabled)
        }
        #[cfg(feature = "camera")]
        {
            use nokhwa::pixel_format::RgbFormat;
            let inner = self.inner.as_mut().ok_or(CameraError::NoDevice)?;
            let frame = inner
                .camera
                .frame()
                .map_err(|e| CameraError::Capture(e.to_string()))?;
            let decoded = frame
                .decode_image::<RgbFormat>()
                .map_err(|e| CameraError::Capture(e.to_string()))?;
            let width = decoded.width();
            let height = decoded.height();
            FrameBuffer::new(width, height, decoded.into_raw())
        }
    }
}
