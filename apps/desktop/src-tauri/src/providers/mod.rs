//! Провайдерные трейты STT/LLM. Cloud-реализации (proxy/BYO) удалены при
//! переходе на local-only — остаются только трейты + типы, которые реализует
//! локальный движок (`LocalWhisperProvider` / `LocalLlamaProvider`) и моки.

pub mod llm;
pub mod transcription;
