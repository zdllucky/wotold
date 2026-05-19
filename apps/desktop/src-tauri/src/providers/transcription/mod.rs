use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Единый интерфейс STT с диаризацией. См. M2.1 паспорта.
/// Реализации: SonioxProvider (primary), GladiaProvider (fallback) — Этап 3.
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(
        &self,
        audio: TranscriptionInput,
        opts: TranscriptionOpts,
    ) -> Result<DiarizedTranscript, TranscriptionError>;
}

#[derive(Debug, Clone)]
pub enum TranscriptionInput {
    /// Локальный файл (BYO-путь).
    FilePath(std::path::PathBuf),
    /// R2-ключ для managed-пути через прокси.
    R2Key(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionOpts {
    /// 'auto' или BCP 47.
    pub lang: String,
    /// Диаризация всегда включена (M2.4).
    pub diarization: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub speaker_tag: String,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiarizedTranscript {
    pub version: u32,
    pub lang_detected: Option<String>,
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
    #[error("not implemented")]
    NotImplemented,
}

mod gladia;
mod soniox;

pub use gladia::GladiaProvider;
pub use soniox::SonioxProvider;
