use async_trait::async_trait;

use super::{
    DiarizedTranscript, TranscriptionError, TranscriptionInput, TranscriptionOpts,
    TranscriptionProvider,
};
use crate::providers::ProviderMode;

/// Soniox — primary STT (M2.2). Реальная имплементация — Этап 3.
pub struct SonioxProvider {
    pub mode: ProviderMode,
}

#[async_trait]
impl TranscriptionProvider for SonioxProvider {
    async fn transcribe(
        &self,
        _audio: TranscriptionInput,
        _opts: TranscriptionOpts,
    ) -> Result<DiarizedTranscript, TranscriptionError> {
        Err(TranscriptionError::NotImplemented)
    }
}
