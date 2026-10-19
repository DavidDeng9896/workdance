//! Default webcam capture. Compiled only with feature `camera`.

use std::fmt;

use crate::frame::FrameBuffer;

#[derive(Debug)]
pub enum CameraError {
    NoDevice,
    Open(String),
    Capture(String),
}

impl fmt::Display for CameraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => write!(f, "no camera device available"),
            Self::Open(msg) => write!(f, "failed to open camera: {msg}"),
            Self::Capture(msg) => write!(f, "failed to capture frame: {msg}"),
        }
    }
}

impl std::error::Error for CameraError {}

pub struct CameraCapture {
    camera: nokhwa::Camera,
}

impl CameraCapture {
    pub fn open_default() -> Result<Self, CameraError> {
        Self::open_index(0, 640, 480)
    }

    pub fn open_index(index: u32, width: u32, height: u32) -> Result<Self, CameraError> {
        use nokhwa::pixel_format::RgbFormat;
        use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
        use nokhwa::Camera;

        let requested =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
        let mut camera = Camera::new(CameraIndex::Index(index), requested)
            .map_err(|e| CameraError::Open(e.to_string()))?;
        let _ = camera.set_resolution(nokhwa::utils::Resolution::new(width, height));
        camera
            .open_stream()
            .map_err(|e| CameraError::Open(e.to_string()))?;
        Ok(Self { camera })
    }

    pub fn next_frame(&mut self) -> Result<FrameBuffer, CameraError> {
        use nokhwa::pixel_format::RgbFormat;
        let frame = self
            .camera
            .frame()
            .map_err(|e| CameraError::Capture(e.to_string()))?;
        let decoded = frame
            .decode_image::<RgbFormat>()
            .map_err(|e| CameraError::Capture(e.to_string()))?;
        let width = decoded.width();
        let height = decoded.height();
        FrameBuffer::new(width, height, decoded.into_raw())
            .ok_or_else(|| CameraError::Capture("frame size mismatch".into()))
    }
}
