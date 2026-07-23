use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Генерация рекапа/МоМ/задач (M4.1 паспорта). Реализация —
/// `LocalLlamaProvider` (llama.cpp sidecar, macOS).
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
    /// [M14 T-09 Phase E] Optional GBNF grammar text. Provider may ignore.
    /// `LocalLlamaProvider` пишет в temp файл + передаёт `--grammar-file`;
    /// `AnthropicProvider` игнорирует (proxy validates JSON через API).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
    /// Optional JSON Schema text. Сильнее `grammar`: llama.cpp конвертит схему
    /// в GBNF и форсит ИМЕННО форму (required-поля, enum, массивы). `LocalLlama`
    /// пишет в temp + передаёт `--json-schema-file`; `Anthropic` игнорирует.
    /// Если задано — провайдер использует схему вместо `grammar`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<String>,
}

// Local-only: LLM-провайдер (LocalLlamaProvider) конструирует `Provider` и
// `NotImplemented`. Auth/Network/QuotaExceeded — зарезервированная таксономия
// под будущие внешние интеграции; пока конструируются только в тестах.
#[allow(dead_code)]
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
