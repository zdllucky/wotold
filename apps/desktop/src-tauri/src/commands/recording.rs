//! Commands for start/stop recording + audio permissions.

use crate::call_id::CallId;
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
/// [M13 follow-up, P-fix7] Mic diarization toggle. Default OFF (включается
/// только явным '1'/'true'). См. api/settings.ts `MIC_DIARIZATION_ENABLED`.
const SETTING_MIC_DIARIZATION: &str = "mic_diarization_enabled";
/// [P1.2] Labs: «Force N speakers» override для sortformer's `num_clusters`.
/// `None` (или невалидное значение) = auto-detect. Допустимые: "2" | "3" | "4"
/// (clamp к 1..=MAX_LOCAL_SPEAKERS в `SortformerDiarizer::with_num_speakers`).
const SETTING_MIC_DIARIZATION_NUM_SPEAKERS: &str = "mic_diarization_num_speakers";

/// [P1.2] Helper: прочитать `SETTING_MIC_DIARIZATION_NUM_SPEAKERS` из DB и
/// вернуть `Option<i32>` с clamping. Out-of-range / non-numeric → `None`.
/// [P14.3] Range 1..=MAX_LOCAL_SPEAKERS (=3). Старые legacy values "4" →
/// None (auto fallback) — `with_num_speakers` log warn'нёт.
async fn read_num_speakers_override(pool: &sqlx::SqlitePool) -> Result<Option<i32>, AppError> {
    Ok(db::get_setting(pool, SETTING_MIC_DIARIZATION_NUM_SPEAKERS)
        .await?
        .as_deref()
        .and_then(|s| s.parse::<i32>().ok())
        .filter(|n| (1..=crate::local_engine::diarization::MAX_LOCAL_SPEAKERS as i32).contains(n)))
}

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
    call_id: &CallId,
) -> Result<(Option<String>, i64), AppError> {
    let call = crate::db::get_call(&state.db, call_id.as_str())
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

    let (paused_at, paused_total_ms) = pause_snapshot(&state, &CallId::from_db(&call_id)).await?;
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
    let mic_path = state.store.mic_path(&CallId::from_db(&call.id));
    let system_path = state.store.system_path(&CallId::from_db(&call.id));

    // [M13.1.5c] Если CHUNKED_PIPELINE=ON + engine=local — настраиваем каналы
    // для chunk_orchestrator. Cloud engine ignor'ит флаг (server-side streaming
    // даёт минимальный win от chunking). При любых ошибках setup (preset не
    // задан, модель не скачана) — откатываемся на happy path без chunked.
    let chunked_setup = match prepare_chunked_setup(&app, &state, &CallId::from_db(&call.id)).await
    {
        Ok(setup) => setup,
        Err(e) => {
            log::warn!("chunked_pipeline disabled (setup failed): {e}; falling back to full-file");
            None
        }
    };
    let orchestrator_channels = chunked_setup.as_ref().map(|s| s.channels.clone());

    // [M13 fix] Chunk-0 layout: при chunked-режиме sidecar пишет первый chunk
    // прямо в `chunks/0/` (не в root), чтобы `run_chunk(0)` находил его по
    // `chunk_mic_path(0)`. Root `mic.wav`/`system.wav` — цель финального merge.
    // В non-chunked режиме sidecar пишет в root (прежнее поведение).
    let chunked = chunked_setup.is_some();
    let (sidecar_mic, sidecar_system) =
        sidecar_write_paths(&state.store, &CallId::from_db(&call.id), chunked);
    if chunked {
        state
            .store
            .ensure_chunk_dir(&CallId::from_db(&call.id), 0)
            .await?;
    }

    match audio_macos::start(
        &app,
        call.id.clone(),
        sidecar_mic,
        sidecar_system,
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
                spawn_orchestrator(&state, &app, &CallId::from_db(&call.id), setup).await;
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

/// Минимальная длительность записи (сек) — короче отбрасываем (не сохраняем,
/// не обрабатываем). Держать в синхроне с i18n `recording.tooShort {sec:30}`.
pub const MIN_RECORDING_SEC: f64 = 30.0;

/// [B19.6] Длительность WAV (сек) по фактическому размеру файла, а не по полю
/// `data`-чанка в заголовке: у прерванной (не финализированной) записи поле
/// размера может быть нулевым/устаревшим, а длина на диске — достоверна.
/// `None` если файл отсутствует/не WAV/повреждён. Не грузит весь файл — читает
/// только заголовок до `data`-чанка.
fn wav_duration_secs(path: &std::path::Path) -> Option<f64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let file_len = f.metadata().ok()?.len();
    let mut riff = [0u8; 12];
    f.read_exact(&mut riff).ok()?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return None;
    }
    let mut byte_rate: Option<u32> = None;
    let mut data_offset: Option<u64> = None;
    let mut pos: u64 = 12;
    loop {
        let mut chdr = [0u8; 8];
        if f.read_exact(&mut chdr).is_err() {
            break;
        }
        let size = u32::from_le_bytes([chdr[4], chdr[5], chdr[6], chdr[7]]) as u64;
        let body = pos + 8;
        if &chdr[0..4] == b"fmt " {
            // Sane fmt chunk is 16/18/40 bytes; reject absurd sizes (malformed/
            // adversarial) instead of computing a huge seek.
            if !(16..=128).contains(&size) {
                return None;
            }
            let mut fmt = [0u8; 16];
            if f.read_exact(&mut fmt).is_err() {
                break;
            }
            byte_rate = Some(u32::from_le_bytes([fmt[8], fmt[9], fmt[10], fmt[11]]));
            let skip = size.saturating_sub(16) + (size & 1);
            if skip > 0 {
                f.seek(SeekFrom::Current(skip as i64)).ok()?;
            }
        } else if &chdr[0..4] == b"data" {
            data_offset = Some(body);
            break;
        } else {
            f.seek(SeekFrom::Current((size + (size & 1)) as i64)).ok()?;
        }
        pos = body + size + (size & 1);
    }
    let br = byte_rate? as f64;
    let off = data_offset?;
    if br <= 0.0 || file_len <= off {
        return None;
    }
    Some((file_len - off) as f64 / br)
}

/// [B19.6] На старте разбираем строки, застрявшие в status='recording' (краш/
/// force-quit во время записи). По длине частичного mic-WAV: `<MIN_RECORDING_SEC`
/// или нет аудио → удаляем строку + temp WAV'ы; `≥MIN_RECORDING_SEC` → помечаем
/// 'failed' (восстановимо, юзер сможет переобработать). Возвращает число
/// обработанных строк.
pub async fn reconcile_orphan_recordings(
    pool: &SqlitePool,
    store: &CallStore,
) -> Result<usize, AppError> {
    let ids = db::list_orphan_recording_ids(pool).await?;
    let mut handled = 0usize;
    for id in ids {
        let dur = wav_duration_secs(&store.mic_path(&CallId::from_db(&id)));
        match dur {
            Some(d) if d >= MIN_RECORDING_SEC => {
                // Per-id non-fatal: одна битая строка не должна блокировать остальные.
                if let Err(e) =
                    db::fail_recording_with_reason(pool, &id, Some("Запись прервана")).await
                {
                    log::warn!("orphan recording {id}: mark-failed failed: {e}");
                    continue;
                }
                log::warn!("orphan recording {id}: {d:.0}s → failed (recoverable)");
            }
            _ => {
                // Файлы трём ТОЛЬКО при успешном DB-delete: иначе ghost-строка
                // 'recording' с удалёнными WAV → reconcile зациклится на старте.
                // remove_call_dir сносит весь calls/<id>/ (вкл. chunks/) — C5.
                match db::delete_call_and_samples(pool, &id).await {
                    Ok(()) => {
                        let _ = store.remove_call_dir(&CallId::from_db(&id)).await;
                        log::warn!(
                            "orphan recording {id}: discarded (interrupted <30s or no audio)"
                        );
                    }
                    Err(e) => {
                        log::warn!("orphan recording {id}: delete failed, leaving for retry: {e}");
                        continue;
                    }
                }
            }
        }
        handled += 1;
    }
    Ok(handled)
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Call>, AppError> {
    let session = {
        let mut guard = state.recording.lock().await;
        guard
            .take()
            .ok_or_else(|| AppError::Other("not recording".into()))?
    };

    let call_id = session.call_id.clone();
    let mic_path = session.mic_path.clone();
    let system_path = session.system_path.clone();
    // [P6] Capture wall-clock start ДО move session — sidecar's stopped
    // event возвращает duration ТОЛЬКО текущего chunk'а (с last rotate),
    // не total. Используем Rust-side started_at для real total duration.
    let started_at = session.started_at;

    // [M13 review fix] Signal stop через oneshot — это каноничный path для
    // orchestrator exit. До fix'а `stop_tx` дропался в `spawn_orchestrator`
    // → `stop_rx` сразу closed → premature exit. Теперь stop_tx живёт в
    // AppState и здесь take()→send() даёт чистый shutdown.
    if let Some(stop_tx) = state.orchestrator_stop_tx.lock().await.take() {
        let _ = stop_tx.send(());
        log::debug!("orchestrator stop signal sent");
    }
    // [M13 fix] Take orchestrator handle — раньше он детачился и summary
    // терялся. Теперь background finalize task (ниже) await'ит его чтобы
    // (а) дренировать все rotated-chunk `run_chunk` до assembly и
    // (б) получить координаты открытого финального chunk'а (который никогда
    // не rotated → никогда не enqueued) и обработать его. `Some` только при
    // активном chunked-режиме.
    let orch_handle = state.orchestrator.lock().await.take();
    // [M13.2.1] Drop pause_tx — recv() arm orchestrator'а получит None →
    // wildcard match → no-op. Stop сигнал выше уже триггерит break.
    state.orchestrator_pause_tx.lock().await.take();

    let result = audio_macos::stop(session).await;

    let (call, total_ms) = match result {
        Ok(_r) => {
            // [P6] Real total wall-clock duration. `_r.duration_sec` от sidecar
            // = только current chunk (per-rotate reset в Swift AudioRecorder
            // → не accumulated). Используем session.started_at, минус
            // paused_total_ms из DB (finish_recording не делает это сам).
            let elapsed_ms = (chrono::Utc::now() - started_at).num_milliseconds().max(0);
            // paused_total_ms НЕ включает текущее открытое окно паузы, если stop
            // нажали во время паузы (resume_call/finish_recording свернут его лишь
            // позже, line 242). Складываем его здесь тем же расчётом, что resume_call
            // (RFC3339 paused_at) — иначе total_sec завышается на длительность паузы,
            // duration_sec пишется неверно, и короткая запись (audio <30с, но долгая
            // пауза) могла бы проскочить min-duration гейт.
            let (paused_at, paused_total_ms): (Option<String>, i64) =
                sqlx::query_as("SELECT paused_at, paused_total_ms FROM calls WHERE id = ?1")
                    .bind(&call_id)
                    .fetch_optional(&state.db)
                    .await?
                    .unwrap_or((None, 0));
            let lingering_paused_ms = paused_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| {
                    (chrono::Utc::now() - dt.with_timezone(&chrono::Utc))
                        .num_milliseconds()
                        .max(0)
                })
                .unwrap_or(0);
            let paused_ms = paused_total_ms.saturating_add(lingering_paused_ms);
            let total_sec = (elapsed_ms - paused_ms).max(0) as f64 / 1000.0;
            // [min-duration] <30с — отбрасываем: удаляем строку звонка + temp
            // WAV, пайплайн НЕ пускаем. Фронт покажет тост «слишком коротко».
            if total_sec < MIN_RECORDING_SEC {
                // Сначала DB-удаление; файлы трём ТОЛЬКО при успехе — иначе
                // получим ghost-строку 'recording' с удалёнными WAV (на старте
                // reconcile зациклится). remove_call_dir сносит весь calls/<id>/
                // (вкл. chunks/), а не только top-level mic/system — иначе C5
                // residual-аудио при chunked-записи.
                match crate::db::delete_call_and_samples(&state.db, &call_id).await {
                    Ok(()) => {
                        let _ = state
                            .store
                            .remove_call_dir(&CallId::from_db(&call_id))
                            .await;
                    }
                    Err(e) => {
                        log::warn!("min-duration discard: delete call {call_id} failed: {e}");
                    }
                }
                EventBus::new(Some(&app)).recording_state_changed();
                return Ok(None);
            }
            let call = crate::db::finish_recording(&state.db, &call_id, total_sec).await?;
            (call, (total_sec * 1000.0) as u64)
        }
        Err(e) => {
            let _ = crate::db::fail_recording(&state.db, &call_id).await;
            EventBus::new(Some(&app)).recording_state_changed();
            return Err(e);
        }
    };

    EventBus::new(Some(&app)).recording_state_changed();

    // [M13 fix] Background finalize: (1) await orchestrator (drain rotated
    // chunks + получить final-chunk координаты) → (2) обработать открытый
    // финальный chunk → (3) запустить pipeline. Всё в одном spawned task'е
    // чтобы гарантировать порядок (финальный chunk done ДО assembly), но не
    // блокировать Stop — команда возвращает calls row (status=processing)
    // сразу, как раньше (M2.4-2.5).
    spawn_finalize_and_pipeline(
        &state,
        &app,
        CallId::from_db(&call_id),
        mic_path,
        system_path,
        orch_handle,
        total_ms,
    );

    Ok(Some(call))
}

/// [M13 fix] Решение по открытому финальному chunk'у на stop — на основе его
/// текущего DB-статуса. Pure + тестируемо. `Run` — вставить (если нет) и
/// прогнать; `RunAfterReset` — `failed`→`pending` затем прогнать; `Skip` —
/// уже `done`/`processing` (rotated event успел его обработать).
#[derive(Debug, PartialEq, Eq)]
enum FinalChunkAction {
    Run,
    RunAfterReset,
    Skip,
}

fn plan_final_chunk(rows: &[db::chunks::ChunkRow], k: u32) -> FinalChunkAction {
    match rows.iter().find(|r| r.chunk_idx == k) {
        None => FinalChunkAction::Run,
        Some(r) => match r.status.as_str() {
            "pending" => FinalChunkAction::Run,
            "failed" => FinalChunkAction::RunAfterReset,
            // done / processing — rotated event уже enqueue'нул этот chunk.
            _ => FinalChunkAction::Skip,
        },
    }
}

/// [M13 fix] Spawn'ит фоновый task: дренирует orchestrator, обрабатывает
/// открытый финальный chunk, затем запускает pipeline. Не блокирует Stop.
#[allow(clippy::too_many_arguments)]
fn spawn_finalize_and_pipeline(
    state: &State<'_, AppState>,
    app: &AppHandle,
    call_id: CallId,
    mic_path: std::path::PathBuf,
    system_path: std::path::PathBuf,
    orch_handle: Option<tauri::async_runtime::JoinHandle<chunk_orchestrator::OrchestratorSummary>>,
    total_ms: u64,
) {
    let db = state.db.clone();
    let store = state.store.clone();
    let pipeline_tasks = state.pipeline_tasks.clone();
    let app_data_dir = state.app_data_dir.clone();
    let app = app.clone();

    tokio::spawn(async move {
        // 1. Chunked-режим: await orchestrator (дренирует rotated chunks +
        //    возвращает координаты открытого финального chunk'а).
        if let Some(handle) = orch_handle {
            match handle.await {
                Ok(summary) => {
                    if let Err(e) = process_final_chunk(
                        &db,
                        &store,
                        &app_data_dir,
                        &app,
                        &call_id,
                        &summary,
                        total_ms,
                    )
                    .await
                    {
                        log::warn!("final chunk processing failed for {call_id}: {e}");
                    }
                }
                Err(e) => log::warn!("orchestrator join failed for {call_id}: {e}"),
            }
        }

        // 2. Pipeline (assembly → merge → recap). Все chunk'и уже в DB.
        PipelineRunner::spawn_initial(
            db,
            store,
            app,
            pipeline_tasks,
            call_id.as_str().to_string(),
            mic_path,
            system_path,
        )
        .await;
    });
}

/// [M13 fix] Обработать открытый финальный chunk (тот, чей rotated event так
/// и не пришёл до stop). Читает координаты из `OrchestratorSummary`, проверяет
/// FSM-статус + наличие аудио на диске, гоняет `run_chunk`. `end_ms` берётся
/// из authoritative wall-clock total (точнее чем last RMS timestamp).
async fn process_final_chunk(
    db: &SqlitePool,
    store: &Arc<CallStore>,
    app_data_dir: &std::path::Path,
    app: &AppHandle,
    call_id: &CallId,
    summary: &chunk_orchestrator::OrchestratorSummary,
    total_ms: u64,
) -> Result<(), AppError> {
    let k = summary.final_chunk_idx;
    let rows = db::chunks::list_chunks_by_call(db, call_id.as_str()).await?;
    let action = plan_final_chunk(&rows, k);
    if action == FinalChunkAction::Skip {
        log::debug!("final chunk {call_id}/{k}: skip (already tracked as done/processing)");
        return Ok(());
    }

    let mic_path = store.chunk_mic_path(call_id, k);
    let system_path = store.chunk_system_path(call_id, k);
    if !mic_path.exists() {
        log::warn!(
            "final chunk {call_id}/{k}: no audio at {}, skip",
            mic_path.display()
        );
        return Ok(());
    }

    let start_ms = summary.final_chunk_start_ms;
    // [M13 fix] end_ms из реальной длины WAV финального chunk'а — точно и
    // согласовано с audio + orchestrator-offset'ами. Раньше брали wall-clock
    // total_ms (pause-SUBTRACTED), тогда как chunk_start_ms из sidecar-durations
    // pause-INCLUSIVE → на паузах финальный chunk схлопывался в ноль. Fallback
    // на total_ms.max(start) если WAV нечитаем.
    let chunk_dur_ms = crate::pipeline::chunk_recovery::wav_duration_ms(&mic_path).unwrap_or(0);
    let end_ms = if chunk_dur_ms > 0 {
        start_ms + chunk_dur_ms
    } else {
        total_ms.max(start_ms)
    };

    // Гарантировать pending-строку перед run_chunk (он делает pending→processing).
    match action {
        FinalChunkAction::RunAfterReset => {
            db::chunks::mark_chunk_pending(db, call_id.as_str(), k).await?;
        }
        FinalChunkAction::Run => {
            db::chunks::insert_chunk(db, call_id.as_str(), k, start_ms, &mic_path, &system_path)
                .await?;
        }
        FinalChunkAction::Skip => unreachable!(),
    }

    let providers = build_chunk_providers(db, app_data_dir, app, call_id).await?;
    let input = ChunkRunInput {
        call_id: call_id.to_string(),
        chunk_idx: k,
        start_ms,
        end_ms,
        mic_path,
        system_path,
        // Parallel-mode trade-off: prev_prompt None (тот же выбор что orchestrator).
        prev_prompt: None,
        lang: providers.lang.clone(),
        app_data_dir: Some(app_data_dir.to_path_buf()),
        app_handle: Some(app.clone()),
        mic_diarization: providers.mic_diarization,
        mic_diarization_num_speakers: providers.mic_diarization_num_speakers,
    };
    let out = chunk_runner::run_chunk(db, providers.mic.as_ref(), providers.system.as_ref(), input)
        .await?;
    log::info!(
        "final chunk {call_id}/{k}: done, {} segments (start_ms={start_ms}, end_ms={end_ms})",
        out.segment_count
    );
    Ok(())
}

/// [TD-07] Pause активную запись — включая сам ЗАХВАТ.
///
/// Порядок принципиален: сначала команда сайдкару, только потом БД. Если
/// запись в сайдкар упала, возвращаем Err и БД не трогаем — UI останется в
/// состоянии «идёт запись». Обратный порядок дал бы ровно ту багу, которую
/// чиним: интерфейс говорит «на паузе», а микрофон пишет.
///
/// До TD-07 пауза была DB-only: сайдкар о ней не знал, кадры продолжали
/// писаться в WAV, и сказанное «на паузе» уезжало в транскрипт и саммари.
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

    // Сайдкар первым — см. rationale выше.
    {
        let mut guard = state.recording.lock().await;
        if let Some(session) = guard.as_mut() {
            crate::audio::macos::pause(session).await?;
        }
    }
    crate::db::pause_call(&state.db, &call_id).await?;
    // [M13.2.1] Fire-and-forget pause сигнал в orchestrator (если активен).
    // Channel buffer=8 покрывает burst pause/resume; на full — drop OK
    // (next pause/resume цикл починит state).
    if let Some(tx) = state.orchestrator_pause_tx.lock().await.as_ref() {
        let _ = tx.try_send(true);
    }
    let (paused_at, paused_total_ms) = pause_snapshot(&state, &CallId::from_db(&call_id)).await?;
    EventBus::new(Some(&app)).recording_state_changed();
    Ok(RecordingState {
        call_id,
        started_at,
        paused_at,
        paused_total_ms,
    })
}

/// [TD-07] Resume записи с паузы — возобновляет захват (см. `pause_recording`).
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

    {
        let mut guard = state.recording.lock().await;
        if let Some(session) = guard.as_mut() {
            crate::audio::macos::resume(session).await?;
        }
    }
    crate::db::resume_call(&state.db, &call_id).await?;
    // [M13.2.1] Fire-and-forget resume сигнал в orchestrator.
    if let Some(tx) = state.orchestrator_pause_tx.lock().await.as_ref() {
        let _ = tx.try_send(false);
    }
    let (paused_at, paused_total_ms) = pause_snapshot(&state, &CallId::from_db(&call_id)).await?;
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

/// [M13 fix] Выбор путей, в которые sidecar физически пишет первый chunk.
/// chunked → `chunks/0/{mic,system}.wav`; non-chunked → root `{mic,system}.wav`.
/// Pure (без side-effects); создание `chunks/0/` — на caller'е (ensure_chunk_dir).
fn sidecar_write_paths(
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
    let mic_diarization = matches!(
        db::get_setting(db, SETTING_MIC_DIARIZATION)
            .await?
            .as_deref(),
        Some("1") | Some("true")
    );
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
    /// [M13 follow-up] Sortformer на mic для multi-voice. Default ON.
    mic_diarization: bool,
    /// [P1.2] Labs «Force N speakers» override. None = auto-detect.
    mic_diarization_num_speakers: Option<i32>,
}

/// Прочитать settings + (если оба условия true) построить provider + channels.
/// Возвращает `Ok(None)` если chunked не активирован — это норм happy path.
/// `Err` зарезервирован для config errors которые имеет смысл log'нуть.
async fn prepare_chunked_setup(
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
async fn spawn_orchestrator(
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

/// [Tech-debt P0.2] Retry одного failed chunk'а через background spawn.
///
/// Wire-up:
/// - DB `mark_chunk_pending` (FSM gate failed → pending) — sync, fail если
///   chunk не в failed.
/// - Resolve providers тем же путём что `prepare_chunked_setup` (preset →
///   LocalWhisperProvider mic + system).
/// - `tokio::spawn` background task: `chunk_runner::run_chunk` пишет
///   результат в DB + emit'ит `transcript:chunk_done` — UI обновится сам.
/// - Return Ok(()) сразу — не блокируем Tauri command поток.
#[tauri::command]
pub async fn retry_chunk(
    app: AppHandle,
    state: State<'_, AppState>,
    call_id: String,
    chunk_idx: u32,
) -> Result<(), AppError> {
    // [TD-05] call_id из webview — валидируем до любых путей.
    let parsed_id = CallId::parse(&call_id)?;
    use crate::pipeline::chunk_runner;

    // 1. Validate chunk существует + status == failed (mark_chunk_pending
    //    enforce'ит FSM, но row lookup всё равно нужен для start/end_ms).
    let rows = db::chunks::list_chunks_by_call(&state.db, &call_id).await?;
    let row = rows
        .iter()
        .find(|r| r.chunk_idx == chunk_idx)
        .ok_or_else(|| AppError::NotFound(format!("chunk {call_id}/{chunk_idx} not found")))?;
    if row.status != "failed" {
        return Err(AppError::Other(format!(
            "retry_chunk: chunk {call_id}/{chunk_idx} status={} (need 'failed')",
            row.status
        )));
    }
    let start_ms = row.start_ms.max(0) as u64;
    let end_ms = row.end_ms.unwrap_or(start_ms as i64).max(0) as u64;

    // 2. Build providers — shared helper (mirror prepare_chunked_setup).
    let ChunkProviders {
        mic: mic_provider,
        system: system_provider,
        lang: stt_lang,
        mic_diarization,
        mic_diarization_num_speakers,
    } = build_chunk_providers(&state.db, &state.app_data_dir, &app, &parsed_id).await?;

    // 4. FSM gate failed → pending. После этого chunk_runner внутри сделает
    //    pending → processing → done|failed.
    db::chunks::mark_chunk_pending(&state.db, &call_id, chunk_idx).await?;

    // 5. Background spawn — не блокируем UI. Errors handled внутри
    //    chunk_runner (mark_failed + emit chunk_done event).
    let mic_path = state.store.chunk_mic_path(&parsed_id, chunk_idx);
    let system_path = state.store.chunk_system_path(&parsed_id, chunk_idx);
    let pool = state.db.clone();
    let app_data_dir = state.app_data_dir.clone();
    let app_for_task = app.clone();
    let call_id_clone = call_id.clone();
    // [P11.1] Дополнительные клоны для post-success auto-resume hook.
    let store_for_resume = state.store.clone();
    let tasks_for_resume = state.pipeline_tasks.clone();
    let app_for_resume = app.clone();
    log::info!(
        "retry_chunk: spawning run_chunk for {call_id_clone}/{chunk_idx} \
         (start_ms={start_ms}, end_ms={end_ms})"
    );
    tokio::spawn(async move {
        let input = ChunkRunInput {
            call_id: call_id_clone.clone(),
            chunk_idx,
            start_ms,
            end_ms,
            mic_path,
            system_path,
            // No prev_prompt — chunk N-1 tail может быть устаревший после
            // первого fail'а. Точность первой фразы пострадает на ~10pp,
            // acceptable trade-off для retry-сценария.
            prev_prompt: None,
            lang: stt_lang,
            app_data_dir: Some(app_data_dir),
            app_handle: Some(app_for_task),
            mic_diarization,
            mic_diarization_num_speakers,
        };
        match chunk_runner::run_chunk(
            &pool,
            mic_provider.as_ref(),
            system_provider.as_ref(),
            input,
        )
        .await
        {
            Ok(out) => {
                log::info!(
                    "retry_chunk[{call_id_clone}/{chunk_idx}]: success, {} segments",
                    out.segment_count
                );
                // [P11.1] Auto-resume pipeline: если все chunks теперь done и
                // звонок не активен (не recording) — spawn reprocess через тот
                // же PipelineRunner что использует stop_recording. Idempotent
                // через `spawn_reprocess` abort+respawn для same call_id.
                if let Err(e) = maybe_resume_pipeline_after_chunk(
                    &pool,
                    &CallId::from_db(&call_id_clone),
                    store_for_resume,
                    app_for_resume,
                    tasks_for_resume,
                )
                .await
                {
                    log::warn!("retry_chunk[{call_id_clone}/{chunk_idx}]: auto-resume failed: {e}");
                }
            }
            Err(e) => log::warn!("retry_chunk[{call_id_clone}/{chunk_idx}]: failed: {e}"),
        }
    });

    Ok(())
}

// [TD-33] Восстановление сломанных/прерванных записей (recover_chunked_call,
// spawn_recover_chunked, headless- и авто-триггеры) переехало в
// `commands/recovery.rs`: там оно тестируемо, здесь файл и так сверх лимита 800.

/// [P11.1] После того как chunk_runner перевёл chunk failed→done в
/// `retry_chunk`, проверить: можно ли уже автоматически возобновить
/// downstream pipeline (diarize → audio_merger → recap).
///
/// **Гарантии auto-resume:**
/// - Все chunks для звонка `status='done'` (нет ни pending, ни failed).
/// - Звонок не recording (active recording owns orchestrator, не наш case).
/// - Звонок не уже `status='ready'` (recap уже persisted).
///
/// Если условия совпали — `PipelineRunner::spawn_reprocess` сам идемпотентен:
/// abort'ает existing task для этого `call_id` (если есть) и spawn'ит новый
/// `pipeline::run` через chunked path (load_chunked_transcripts skip STT).
async fn maybe_resume_pipeline_after_chunk(
    pool: &SqlitePool,
    call_id: &CallId,
    store: Arc<crate::call_store::CallStore>,
    app: AppHandle,
    tasks: crate::services::pipeline_runner::PipelineTasks,
) -> Result<(), AppError> {
    if !db::chunks::all_chunks_done(pool, call_id.as_str()).await? {
        log::debug!("maybe_resume_pipeline_after_chunk[{call_id}]: not all chunks done, skip");
        return Ok(());
    }
    let Some(call) = db::get_call(pool, call_id.as_str()).await? else {
        return Err(AppError::NotFound(format!("call {call_id}")));
    };
    if call.status == "recording" {
        log::debug!("maybe_resume_pipeline_after_chunk[{call_id}]: still recording, skip");
        return Ok(());
    }
    if call.status == "ready" && call.recap_failed_reason.is_none() {
        log::debug!(
            "maybe_resume_pipeline_after_chunk[{call_id}]: already ready w/o failure, skip"
        );
        return Ok(());
    }
    log::info!("maybe_resume_pipeline_after_chunk[{call_id}]: all chunks done, spawning reprocess");
    PipelineRunner::spawn_reprocess(pool.clone(), store, app, tasks, call_id.to_string()).await
}

// [P-fix4] `force_restt_call` удалён — слит в `reprocess_call` («Переобработать
// целиком» теперь всегда делает re-STT через delete_chunks_for_call). Отдельная
// destructive-кнопка «Распознать заново» убрана как дубль.

#[cfg(test)]
mod tests {
    use super::*;
    use db::chunks::ChunkRow;

    fn row(idx: u32, status: &str) -> ChunkRow {
        ChunkRow {
            call_id: "c1".into(),
            chunk_idx: idx,
            start_ms: 0,
            end_ms: None,
            mic_path: String::new(),
            system_path: String::new(),
            status: status.into(),
            transcript_json: None,
            system_transcript_json: None,
            embeddings_json: None,
        }
    }

    #[test]
    fn plan_final_chunk_runs_when_absent() {
        // K не в списке → Run (вставить + прогнать).
        assert_eq!(
            plan_final_chunk(&[row(0, "done")], 1),
            FinalChunkAction::Run
        );
        assert_eq!(plan_final_chunk(&[], 0), FinalChunkAction::Run);
    }

    #[test]
    fn plan_final_chunk_runs_when_pending() {
        assert_eq!(
            plan_final_chunk(&[row(2, "pending")], 2),
            FinalChunkAction::Run
        );
    }

    #[test]
    fn plan_final_chunk_reset_when_failed() {
        assert_eq!(
            plan_final_chunk(&[row(2, "failed")], 2),
            FinalChunkAction::RunAfterReset
        );
    }

    #[test]
    fn plan_final_chunk_skips_when_done_or_processing() {
        // rotated event уже enqueue'нул этот chunk → не дублируем.
        assert_eq!(
            plan_final_chunk(&[row(1, "done")], 1),
            FinalChunkAction::Skip
        );
        assert_eq!(
            plan_final_chunk(&[row(1, "processing")], 1),
            FinalChunkAction::Skip
        );
    }

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

    use crate::db::test_support::fresh_db;

    /// Bytes of a minimal PCM WAV with `data_len` payload bytes. `data_size_field`
    /// lets us simulate an unfinalized header (0) vs a correct size.
    fn wav_bytes(byte_rate: u32, data_len: usize, data_size_field: u32) -> Vec<u8> {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&0u32.to_le_bytes()); // riff size — ignored by parser
        b.extend_from_slice(b"WAVE");
        b.extend_from_slice(b"fmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // PCM
        b.extend_from_slice(&1u16.to_le_bytes()); // mono
        b.extend_from_slice(&16_000u32.to_le_bytes()); // sample rate
        b.extend_from_slice(&byte_rate.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes()); // block align
        b.extend_from_slice(&16u16.to_le_bytes()); // bits/sample
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_size_field.to_le_bytes());
        b.extend(std::iter::repeat(0u8).take(data_len));
        b
    }

    fn write_wav(byte_rate: u32, data_len: usize, data_size_field: u32) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "wotold-wavtest-{byte_rate}-{data_len}-{data_size_field}.wav"
        ));
        std::fs::write(&path, wav_bytes(byte_rate, data_len, data_size_field)).unwrap();
        path
    }

    #[test]
    fn duration_uses_file_size_not_header_field() {
        let br = 32_000u32; // 16kHz * 2 bytes/sample
                            // 2s of audio, but header data-size is 0 (unfinalized, crash).
        let p = write_wav(br, 64_000, 0);
        let d = wav_duration_secs(&p).expect("duration");
        assert!((d - 2.0).abs() < 0.05, "expected ~2.0s, got {d}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn duration_correct_with_valid_header() {
        let br = 32_000u32;
        let p = write_wav(br, 16_000, 16_000); // 0.5s
        let d = wav_duration_secs(&p).expect("duration");
        assert!((d - 0.5).abs() < 0.05, "expected ~0.5s, got {d}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn duration_none_for_missing_or_nonwav() {
        assert!(wav_duration_secs(std::path::Path::new("/no/such/file.wav")).is_none());
        let p = std::env::temp_dir().join("wotold-not-a-wav.bin");
        std::fs::write(&p, b"not a wav file at all").unwrap();
        assert!(wav_duration_secs(&p).is_none());
        let _ = std::fs::remove_file(&p);
    }

    /// Helper: insert a status='recording' orphan + write a mic WAV reporting
    /// `secs` of audio (byte_rate=1000 → file_len ≈ secs*1000).
    async fn seed_orphan(pool: &SqlitePool, store: &CallStore, secs: usize) -> String {
        let call = db::insert_recording(pool, "managed").await.unwrap();
        std::fs::create_dir_all(store.call_dir(&CallId::from_db(&call.id))).unwrap();
        std::fs::write(
            store.mic_path(&CallId::from_db(&call.id)),
            wav_bytes(1000, secs * 1000, 0),
        )
        .unwrap();
        call.id
    }

    #[tokio::test]
    async fn reconcile_keeps_long_interrupted_recording_as_failed() {
        let db = fresh_db().await;
        let dir = tempfile::tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let id = seed_orphan(&db.pool, &store, 35).await; // ≥30s

        let n = reconcile_orphan_recordings(&db.pool, &store).await.unwrap();
        assert_eq!(n, 1);

        let after = db::get_call(&db.pool, &id).await.unwrap().unwrap();
        assert_eq!(
            after.status, "failed",
            "≥30s interrupted → recoverable failed"
        );
        assert_eq!(after.failed_reason.as_deref(), Some("Запись прервана"));
        assert!(
            store.mic_path(&CallId::from_db(&id)).exists(),
            "audio kept for recovery"
        );
    }

    #[tokio::test]
    async fn reconcile_discards_short_interrupted_recording() {
        let db = fresh_db().await;
        let dir = tempfile::tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let id = seed_orphan(&db.pool, &store, 8).await; // <30s

        let n = reconcile_orphan_recordings(&db.pool, &store).await.unwrap();
        assert_eq!(n, 1);

        assert!(
            db::get_call(&db.pool, &id).await.unwrap().is_none(),
            "<30s interrupted → row deleted"
        );
        assert!(
            !store.call_dir(&CallId::from_db(&id)).exists(),
            "call dir (incl. chunks) removed"
        );
    }
}
