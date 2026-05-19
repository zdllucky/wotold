pub mod llm;
pub mod transcription;

/// Путь доставки партнёрского запроса.
/// См. M2.3 (STT) и M4.1 (LLM) паспорта.
#[derive(Debug, Clone)]
pub enum ProviderMode {
    /// Через прокси (ключ владельца + квота по device-id).
    Managed {
        proxy_base_url: String,
        device_id: String,
    },
    /// BYO — ключ пользователя из системного keychain. Прокси не задействован.
    Byo { api_key: String },
}
