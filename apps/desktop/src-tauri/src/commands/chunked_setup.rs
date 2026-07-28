//! [TD-41] Обвязка chunked-записи: настройки STT, провайдеры чанка и запуск
//! оркестратора ротации.
//!
//! Выделено из `commands/recording.rs` (1426 строк при лимите 800, правило 8)
//! по фазе: здесь всё, что нужно, чтобы завести конвейер чанков, — но не
//! команды жизненного цикла записи. Логика не менялась.

use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::{AppHandle, State};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::{
    audio::macos::{self as audio_macos, OrchestratorChannels, RecordingSession},
    call_id::CallId,
    call_store::CallStore,
    db,
    local_engine::{
        preset::{LocalEnginePreset, SETTING_ACTIVE_PRESET},
        stt::{LocalWhisperProvider, TrackKind},
    },
    pipeline::settings::{mic_diarization_enabled, read_num_speakers_override},
    pipeline::{
        chunk_orchestrator::{self, ChunkOrchestratorConfig},
        chunk_runner::{self, ChunkRunInput},
    },
    providers::transcription::TranscriptionProvider,
    state::AppState,
    AppError,
};

/// [M13.1.5c] Settings key для feature flag. См. PRD §M13.1.5.
const SETTING_CHUNKED_PIPELINE: &str = "recording.chunked_pipeline";

/// [M13 fix] Выбор путей, в которые sidecar физически пишет первый chunk.
/// chunked → `chunks/0/{mic,system}.wav`; non-chunked → root `{mic,system}.wav`.
/// Pure (без side-effects); создание `chunks/0/` — на caller'е (ensure_chunk_dir).
pub(crate) fn sidecar_write_paths(
    store: &CallStore,
    call_id: &CallId,
    chunked: bool,
) -> (std::path::PathBuf, std::path::PathBuf) {
    if chunked {
        (
            store.chunk_mic_path(call_id, 0),
            store.chunk_system_path(call_id, 0),
        )
    } else {
        (store.mic_path(call_id), store.system_path(call_id))
    }
}

/// [M13 fix] Собранные provider'ы + STT-настройки для одного chunk-прогона.
/// Shared между `prepare_chunked_setup` (live orchestrator), `retry_chunk`,
/// финальным-chunk путём на stop и recovery — чтобы все они STT'или chunk'и
/// одинаково (тот же preset/lang/diarization).
pub(crate) struct ChunkProviders {
    pub mic: Arc<dyn TranscriptionProvider>,
    pub system: Arc<dyn TranscriptionProvider>,
    pub lang: String,
    pub mic_diarization: bool,
    pub mic_diarization_num_speakers: Option<i32>,
}

/// [M13 fix] Построить `ChunkProviders` из active preset + settings.
/// НЕ проверяет engine (caller решает: `prepare_chunked_setup` возвращает
/// `Ok(None)` при non-local, `retry_chunk`/recovery — `Err`). `Err` при
/// отсутствии preset (модель не выбрана).
pub(crate) async fn build_chunk_providers(
    db: &SqlitePool,
    app_data_dir: &std::path::Path,
    app: &AppHandle,
    // [Q] call_id → STT-очередь (QueueMonitor видит чей звонок у whisper'а).
    call_id: &CallId,
) -> Result<ChunkProviders, AppError> {
    let preset = db::get_setting(db, SETTING_ACTIVE_PRESET)
        .await?
        .as_deref()
        .and_then(LocalEnginePreset::from_str)
        .ok_or_else(|| {
            AppError::Other(
                "local_engine_preset_not_set: выберите Light/Balanced/Quality в Settings".into(),
            )
        })?;
    let whisper_id = preset.whisper_model_id();
    // TrackKind влияет на дефолтные speaker tags ("owner" для mic, "speaker:0"
    // для system) — их потом переcassign'ает cluster pipeline.
    let mic = LocalWhisperProvider::for_preset(app_data_dir, whisper_id, TrackKind::MicOwner)
        .with_call(call_id.as_str())
        .with_app(app.clone())
        .await;
    let system = LocalWhisperProvider::for_preset(app_data_dir, whisper_id, TrackKind::System)
        .with_call(call_id.as_str())
        .with_app(app.clone())
        .await;
    let mic: Arc<dyn TranscriptionProvider> = Arc::new(mic);
    let system: Arc<dyn TranscriptionProvider> = Arc::new(system);

    let lang = db::get_setting(db, "stt_lang")
        .await?
        .unwrap_or_else(|| "auto".to_string());
    // [P-fix7] Mic diarization — Default OFF (mic = микрофон владельца, M2.4).
    // [TD-36] Обе настройки читаются общим типизированным путём — клэмп и
    // разбор живут в одном месте с локальным маршрутом.
    let mic_diarization = mic_diarization_enabled(db).await?;
    let mic_diarization_num_speakers = read_num_speakers_override(db).await?;

    Ok(ChunkProviders {
        mic,
        system,
        lang,
        mic_diarization,
        mic_diarization_num_speakers,
    })
}

/// [M13 fix / test] Config для orchestrator. По умолчанию 10-мин chunks. Env
/// `WOTOLD_CHUNK_WINDOW_MS=<ms>` (≥2000) ужимает окно для быстрой E2E-проверки
/// (`window`/`tick`/`retention` масштабируются от target). Prod env не задаёт.
fn orchestrator_config_from_env() -> ChunkOrchestratorConfig {
    let base = ChunkOrchestratorConfig::default();
    match std::env::var("WOTOLD_CHUNK_WINDOW_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    {
        Some(target) if target >= 2000 => {
            let half = (target / 10).max(200);
            log::warn!("chunk_orchestrator: WOTOLD_CHUNK_WINDOW_MS override → target={target}ms (test-only)");
            ChunkOrchestratorConfig {
                target_chunk_ms: target,
                window_start_offset_ms: target.saturating_sub(half),
                window_end_offset_ms: target + half,
                tick_interval_ms: (target / 10).max(500),
                rms_retention_ms: target * 2,
                ..base
            }
        }
        _ => base,
    }
}

/// Owned bundle для setup chunked pipeline'а — channels + provider + stop_tx.
/// Создаётся в `prepare_chunked_setup`, разбирается в `spawn_orchestrator`.
pub(crate) struct ChunkedSetup {
    pub(crate) channels: OrchestratorChannels,
    pub(crate) rms_rx: mpsc::Receiver<(u64, f32)>,
    pub(crate) rotate_rx: mpsc::Receiver<serde_json::Value>,
    pub(crate) stop_rx: oneshot::Receiver<()>,
    /// [M13 review fix] Moved в AppState в `spawn_orchestrator` чтобы пережить
    /// возврат функции. Иначе оригинальный `_stop_tx` дропался → `stop_rx`
    /// сразу closed → orchestrator exit преждевременно.
    pub(crate) stop_tx: oneshot::Sender<()>,
    /// [M13.2.1] Pause/resume control. Pause-aware orchestrator skip'ает
    /// rotation timer пока запись на pause. `pause_tx` moved в AppState
    /// в `spawn_orchestrator`.
    pub(crate) pause_tx: mpsc::Sender<bool>,
    pub(crate) pause_rx: mpsc::Receiver<bool>,
    mic_provider: Arc<dyn TranscriptionProvider>,
    system_provider: Arc<dyn TranscriptionProvider>,
    stt_lang: String,
    /// [M13 follow-up] Sortformer на mic для multi-voice. Default ON.
    mic_diarization: bool,
    /// [P1.2] Labs «Force N speakers» override. None = auto-detect.
    mic_diarization_num_speakers: Option<i32>,
}

/// Прочитать settings + (если оба условия true) построить provider + channels.
/// Возвращает `Ok(None)` если chunked не активирован — это норм happy path.
/// `Err` зарезервирован для config errors которые имеет смысл log'нуть.
pub(crate) async fn prepare_chunked_setup(
    app: &AppHandle,
    state: &State<'_, AppState>,
    call_id: &CallId,
) -> Result<Option<ChunkedSetup>, AppError> {
    let _ = call_id;

    // [M13.3.4] Default ON. Explicit "0" / "false" — escape hatch (debug / поддержка).
    // Missing setting (`None`) — новый юзер либо existing-без-explicit-toggle — ON.
    let chunked_off = matches!(
        db::get_setting(&state.db, SETTING_CHUNKED_PIPELINE)
            .await?
            .as_deref(),
        Some("0") | Some("false")
    );
    let chunked_on = !chunked_off;
    if !chunked_on {
        return Ok(None);
    }

    // Build LocalWhisperProvider mirror'ом логики pipeline::run.
    let ChunkProviders {
        mic: mic_provider,
        system: system_provider,
        lang: stt_lang,
        mic_diarization,
        mic_diarization_num_speakers,
    } = build_chunk_providers(&state.db, &state.app_data_dir, app, call_id).await?;

    let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(256);
    let (rotate_tx, rotate_rx) = mpsc::channel::<serde_json::Value>(8);
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    // [M13.2.1] pause/resume control channel. Buffer 8 — burst tolerance
    // для рук-сёрфингов pause/resume в UI; orchestrator drain'ит мгновенно.
    let (pause_tx, pause_rx) = mpsc::channel::<bool>(8);

    Ok(Some(ChunkedSetup {
        channels: OrchestratorChannels { rms_tx, rotate_tx },
        rms_rx,
        rotate_rx,
        stop_rx,
        stop_tx,
        pause_tx,
        pause_rx,
        mic_provider,
        system_provider,
        stt_lang,
        mic_diarization,
        mic_diarization_num_speakers,
    }))
}

/// Spawn'нуть chunk_orchestrator task с rotate_fn + enqueue_fn closures.
/// Handle store'ится в `AppState.orchestrator`.
pub(crate) async fn spawn_orchestrator(
    state: &State<'_, AppState>,
    app: &AppHandle,
    call_id: &CallId,
    setup: ChunkedSetup,
) {
    let ChunkedSetup {
        channels: _,
        rms_rx,
        rotate_rx,
        stop_rx,
        stop_tx,
        pause_tx,
        pause_rx,
        mic_provider,
        system_provider,
        stt_lang,
        mic_diarization,
        mic_diarization_num_speakers,
    } = setup;

    let session_ref = state.recording.clone();
    let pool = state.db.clone();
    let store = state.store.clone();
    let call_id_str = call_id.to_string();

    let rotate_fn = make_rotate_fn(call_id.clone(), store.clone(), session_ref);
    let enqueue_fn = make_enqueue_fn(
        call_id.clone(),
        pool.clone(),
        store.clone(),
        mic_provider,
        system_provider,
        stt_lang,
        // [M13.2.1] app_data_dir для embedder resolve'а внутри chunk_runner;
        // app_handle — для emit'а transcript:chunk_done event.
        state.app_data_dir.clone(),
        app.clone(),
        // [M13 follow-up] Sortformer на mic-дорожке per-chunk.
        mic_diarization,
        // [P1.2] Labs «Force N speakers» override; None = auto.
        mic_diarization_num_speakers,
    );

    let handle = tauri::async_runtime::spawn(async move {
        let summary = chunk_orchestrator::run(
            orchestrator_config_from_env(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            rotate_fn,
            enqueue_fn,
        )
        .await;
        log::info!(
            "chunk_orchestrator finished: rotations={} chunks_done={} rotate_err={} enqueue_err={}",
            summary.rotations_triggered,
            summary.chunks_completed,
            summary.rotate_errors,
            summary.enqueue_errors,
        );
        summary
    });

    *state.orchestrator.lock().await = Some(handle);
    *state.orchestrator_pause_tx.lock().await = Some(pause_tx);
    *state.orchestrator_stop_tx.lock().await = Some(stop_tx);
    log::info!("chunk_orchestrator spawned for call {call_id_str}");
}

/// Closure factory для rotate-callback. Lock'аем `state.recording` Mutex чтобы
/// получить `&mut RecordingSession` для `audio_macos::rotate`.
fn make_rotate_fn(
    call_id: CallId,
    store: Arc<CallStore>,
    session: Arc<Mutex<Option<RecordingSession>>>,
) -> impl Fn(u32) -> chunk_orchestrator::RotateFut + Send + Sync + 'static {
    move |chunk_idx_closing| {
        let call_id = call_id.clone();
        let store = store.clone();
        let session = session.clone();
        Box::pin(async move {
            let next_idx = chunk_idx_closing + 1;
            store
                .ensure_chunk_dir(&call_id, next_idx)
                .await
                .map_err(|e| format!("ensure_chunk_dir({next_idx}): {e}"))?;
            let next_mic = store.chunk_mic_path(&call_id, next_idx);
            let next_system = store.chunk_system_path(&call_id, next_idx);
            let mut guard = session.lock().await;
            let Some(s) = guard.as_mut() else {
                return Err("recording session gone before rotate".to_string());
            };
            audio_macos::rotate(s, next_mic, next_system)
                .await
                .map_err(|e| format!("audio rotate: {e}"))
        })
    }
}

/// Closure factory для enqueue-callback. Pre-insert'ит chunk row, потом
/// запускает `chunk_runner::run_chunk`. Возвращает transcript_tail для
/// prev_prompt следующего chunk'а.
#[allow(clippy::too_many_arguments)]
fn make_enqueue_fn(
    call_id: CallId,
    pool: SqlitePool,
    store: Arc<CallStore>,
    mic_provider: Arc<dyn TranscriptionProvider>,
    system_provider: Arc<dyn TranscriptionProvider>,
    lang: String,
    app_data_dir: std::path::PathBuf,
    app_handle: AppHandle,
    mic_diarization: bool,
    mic_diarization_num_speakers: Option<i32>,
) -> impl Fn(u32, u64, u64, Option<String>) -> chunk_orchestrator::EnqueueFut + Send + Sync + 'static
{
    move |chunk_idx, start_ms, end_ms, prev_prompt| {
        let call_id = call_id.clone();
        let pool = pool.clone();
        let store = store.clone();
        let mic_provider = mic_provider.clone();
        let system_provider = system_provider.clone();
        let lang = lang.clone();
        let app_data_dir = app_data_dir.clone();
        let app_handle = app_handle.clone();
        let mic_diarization = mic_diarization;
        let mic_diarization_num_speakers = mic_diarization_num_speakers;
        Box::pin(async move {
            let mic_path = store.chunk_mic_path(&call_id, chunk_idx);
            let system_path = store.chunk_system_path(&call_id, chunk_idx);
            db::chunks::insert_chunk(
                &pool,
                call_id.as_str(),
                chunk_idx,
                start_ms,
                &mic_path,
                &system_path,
            )
            .await
            .map_err(|e| format!("insert_chunk({chunk_idx}): {e}"))?;

            let input = ChunkRunInput {
                call_id: call_id.as_str().to_string(),
                chunk_idx,
                start_ms,
                end_ms,
                mic_path,
                system_path,
                prev_prompt,
                lang: lang.clone(),
                app_data_dir: Some(app_data_dir.clone()),
                app_handle: Some(app_handle.clone()),
                mic_diarization,
                mic_diarization_num_speakers,
            };
            let out = chunk_runner::run_chunk(
                &pool,
                mic_provider.as_ref(),
                system_provider.as_ref(),
                input,
            )
            .await
            .map_err(|e| format!("run_chunk({chunk_idx}): {e}"))?;
            Ok(Some(out.transcript_tail))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_write_paths_chunked_uses_chunk0() {
        let store = CallStore::new(std::path::PathBuf::from("/data"));
        let (mic, sys) = sidecar_write_paths(&store, &CallId::from_db("c1"), true);
        assert!(
            mic.ends_with("chunks/0/mic.wav"),
            "chunked mic → chunks/0/, got {}",
            mic.display()
        );
        assert!(sys.ends_with("chunks/0/system.wav"));
    }

    #[test]
    fn sidecar_write_paths_non_chunked_uses_root() {
        let store = CallStore::new(std::path::PathBuf::from("/data"));
        let (mic, sys) = sidecar_write_paths(&store, &CallId::from_db("c1"), false);
        assert!(
            mic.ends_with("c1/mic.wav"),
            "root mic, got {}",
            mic.display()
        );
        assert!(!mic.to_string_lossy().contains("chunks"));
        assert!(sys.ends_with("c1/system.wav"));
    }
}
