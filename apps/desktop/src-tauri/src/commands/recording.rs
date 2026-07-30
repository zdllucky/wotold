//! Commands for start/stop recording + audio permissions.

use crate::call_id::CallId;
use std::sync::Arc;

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, State};

use crate::{
    audio::macos::{self as audio_macos},
    audio::permissions::{self, PermissionsStatus},
    audio::silence_watch::SilenceControl,
    call_store::CallStore,
    db::{self, Call},
    events::EventBus,
    pipeline::{
        chunk_orchestrator,
        chunk_runner::{self, ChunkRunInput},
    },
    services::pipeline_runner::PipelineRunner,
    state::AppState,
    AppError,
};

use super::chunked_setup::{
    build_chunk_providers, prepare_chunked_setup, sidecar_write_paths, spawn_orchestrator,
};

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
    state: &AppState,
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
    // [T2] Наблюдатель тишины поднимается независимо от chunked-режима: без
    // preset'а `prepare_chunked_setup` вернёт None, а тишину ловить всё равно
    // надо. `Ok(None)` — обе настройки выключены, канала не будет вовсе.
    let silence_tx = match super::silence::spawn_silence_watch(&app, &state).await {
        Ok(tx) => tx,
        Err(e) => {
            log::warn!("silence_watch disabled (settings read failed): {e}");
            None
        }
    };
    let fanout = audio_macos::DispatcherFanout {
        orchestrator: chunked_setup.as_ref().map(|s| s.channels.clone()),
        silence_tx,
    };

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
        fanout,
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
            // [T2] И гасим наблюдателя: он уже поднят, но записи не будет —
            // иначе задача осталась бы висеть с живым control-каналом до
            // следующего старта, который затёр бы ручки.
            super::silence::stop_silence_watch(&state).await;
            Err(e)
        }
    }
}

/// Минимальная длительность записи (сек) — короче отбрасываем (не сохраняем,
/// не обрабатываем). Держать в синхроне с i18n `recording.tooShort {sec:30}`.
pub const MIN_RECORDING_SEC: f64 = 30.0;

/// [T5 review] «Случайное нажатие» — запись, которую нужно снести целиком
/// вместе с каталогом, а не отдавать в пайплайн.
///
/// Смотрит на **wall-clock** за вычетом пауз, а НЕ на подрезанную длительность.
/// Разница появилась с авто-стопом: у получасовой записи с замьюченным
/// микрофоном подрезанная длительность — секунды, и гейт по ней удалил бы
/// строку звонка вместе со всем `calls/<id>/` молча. Признак случайного
/// нажатия — что запись вообще шла недолго, а не что в ней мало звука.
fn is_accidental_tap(elapsed_ms: i64, paused_ms: i64) -> bool {
    let sec = (elapsed_ms - paused_ms).max(0) as f64 / 1000.0;
    sec < MIN_RECORDING_SEC
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Call>, AppError> {
    stop_recording_inner(&app, &state, None).await
}

/// [T5/R15] Общее тело стопа для ручной команды и авто-стопа по тишине.
///
/// `trim_at_ms = None` — ручной стоп: поведение прежнее, аудио не трогаем
/// (решение владельца — резать только авто-остановленное).
/// `Some(ms)` — авто-стоп: всё после точки реза заведомо тишина, поэтому
/// длительность считается от неё, а не по wall-clock, и точка кладётся в
/// `calls.silence_trim_ms` — по ней пайплайн подрежет WAV и транскрипт.
pub(crate) async fn stop_recording_inner(
    app: &AppHandle,
    state: &AppState,
    trim_at_ms: Option<u64>,
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
    // [T2] Погасить наблюдателя тишины. Ручки живут ровно столько, сколько
    // сессия; следующий start поднимет новые. Задача и сама вышла бы на
    // закрытии `rms_tx`, но ждать терминального события сайдкара незачем.
    super::silence::stop_silence_watch(state).await;

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
            // [T5] Авто-стоп: длительность считается до точки реза. Всё после
            // неё — тишина, которую пайплайн отрежет; оставить здесь wall-clock
            // значило бы показать «звонок 3 часа» для 25-минутного разговора и
            // разойтись с длиной WAV в плеере.
            let effective_ms = match trim_at_ms {
                Some(trim) => (trim as i64).min(elapsed_ms),
                None => elapsed_ms,
            };
            let total_sec = (effective_ms - paused_ms).max(0) as f64 / 1000.0;
            // [min-duration] <30с — отбрасываем: удаляем строку звонка + temp
            // WAV, пайплайн НЕ пускаем. Фронт покажет тост «слишком коротко».
            //
            // [T5 review] Гейт смотрит на wall-clock, а НЕ на подрезанную
            // длительность. Его смысл — «случайное нажатие», и признак этого
            // именно wall-clock. На авто-стопе подрезанная длительность может
            // быть секундами при получасовой записи (микрофон замьючен, захват
            // системы запрещён — RMS ноль с самого начала), и гейт по ней снёс
            // бы строку звонка вместе со всем каталогом `calls/<id>/` молча.
            // Пользователь вернулся бы к пустому списку без единого следа.
            if is_accidental_tap(elapsed_ms, paused_ms) {
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
                EventBus::new(Some(app)).recording_state_changed();
                return Ok(None);
            }
            // [T5] Точка реза запоминается ДО finish_recording: пайплайн
            // стартует из `spawn_finalize_and_pipeline` ниже и обязан увидеть
            // её в БД, иначе подрежет не тот звонок или не подрежет вовсе.
            if let Some(trim) = trim_at_ms {
                if let Err(e) = crate::db::set_silence_trim_ms(&state.db, &call_id, trim).await {
                    // Рез не критичен для самой записи: аудио и транскрипт
                    // останутся полными, просто с тихим хвостом. Ронять из-за
                    // этого готовый звонок было бы хуже.
                    log::warn!("silence trim point not persisted for {call_id}: {e}");
                }
            }
            let call = crate::db::finish_recording(&state.db, &call_id, total_sec).await?;
            (call, (total_sec * 1000.0) as u64)
        }
        Err(e) => {
            let _ = crate::db::fail_recording(&state.db, &call_id).await;
            EventBus::new(Some(app)).recording_state_changed();
            return Err(e);
        }
    };

    EventBus::new(Some(app)).recording_state_changed();

    // [M13 fix] Background finalize: (1) await orchestrator (drain rotated
    // chunks + получить final-chunk координаты) → (2) обработать открытый
    // финальный chunk → (3) запустить pipeline. Всё в одном spawned task'е
    // чтобы гарантировать порядок (финальный chunk done ДО assembly), но не
    // блокировать Stop — команда возвращает calls row (status=processing)
    // сразу, как раньше (M2.4-2.5).
    spawn_finalize_and_pipeline(
        state,
        app,
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
    state: &AppState,
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
    };
    let out = chunk_runner::run_chunk(db, providers.mic.as_ref(), providers.system.as_ref(), input)
        .await?;
    log::info!(
        "final chunk {call_id}/{k}: done, {} segments (start_ms={start_ms}, end_ms={end_ms})",
        out.segment_count
    );
    Ok(())
}

/// [T2] Один сигнал паузы/возобновления на всех наблюдателей активной записи.
/// Отдельным хелпером, потому что их двое — `chunk_orchestrator` и
/// `silence_watch` — и они обязаны видеть одно и то же: «одинаковый контракт,
/// разная зрелость» — главный источник будущих багов в этом репо (правило 2).
///
/// [M13.2.1] Fire-and-forget: буфер каналов 8 покрывает burst pause/resume от
/// рук на UI, при переполнении дроп безопасен — следующий цикл починит.
async fn signal_capture_paused(state: &AppState, paused: bool) {
    if let Some(tx) = state.orchestrator_pause_tx.lock().await.as_ref() {
        let _ = tx.try_send(paused);
    }
    let control = if paused {
        SilenceControl::Pause
    } else {
        SilenceControl::Resume
    };
    super::silence::signal_silence_watch(state, control).await;
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
    signal_capture_paused(&state, true).await;
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
    signal_capture_paused(&state, false).await;
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

// [P-fix4] `force_restt_call` удалён — слит в `reprocess_call` («Переобработать
// целиком» теперь всегда делает re-STT через delete_chunks_for_call). Отдельная
// destructive-кнопка «Распознать заново» убрана как дубль.

#[cfg(test)]
mod tests {
    use super::*;
    use db::chunks::ChunkRow;

    // ── [T5 review] Гейт «случайное нажатие». Регрессия: пока он смотрел на
    //    подрезанную длительность, авто-стоп сносил получасовую запись с
    //    замьюченным микрофоном вместе с каталогом и без единого следа в UI.

    #[test]
    fn short_tap_is_discarded() {
        assert!(is_accidental_tap(5_000, 0));
        assert!(is_accidental_tap(29_999, 0));
    }

    #[test]
    fn long_recording_is_kept() {
        assert!(!is_accidental_tap(30_000, 0));
        assert!(!is_accidental_tap(30 * 60 * 1_000, 0));
    }

    #[test]
    fn pause_time_does_not_count_towards_the_gate() {
        // Минута записи, из них 50 секунд на паузе — реального звука 10с.
        assert!(is_accidental_tap(60_000, 50_000));
    }

    #[test]
    fn auto_stopped_silent_recording_survives_the_gate() {
        // Полчаса записи, из которых звук был только первые пять секунд
        // (микрофон замьючен, захват системы запрещён). Подрезанная
        // длительность — 5с, но удалять получасовую запись нельзя: это не
        // случайное нажатие, а сломанный захват, и юзер должен это увидеть.
        let elapsed_ms = 30 * 60 * 1_000;
        let trimmed_ms = 5_000;
        assert!(!is_accidental_tap(elapsed_ms, 0));
        assert!(
            is_accidental_tap(trimmed_ms, 0),
            "по подрезанной длительности гейт сработал бы — именно это и чинили"
        );
    }

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
}
