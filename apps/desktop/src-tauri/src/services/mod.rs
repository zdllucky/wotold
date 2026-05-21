//! [Phase 4 R4] Service layer — бизнес-логика, извлечённая из `commands.rs`.
//!
//! Tauri-commands (`commands/*`) — тонкие адаптеры: parse args, call service,
//! map result. Реальная логика (spawn pipeline + abort + restore SQL,
//! compose markdown) живёт здесь и тестируется изолированно от Tauri runtime.

pub mod export;
pub mod pipeline_runner;
