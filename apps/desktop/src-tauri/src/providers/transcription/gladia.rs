use std::path::Path;

use async_trait::async_trait;

use super::{DiarizedTranscript, TranscriptionError, TranscriptionOpts, TranscriptionProvider};
use crate::providers::ProviderMode;

/// Gladia — fallback STT (M2.2). Реальная имплементация — #21.
pub struct GladiaProvider {
    pub mode: ProviderMode,
}

#[async_trait]
impl TranscriptionProvider for GladiaProvider {
    async fn transcribe(
        &self,
        _audio_path: &Path,
        _opts: TranscriptionOpts,
    ) -> Result<DiarizedTranscript, TranscriptionError> {
        Err(TranscriptionError::NotImplemented)
    }
}
