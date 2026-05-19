use async_trait::async_trait;

use super::{AudioCapture, CaptureError, CaptureResult};

/// Windows-захват — заглушка (R4 паспорта). UI показывает «недоступно на этой платформе».
pub struct WindowsWasapiCapture;

#[async_trait]
impl AudioCapture for WindowsWasapiCapture {
    async fn start(&self) -> Result<(), CaptureError> {
        Err(CaptureError::NotImplemented)
    }

    async fn stop(&self) -> Result<CaptureResult, CaptureError> {
        Err(CaptureError::NotImplemented)
    }
}
