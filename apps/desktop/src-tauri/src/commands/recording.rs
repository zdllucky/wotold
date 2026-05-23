//! Commands for start/stop recording + audio permissions.

use std::sync::Arc;

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, State};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::{
    audio::macos::{self as audio_macos, OrchestratorChannels, RecordingSession},
    audio::permissions::{self, PermissionsStatus},
    call_store::CallStore,
    db::{self, Call},
    events::EventBus,
    local_engine::{
        engine::{EngineKind, SETTING_ACTIVE_ENGINE},
        preset::{LocalEnginePreset, SETTING_ACTIVE_PRESET},
        stt::{LocalWhisperProvider, TrackKind},
    },
    pipeline::{
        chunk_orchestrator::{self, ChunkOrchestratorConfig},
        chunk_runner::{self, ChunkRunInput},
    },
    providers::transcription::TranscriptionProvider,
    services::pipeline_runner::PipelineRunner,
    state::AppState,
    AppError,
};

/// [M13.1.5c] Settings key для feature flag. См. PRD §M13.1.5.
const SETTING_CHUNKED_PIPELINE: &str = "recording.chunked_pipeline";

#[derive(Debug, Clone, Serialize)]
pub struct RecordingState {
    pub call_id: String,
    pub started_at: String,
    /// [W2] RFC3339 если запись сейчас на паузе, иначе null.
    pub paused_at: Option<String>,
    /// [W2] Накопленная длительность пауз в мс, для elapsed-калькуляций на UI.
    pub paused_total_ms: i64,
}

/// [W2] Снимок DB-полей паузы для построения RecordingState.
async fn pause_snapshot(
    state: &State<'_, AppState>,
    call_id: &str,
) -> Result<(Option<String>, i64), AppError> {
    let call = crate::db::get_call(&state.db, call_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("call {call_id} not found")))?;
    Ok((call.paused_at, call.paused_total_ms))
}

#[tauri::command]
pub async fn get_recording_state(
    state: State<'_, AppState>,
) -> Result<Option<RecordingState>, AppError> {
    let guard = state.recording.lock().await;
    let Some(session) = guard.as_ref() else {
        return Ok(None);
    };
    let call_id = session.call_id.clone();
    let started_at = session.started_at.to_rfc3339();
    // Освобождаем lock до DB-запроса, чтобы pause/resume не блокировались.
    drop(guard);

    let (paused_at, paused_total_ms) = pause_snapshot(&state, &call_id).await?;
    Ok(Some(RecordingState {
        call_id,
        started_at,
        paused_at,
        paused_total_ms,
    }))
}

#[tauri::command]
pub async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<Call, AppError> {
    let mut guard = state.recording.lock().await;
    if guard.is_some() {
        return Err(AppError::Other("recording already in progress".into()));
    }

    // [B16 audit P1]: pre-check разрешений перед попыткой start. Раньше
    // sidecar просто молча fail-stop'ал — юзер видел загадочный «calls failed»
    // через 1-2 секунды. Теперь возвращаем clear AppError → frontend
    // покажет 'Нет разрешения на ...' и направит в Настройки.
    let perms = permissions::check(&app).await?;
    if perms.microphone != "granted" {
        return Err(AppError::Other(
            "Нет разрешения на микрофон. Открой Настройки → Разрешения.".into(),
        ));
    }
    if perms.screen_recording != "granted" {
        return Err(AppError::Other(
            "Нет разрешения на захват системного звука (для записи голоса собеседника в FaceTime, Zoom и т.д.). Открой Настройки → Разрешения.".into(),
        ));
    }

    // M2.3: path_label фиксирует путь доставки на момент создания звонка.
    // По умолчанию managed; переключаемое значение из settings подключим
    // в #20/#21 (когда провайдеры реально начнут вызываться).
    let path_label = "managed";
    let call = crate::db::insert_recording(&state.db, path_label).await?;
    let mic_path = state.store.mic_path(&call.id);
    let system_path = state.store.system_path(&call.id);

    // [M13.1.5c] Если CHUNKED_PIPELINE=ON + engine=local — настраиваем каналы
    // для chunk_orchestrator. Cloud engine ignor'ит флаг (server-side streaming
    // даёт минимальный win от chunking). При любых ошибках setup (preset не
    // задан, модель не скачана) — откатываемся на happy path без chunked.
    let chunked_setup = match prepare_chunked_setup(&app, &state, &call.id).await {
        Ok(setup) => setup,
        Err(e) => {
            log::warn!("chunked_pipeline disabled (setup failed): {e}; falling back to full-file");
            None
        }
    };
    let orchestrator_channels = chunked_setup.as_ref().map(|s| s.channels.clone());

    match audio_macos::start(
        &app,
        call.id.clone(),
        mic_path,
        system_path,
        orchestrator_channels,
    )
    .await
    {
        Ok(session) => {
            *guard = Some(session);
            drop(guard);

            // Spawn orchestrator только если setup succeed'ил — session уже
            // в state.recording, rotate_fn будет lock'ать тот же Mutex.
            if let Some(setup) = chunked_setup {
                spawn_orchestrator(&state, &app, &call.id, setup).await;
            }

            EventBus::new(Some(&app)).recording_state_changed();
            Ok(call)
        }
        Err(e) => {
            // Откат: помечаем call как failed чтобы он не висел в "recording" навсегда.
            let _ = crate::db::fail_recording(&state.db, &call.id).await;
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<Call, AppError> {
    let session = {
        let mut guard = state.recording.lock().await;
        guard
            .take()
            .ok_or_else(|| AppError::Other("not recording".into()))?
    };

    let call_id = session.call_id.clone();
    let mic_path = session.mic_path.clone();
    let system_path = session.system_path.clone();

    // [M13 review fix] Signal stop через oneshot — это каноничный path для
    // orchestrator exit. До fix'а `stop_tx` дропался в `spawn_orchestrator`
    // → `stop_rx` сразу closed → premature exit. Теперь stop_tx живёт в
    // AppState и здесь take()→send() даёт чистый shutdown.
    if let Some(stop_tx) = state.orchestrator_stop_tx.lock().await.take() {
        let _ = stop_tx.send(());
        log::debug!("orchestrator stop signal sent");
    }
    // [M13.1.5c] Detach orchestrator handle. Task сам exit'ит через stop_rx
    // (см. выше). Не await'им и не abort'им — иначе stop_recording завис бы
    // на in-flight chunk_runner или прервали бы его (DB row остался бы в
    // `processing`). Detach'нутый task finalize'ит в фоне.
    if state.orchestrator.lock().await.take().is_some() {
        log::debug!("orchestrator handle detached on stop");
    }
    // [M13.2.1] Drop pause_tx — recv() arm orchestrator'а получит None →
    // wildcard match → no-op. Stop сигнал выше уже триггерит break.
    state.orchestrator_pause_tx.lock().await.take();

    let result = audio_macos::stop(session).await;

    let call = match result {
        Ok(r) => crate::db::finish_recording(&state.db, &call_id, r.duration_sec).await?,
        Err(e) => {
            let _ = crate::db::fail_recording(&state.db, &call_id).await;
            EventBus::new(Some(&app)).recording_state_changed();
            return Err(e);
        }
    };

    EventBus::new(Some(&app)).recording_state_changed();

    // M2.4-2.5: транскрипция в фоне. Возвращаем клиенту calls row сразу
    // (status=processing), статус подтянется через list_calls когда pipeline
    // финишнет (status → ready или failed).
    PipelineRunner::spawn_initial(
        state.db.clone(),
        state.store.clone(),
        state.device_id.clone(),
        app,
        state.pipeline_tasks.clone(),
        call_id,
        mic_path,
        system_path,
    )
    .await;

    Ok(call)
}

/// [W2] Pause активную запись. DB-only: проставляет `paused_at = now()` чтобы
/// UI и timer могли корректно показать состояние паузы и накопленное «на паузе»
/// время. На уровне Swift sidecar v1 пауза не отправляется — audio frames
/// продолжают писаться в WAV (тишина/тихая комната через STT отрежется в
/// silence trim, см. W2 §4 — Rust-level pause).
///
/// TODO(W2 v2): wire NDJSON `{"cmd":"pause"}` в Swift sidecar когда
/// AudioRecorder.swift получит pause/resume API. Сейчас sidecar не знает о
/// паузе, frames продолжают писаться. Это безопасный default для MVP.
#[tauri::command]
pub async fn pause_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RecordingState, AppError> {
    let (call_id, started_at) = {
        let guard = state.recording.lock().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| AppError::Other("not recording".into()))?;
        (session.call_id.clone(), session.started_at.to_rfc3339())
    };

    crate::db::pause_call(&state.db, &call_id).await?;
    // [M13.2.1] Fire-and-forget pause сигнал в orchestrator (если активен).
    // Channel buffer=8 покрывает burst pause/resume; на full — drop OK
    // (next pause/resume цикл починит state).
    if let Some(tx) = state.orchestrator_pause_tx.lock().await.as_ref() {
        let _ = tx.try_send(true);
    }
    let (paused_at, paused_total_ms) = pause_snapshot(&state, &call_id).await?;
    EventBus::new(Some(&app)).recording_state_changed();
    Ok(RecordingState {
        call_id,
        started_at,
        paused_at,
        paused_total_ms,
    })
}

/// [W2] Resume записи с паузы. DB-only (см. `pause_recording` rationale).
/// Идемпотентно: если запись не была на паузе — вернёт текущий state без
/// изменений.
#[tauri::command]
pub async fn resume_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RecordingState, AppError> {
    let (call_id, started_at) = {
        let guard = state.recording.lock().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| AppError::Other("not recording".into()))?;
        (session.call_id.clone(), session.started_at.to_rfc3339())
    };

    crate::db::resume_call(&state.db, &call_id).await?;
    // [M13.2.1] Fire-and-forget resume сигнал в orchestrator.
    if let Some(tx) = state.orchestrator_pause_tx.lock().await.as_ref() {
        let _ = tx.try_send(false);
    }
    let (paused_at, paused_total_ms) = pause_snapshot(&state, &call_id).await?;
    EventBus::new(Some(&app)).recording_state_changed();
    Ok(RecordingState {
        call_id,
        started_at,
        paused_at,
        paused_total_ms,
    })
}

#[tauri::command]
pub async fn get_audio_permissions(app: AppHandle) -> Result<PermissionsStatus, AppError> {
    permissions::check(&app).await
}

#[tauri::command]
pub async fn request_audio_permissions(
    app: AppHandle,
    target: String,
) -> Result<PermissionsStatus, AppError> {
    permissions::request(&app, &target).await
}

#[tauri::command]
pub fn open_system_privacy_pane(pane: String) -> Result<(), AppError> {
    let url = match pane.as_str() {
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        "screen_recording" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        _ => return Err(AppError::Other(format!("unknown pane: {pane}"))),
    };

    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| AppError::Other(format!("open failed: {e}")))?;
    Ok(())
}

// ============================================================================
// [M13.1.5c] Chunked pipeline wiring helpers
// ============================================================================

/// Owned bundle для setup chunked pipeline'а — channels + provider + stop_tx.
/// Создаётся в `prepare_chunked_setup`, разбирается в `spawn_orchestrator`.
struct ChunkedSetup {
    channels: OrchestratorChannels,
    rms_rx: mpsc::Receiver<(u64, f32)>,
    rotate_rx: mpsc::Receiver<serde_json::Value>,
    stop_rx: oneshot::Receiver<()>,
    /// [M13 review fix] Moved в AppState в `spawn_orchestrator` чтобы пережить
    /// возврат функции. Иначе оригинальный `_stop_tx` дропался → `stop_rx`
    /// сразу closed → orchestrator exit преждевременно.
    stop_tx: oneshot::Sender<()>,
    /// [M13.2.1] Pause/resume control. Pause-aware orchestrator skip'ает
    /// rotation timer пока запись на pause. `pause_tx` moved в AppState
    /// в `spawn_orchestrator`.
    pause_tx: mpsc::Sender<bool>,
    pause_rx: mpsc::Receiver<bool>,
    mic_provider: Arc<dyn TranscriptionProvider>,
    system_provider: Arc<dyn TranscriptionProvider>,
    stt_lang: String,
}

/// Прочитать settings + (если оба условия true) построить provider + channels.
/// Возвращает `Ok(None)` если chunked не активирован — это норм happy path.
/// `Err` зарезервирован для config errors которые имеет смысл log'нуть.
async fn prepare_chunked_setup(
    app: &AppHandle,
    state: &State<'_, AppState>,
    call_id: &str,
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

    let engine = db::get_setting(&state.db, SETTING_ACTIVE_ENGINE)
        .await?
        .as_deref()
        .and_then(EngineKind::from_str);
    if !matches!(engine, Some(EngineKind::Local)) {
        log::debug!("chunked_pipeline flag set but engine != local; skipping orchestrator");
        return Ok(None);
    }

    // Build LocalWhisperProvider mirror'ом логики pipeline::run.
    let preset = db::get_setting(&state.db, SETTING_ACTIVE_PRESET)
        .await?
        .as_deref()
        .and_then(LocalEnginePreset::from_str)
        .ok_or_else(|| {
            AppError::Other(
                "local_engine_preset_not_set: выберите Light/Balanced/Quality в Settings".into(),
            )
        })?;
    let whisper_id = preset.whisper_model_id();
    // [M13.1.5d] Два provider'а — для mic + system дорожек. TrackKind влияет
    // на дефолтные speaker tags ("owner" для mic, "speaker:0" для system),
    // которые потом cluster pipeline переcassign'ает в RecognizeSpeakers stage.
    let mic_provider =
        LocalWhisperProvider::for_preset(&state.app_data_dir, whisper_id, TrackKind::MicOwner)
            .with_app(app.clone())
            .await;
    let system_provider =
        LocalWhisperProvider::for_preset(&state.app_data_dir, whisper_id, TrackKind::System)
            .with_app(app.clone())
            .await;
    let mic_provider: Arc<dyn TranscriptionProvider> = Arc::new(mic_provider);
    let system_provider: Arc<dyn TranscriptionProvider> = Arc::new(system_provider);

    // STT lang — auto по умолчанию (см. pipeline::run).
    let stt_lang = db::get_setting(&state.db, "stt_lang")
        .await?
        .unwrap_or_else(|| "auto".to_string());

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
    }))
}

/// Spawn'нуть chunk_orchestrator task с rotate_fn + enqueue_fn closures.
/// Handle store'ится в `AppState.orchestrator`.
async fn spawn_orchestrator(
    state: &State<'_, AppState>,
    app: &AppHandle,
    call_id: &str,
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
    } = setup;

    let session_ref = state.recording.clone();
    let pool = state.db.clone();
    let store = state.store.clone();
    let call_id_str = call_id.to_string();

    let rotate_fn = make_rotate_fn(call_id_str.clone(), store.clone(), session_ref);
    let enqueue_fn = make_enqueue_fn(
        call_id_str.clone(),
        pool.clone(),
        store.clone(),
        mic_provider,
        system_provider,
        stt_lang,
        // [M13.2.1] app_data_dir для embedder resolve'а внутри chunk_runner;
        // app_handle — для emit'а transcript:chunk_done event.
        state.app_data_dir.clone(),
        app.clone(),
    );

    let handle = tauri::async_runtime::spawn(async move {
        let summary = chunk_orchestrator::run(
            ChunkOrchestratorConfig::default(),
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
    call_id: String,
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
    call_id: String,
    pool: SqlitePool,
    store: Arc<CallStore>,
    mic_provider: Arc<dyn TranscriptionProvider>,
    system_provider: Arc<dyn TranscriptionProvider>,
    lang: String,
    app_data_dir: std::path::PathBuf,
    app_handle: AppHandle,
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
        Box::pin(async move {
            let mic_path = store.chunk_mic_path(&call_id, chunk_idx);
            let system_path = store.chunk_system_path(&call_id, chunk_idx);
            db::chunks::insert_chunk(
                &pool,
                &call_id,
                chunk_idx,
                start_ms,
                &mic_path,
                &system_path,
            )
            .await
            .map_err(|e| format!("insert_chunk({chunk_idx}): {e}"))?;

            let input = ChunkRunInput {
                call_id: call_id.clone(),
                chunk_idx,
                start_ms,
                end_ms,
                mic_path,
                system_path,
                prev_prompt,
                lang: lang.clone(),
                app_data_dir: Some(app_data_dir.clone()),
                app_handle: Some(app_handle.clone()),
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
