use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Единый интерфейс STT с диаризацией. См. M2.1 паспорта.
/// Реализации: SonioxProvider (primary), GladiaProvider (fallback).
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(
        &self,
        audio_path: &Path,
        opts: TranscriptionOpts,
    ) -> Result<DiarizedTranscript, TranscriptionError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionOpts {
    /// 'auto' или BCP 47.
    pub lang: String,
    /// Диаризация всегда включена (M2.4).
    pub diarization: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// На wire (JSON через прокси) — `speakerTag`, в Rust — `speaker_tag` (S2).
    pub speaker_tag: String,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizedTranscript {
    pub version: u32,
    /// На wire — `langDetected`, в Rust — `lang_detected`.
    pub lang_detected: Option<String>,
    /// На wire — `durationSec`.
    pub duration_sec: f64,
    pub provider: String,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptionError {
    #[error("auth: {0}")]
    Auth(String),
    #[error("network: {0}")]
    Network(String),
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("provider: {0}")]
    Provider(String),
    // [B16] Зарезервировано для R4 Windows-impl и stub-провайдеров.
    #[allow(dead_code)]
    #[error("not implemented")]
    NotImplemented,
}

mod gladia;
mod retry;
mod soniox;

pub use gladia::GladiaProvider;
pub use retry::{failure_reason, transcribe_with_fallback, RetryConfig};
pub use soniox::SonioxProvider;
