use async_trait::async_trait;

use super::{LlmError, LlmProvider, LlmRequest};
use crate::providers::ProviderMode;

/// AnthropicProvider — реализация LLM (managed через прокси или BYO).
/// Реальная имплементация — Этап 5.
pub struct AnthropicProvider {
    pub mode: ProviderMode,
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn generate(&self, _request: LlmRequest) -> Result<serde_json::Value, LlmError> {
        Err(LlmError::NotImplemented)
    }
}
