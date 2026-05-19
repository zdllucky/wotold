use async_trait::async_trait;

use super::{AudioCapture, CaptureError, CaptureResult};

/// macOS-захват через Swift sidecar (Core Audio process tap, образец — AudioTee).
/// Подключается в Этапе 2 паспорта. Сейчас — каркас, чтобы trait был полностью покрыт.
pub struct MacOsCoreAudioCapture {
    // TODO(Этап 2): дескриптор sidecar-процесса, пайпы PCM/WAV, чанковый флаш (M1.5).
}

impl MacOsCoreAudioCapture {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MacOsCoreAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AudioCapture for MacOsCoreAudioCapture {
    async fn start(&self) -> Result<(), CaptureError> {
        Err(CaptureError::Other(
            "macOS audio capture not wired (Этап 2)".into(),
        ))
    }

    async fn stop(&self) -> Result<CaptureResult, CaptureError> {
        Err(CaptureError::Other(
            "macOS audio capture not wired (Этап 2)".into(),
        ))
    }
}
