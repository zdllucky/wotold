use async_trait::async_trait;

use super::{
    DiarizedTranscript, TranscriptionError, TranscriptionInput, TranscriptionOpts,
    TranscriptionProvider,
};
use crate::providers::ProviderMode;

/// Gladia — fallback STT (M2.2). Реальная имплементация — Этап 3.
pub struct GladiaProvider {
    pub mode: ProviderMode,
}

#[async_trait]
impl TranscriptionProvider for GladiaProvider {
    async fn transcribe(
        &self,
        _audio: TranscriptionInput,
        _opts: TranscriptionOpts,
    ) -> Result<DiarizedTranscript, TranscriptionError> {
        Err(TranscriptionError::NotImplemented)
    }
}
