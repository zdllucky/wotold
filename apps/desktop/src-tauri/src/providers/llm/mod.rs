use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Генерация рекапа/МоМ/задач (M4.1 паспорта). Реализация — Anthropic.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn generate(&self, request: LlmRequest) -> Result<serde_json::Value, LlmError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: Option<String>,
    pub system: String,
    pub input: String,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
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

mod anthropic;

pub use anthropic::AnthropicProvider;
