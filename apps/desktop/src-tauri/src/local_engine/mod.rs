//! Локальный движок (M12 PRD) — STT/диаризация/LLM полностью на устройстве.
//!
//! Источник истины: [`docs/M12_LOCAL_ENGINE_PRD.md`](../../../../docs/M12_LOCAL_ENGINE_PRD.md).
//!
//! # Состав
//!
//! - [`models`] — каталог моделей + per-model download / status / delete +
//!   preset switching. Единственная качалка моделей в приложении: голосовой
//!   эмбеддер (WeSpeaker) переехал сюда из отдельного `voice_model.rs`,
//!   переезд файла на диске — [`model_migrate`].
//! - `stt` *(TODO M12.1)* — `LocalWhisperProvider` через sherpa-onnx Whisper.
//! - `diarization` *(TODO M12.2)* — `Diarizer` trait через sherpa-onnx sortformer.
//! - `llm` *(TODO M12.3)* — `LocalLlamaProvider` через llama.cpp sidecar.
//! - `hw_probe` *(TODO M12.7)* — Hardware probe (Apple Silicon detect / RAM /
//!   Metal) + preset recommendation.
//!
//! # Платформа (R9)
//!
//! В MVP local-движок доступен только на macOS. Весь модуль за
//! `#[cfg(target_os = "macos")]`. На Linux / Windows trait готов,
//! реализация = `unimplemented!()` (см. R4 паспорта).

#![cfg(target_os = "macos")]

// [M12 Phase 3] stt + llm + merge — wired into `pipeline::run_local_inner`.
// diarization — стаб с in-process тестами; реальный sherpa sortformer wire-up
// придёт когда model entries добавятся в MODEL_CATALOG. `merge::SPEAKER_OWNER`
// используется в stt.rs hard-coded (single-shot mic provider), но публичная
// функция-комбайн `assemble_transcript` ждёт sortformer.
#[allow(dead_code)]
pub mod diarization;
pub mod hallucination;
pub mod hw_probe;
pub mod llm;
pub mod llm_json;
pub mod llm_prompt;
pub mod llm_server;
#[allow(dead_code)]
pub mod merge;
pub mod model_catalog;
pub mod model_integrity;
pub mod model_migrate;
pub mod models;
pub mod preset;
pub mod readiness;
pub mod sidecar;
pub mod stt;
pub mod whisper_json;
