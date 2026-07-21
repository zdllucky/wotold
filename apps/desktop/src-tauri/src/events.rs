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
/// [M13.1.2] Sidecar закрыл текущий chunk WAV и открыл новый. Payload —
/// `{ duration_sec, mic_bytes, system_bytes }` (raw sidecar event). Orchestrator
/// слушает чтобы enqueue'ить pipeline job на закрытый chunk.
pub const AUDIO_ROTATED: &str = "audio:rotated";
/// [M13.2.3] Per-chunk pipeline finished (done или failed). Payload —
/// `ChunkDoneEvent`. Frontend (Phase 3) подпишется и обновит ChunkProgressStrip;
/// Phase 2 — backend-only emit (нет TS-typed listener'а).
pub const TRANSCRIPT_CHUNK_DONE: &str = "transcript:chunk_done";
/// [P1.3] Periodic emit во время local LLM (recap) generation. Payload —
/// `RecapProgressEvent` с `elapsed_sec`. UI рендерит «Пересоздаём… {sec}s».
/// Frequency — 15s; нет percentage signals (llama-cli streaming не parsing'нем).
pub const RECAP_PROGRESS: &str = "recap:progress";
/// [P5.2] Live duration update во время active recording. Fires на
/// каждый sidecar `rotated` event (~раз в 10 мин). Payload —
/// `RecordingDurationEvent { call_id, duration_sec }`. UI HomePage
/// list + CallDetailPage subscribe чтобы не показывать stale «1:56»
/// для 30+ мин активных записей.
pub const RECORDING_DURATION: &str = "recording:duration";
/// [S8] Fires whenever the backend recording session changes — start, stop,
/// pause, resume. Both webviews (main + recording-widget) listen so their
/// `RecordingProvider` мирror гарантированно in sync. Payload пустой —
/// слушатели делают `getRecordingState()` для свежего snapshot'а.
pub const RECORDING_STATE: &str = "recording:state";
pub const VOICE_MODEL_PROGRESS: &str = "voice-model:progress";
pub const VOICE_MODEL_DONE: &str = "voice-model:done";
/// [Q] Снапшот состояния очередей тяжёлых ресурсов (stt/diarization/llm) —
/// эмитится на каждый transition (enqueue/acquire/release). Payload —
/// `resource_queue::QueueStateEvent` (generic emit — без цикла events↔pipeline).
/// UI: QueueMonitor попап в сайдбаре + «в очереди» на странице звонка.
pub const QUEUE_STATE: &str = "queue:state";
/// [F3] Пошаговый прогресс генерации рекапа (thinking-блок в RecapView).
/// Payload — `RecapStepEvent`. Эмитится на каждый шаг chain'а: classify /
/// refine chunk i/N / post_pass / narrative / finalize (local) или единый
/// generate (cloud one-shot + короткий local). UI скрывает блок насовсем
/// на `pipeline:finished`.
pub const RECAP_STEP: &str = "recap:step";
/// [Bulk recap] Прогресс массового пересоздания пустых рекапов. Payload —
/// `{ done, total, call_id }`. Settings progress-strip подписывается.
pub const RECAP_BULK_PROGRESS: &str = "recap:bulk_progress";
/// [Bulk recap] Финал массового регена. Payload — `{ regenerated, failed, cancelled }`.
pub const RECAP_BULK_DONE: &str = "recap:bulk_done";

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

/// [M13.2.3] Один chunk pipeline завершился (mark_chunk_done или
/// mark_chunk_failed). Phase 3 UI рендерит strip с per-chunk статусом.
#[derive(Debug, Clone, Serialize)]
pub struct ChunkDoneEvent {
    pub call_id: String,
    pub chunk_idx: u32,
    /// `done` | `failed`
    pub status: &'static str,
    pub segment_count: usize,
}

/// [P1.3] Periodic emit во время local LLM (recap) generation. UI слушает и
/// показывает elapsed timer в кнопке «Пересоздаём… {sec}s». Cancel'ится через
/// drop tokio task на completion (success | failure | timeout).
#[derive(Debug, Clone, Serialize)]
pub struct RecapProgressEvent {
    pub call_id: String,
    pub elapsed_sec: u64,
}

/// [P5.2] Live duration update во время recording. Fires на sidecar
/// `rotated` event (~раз в 10 мин). UI HomePage / CallDetailPage subscribe
/// и патчат `call.duration_sec` чтобы не показывать stale значение для
/// активных long-recordings.
#[derive(Debug, Clone, Serialize)]
pub struct RecordingDurationEvent {
    pub call_id: String,
    pub duration_sec: i64,
}

/// [F3] Усечённое превью промежуточного результата refine-шага —
/// разворачивается в thinking-блоке UI.
#[derive(Debug, Clone, Serialize)]
pub struct RecapStepPreview {
    /// Текущий title рекапа, усечён до 120 chars.
    pub title: String,
    /// Первые ≤3 key_points, каждый усечён до 120 chars.
    pub key_points: Vec<String>,
}

/// [F3] Один шаг генерации рекапа. UI upsert'ит по `step_idx` и рендерит
/// thinking-блок (см. RecapThinking.tsx); скрывает на `pipeline:finished`.
#[derive(Debug, Clone, Serialize)]
pub struct RecapStepEvent {
    pub call_id: String,
    /// 0-based, стабилен для одного шага (started → done обновляет запись).
    pub step_idx: u32,
    /// 0 = ещё неизвестно (до подсчёта чанков).
    pub total_steps: u32,
    /// `classify` | `refine` | `post_pass` | `narrative` | `finalize` | `generate`
    pub kind: &'static str,
    /// `started` | `done` | `failed`
    pub status: &'static str,
    /// kind=refine: 1-based номер чанка.
    pub chunk_no: Option<u32>,
    pub chunk_total: Option<u32>,
    pub preview: Option<RecapStepPreview>,
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

    /// [M13.2.3] Per-chunk pipeline done/failed.
    pub fn transcript_chunk_done(&self, e: &ChunkDoneEvent) {
        self.emit(TRANSCRIPT_CHUNK_DONE, e);
    }

    /// [P1.3] Periodic emit во время local LLM generation. См.
    /// `pipeline::recap_progress::with_recap_progress_emitter`.
    pub fn recap_progress(&self, e: &RecapProgressEvent) {
        self.emit(RECAP_PROGRESS, e);
    }

    /// [F3] Пошаговый прогресс генерации рекапа (thinking-блок).
    pub fn recap_step(&self, e: &RecapStepEvent) {
        self.emit(RECAP_STEP, e);
    }

    /// [Q] Снапшот очередей ресурсов. Generic (payload живёт в
    /// `pipeline::resource_queue`) — избегаем цикла events ↔ pipeline.
    pub fn queue_state<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(QUEUE_STATE, payload);
    }

    /// [P5.2] Live duration update на sidecar rotated event. UI patch'ит
    /// `call.duration_sec` чтобы не показывать stale значение во время
    /// длинной записи.
    pub fn recording_duration(&self, e: &RecordingDurationEvent) {
        self.emit(RECORDING_DURATION, e);
    }

    /// `audio:level` payload — `audio::macos::LevelPayload`. Объявлен generic'ом
    /// чтобы не было циклической зависимости events ↔ audio.
    pub fn audio_level<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(AUDIO_LEVEL, payload);
    }

    /// [M13.1.2] `audio:rotated` raw sidecar JSON (`{duration_sec, mic_bytes,
    /// system_bytes}`). Generic чтобы избежать events ↔ audio cycle.
    pub fn audio_rotated<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(AUDIO_ROTATED, payload);
    }

    /// Generic'ом по той же причине (voice_model держит DoneEvent enum приватно).
    pub fn voice_model_progress<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(VOICE_MODEL_PROGRESS, payload);
    }

    pub fn voice_model_done<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(VOICE_MODEL_DONE, payload);
    }

    pub fn recap_bulk_progress<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(RECAP_BULK_PROGRESS, payload);
    }

    pub fn recap_bulk_done<T: Serialize + Clone>(&self, payload: &T) {
        self.emit(RECAP_BULK_DONE, payload);
    }

    /// [S8] Notify все webview'ы что backend recording state поменялся.
    /// Слушатели зовут `getRecordingState` чтобы pull fresh snapshot.
    pub fn recording_state_changed(&self) {
        self.emit(RECORDING_STATE, &());
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
        bus.transcript_chunk_done(&ChunkDoneEvent {
            call_id: "c1".into(),
            chunk_idx: 0,
            status: "done",
            segment_count: 12,
        });
        bus.recap_progress(&RecapProgressEvent {
            call_id: "c1".into(),
            elapsed_sec: 30,
        });
        bus.recording_duration(&RecordingDurationEvent {
            call_id: "c1".into(),
            duration_sec: 600,
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
        assert_eq!(AUDIO_ROTATED, "audio:rotated");
        assert_eq!(TRANSCRIPT_CHUNK_DONE, "transcript:chunk_done");
        assert_eq!(RECAP_PROGRESS, "recap:progress");
        assert_eq!(RECAP_STEP, "recap:step");
        assert_eq!(QUEUE_STATE, "queue:state");
        assert_eq!(RECORDING_DURATION, "recording:duration");
        assert_eq!(VOICE_MODEL_PROGRESS, "voice-model:progress");
        assert_eq!(VOICE_MODEL_DONE, "voice-model:done");
    }
}
