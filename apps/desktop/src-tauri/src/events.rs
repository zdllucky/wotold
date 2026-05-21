//! [Phase 4 R8] Typed event publisher для frontend.
//!
//! Все события эмитятся через `EventBus` — без magic-string `handle.emit("...")`
//! callsite'ов. Это:
//! - ловит typo в названии event'а на этапе компиляции (const'ы ниже)
//! - даёт типизированный payload — Serialize-trait проверится компилятором
//! - оставляет один `log::warn!("emit X failed: {e}")` шаблон в одной точке
//!
//! Frontend подписывается через `listen<T>(EVENT_NAME, ...)` — имена матчат
//! литералам в `pub const` ниже.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

// ──────────────────────────────────────────────────────────────
// Event names — const'ы, чтобы компилятор ловил typo.
// ──────────────────────────────────────────────────────────────

pub const PIPELINE_STARTED: &str = "pipeline:started";
pub const PIPELINE_FINISHED: &str = "pipeline:finished";
pub const PIPELINE_CANCELLED: &str = "pipeline:cancelled";
pub const CALL_PROGRESS: &str = "call:progress";
pub const CALL_AUTO_BOUND: &str = "call:auto_bound";
pub const AUDIO_LEVEL: &str = "audio:level";
pub const VOICE_MODEL_PROGRESS: &str = "voice-model:progress";
pub const VOICE_MODEL_DONE: &str = "voice-model:done";

// ──────────────────────────────────────────────────────────────
// Payload types. Re-exported из event-bus, чтобы у frontend
// был один источник истины (через Tauri's `specta` в будущем).
// ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PipelineStartedEvent {
    pub call_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineFinishedEvent {
    pub call_id: String,
    /// `ready` | `failed`
    pub status: &'static str,
    pub failed_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineCancelledEvent {
    pub call_id: String,
    pub artifacts_intact: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallProgressEvent {
    pub call_id: String,
    pub step: u8,
    pub pct: u8,
    pub eta_sec: Option<i64>,
    pub upload_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallAutoBoundEvent {
    pub call_id: String,
    pub count: u64,
    pub threshold_pct: u8,
}

// `audio:level` payload живёт в audio::macos (типобезопасность копий нет).
// EventBus принимает его generic'ом для совместимости.

// ──────────────────────────────────────────────────────────────
// EventBus — wrapper над `AppHandle`. None = headless / tests.
// ──────────────────────────────────────────────────────────────

/// Typed publisher над `AppHandle`. Если handle отсутствует (headless / unit-test)
/// — все методы превращаются в no-op. Это позволяет переиспользовать pipeline
/// и service-layer'ы вне Tauri-окружения.
#[derive(Clone, Copy)]
pub struct EventBus<'a> {
    handle: Option<&'a AppHandle>,
}

impl<'a> EventBus<'a> {
    pub fn new(handle: Option<&'a AppHandle>) -> Self {
        Self { handle }
    }

    fn emit<T: Serialize + Clone>(&self, name: &str, payload: &T) {
        let Some(handle) = self.handle else {
            return;
        };
        if let Err(e) = handle.emit(name, payload) {
            log::warn!("emit {name} failed: {e}");
        }
    }

    pub fn pipeline_started(&self, call_id: &str) {
        self.emit(
            PIPELINE_STARTED,
            &PipelineStartedEvent {
                call_id: call_id.to_string(),
            },
        );
    }

    pub fn pipeline_finished(&self, e: &PipelineFinishedEvent) {
        self.emit(PIPELINE_FINISHED, e);
    }

    pub fn pipeline_cancelled(&self, e: &PipelineCancelledEvent) {
        self.emit(PIPELINE_CANCELLED, e);
    }

    pub fn call_progress(&self, e: &CallProgressEvent) {
        self.emit(CALL_PROGRESS, e);
    }

    pub fn call_auto_bound(&self, e: &CallAutoBoundEvent) {
        self.emit(CALL_AUTO_BOUND, e);
    }

    /// `audio:level` payload — `audio::macos::LevelPayload`. Объявлен generic'ом
    /// чтобы не было циклической зависимости events ↔ audio.
    pub fn audio_level<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(AUDIO_LEVEL, payload);
    }

    /// Generic'ом по той же причине (voice_model держит DoneEvent enum приватно).
    pub fn voice_model_progress<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(VOICE_MODEL_PROGRESS, payload);
    }

    pub fn voice_model_done<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(VOICE_MODEL_DONE, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_handle_is_noop() {
        // Без AppHandle все методы — silent no-op. Тест проверяет что вызовы
        // не паникуют и не требуют Tauri runtime.
        let bus = EventBus::new(None);
        bus.pipeline_started("c1");
        bus.pipeline_finished(&PipelineFinishedEvent {
            call_id: "c1".into(),
            status: "ready",
            failed_reason: None,
        });
        bus.pipeline_cancelled(&PipelineCancelledEvent {
            call_id: "c1".into(),
            artifacts_intact: true,
        });
        bus.call_progress(&CallProgressEvent {
            call_id: "c1".into(),
            step: 1,
            pct: 50,
            eta_sec: None,
            upload_bytes: None,
        });
        bus.call_auto_bound(&CallAutoBoundEvent {
            call_id: "c1".into(),
            count: 1,
            threshold_pct: 95,
        });
    }

    #[test]
    fn event_name_constants_match_legacy_strings() {
        // Регрессия: фронтенд подписан на эти строки. Если их менять —
        // ломается UI без compile-time сигнала. Эти assert'ы — guard rail
        // на случай "случайно отрефакторили имя".
        assert_eq!(PIPELINE_STARTED, "pipeline:started");
        assert_eq!(PIPELINE_FINISHED, "pipeline:finished");
        assert_eq!(PIPELINE_CANCELLED, "pipeline:cancelled");
        assert_eq!(CALL_PROGRESS, "call:progress");
        assert_eq!(CALL_AUTO_BOUND, "call:auto_bound");
        assert_eq!(AUDIO_LEVEL, "audio:level");
        assert_eq!(VOICE_MODEL_PROGRESS, "voice-model:progress");
        assert_eq!(VOICE_MODEL_DONE, "voice-model:done");
    }
}
