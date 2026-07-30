use std::path::{Path, PathBuf};

use serde_json::json;
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::{
    db, embeddings,
    events::{CallAutoBoundEvent, CallProgressEvent, EventBus, PipelineFinishedEvent},
    matching,
    pipeline::{clusters::load_and_extract_clusters, merge::OWNER_TAG},
    providers::transcription::{DiarizedTranscript, TranscriptSegment},
    AppError,
};

pub mod clusters;
pub(crate) mod diarize;
pub(crate) mod local_llm;
pub(crate) mod local_run;
use local_run::run_local_inner;

// [TD-35] Фасад: обвязка локальной LLM и диаризация переехали в свои модули,
// но снаружи путь `crate::pipeline::…` остался прежним — переписывать 20+
// вызовов ради переезда файла незачем.
pub(crate) use diarize::diarize_mic_track;
pub use local_llm::regenerate_recap;
#[cfg(target_os = "macos")]
pub use local_llm::{
    build_local_llm_provider, keep_resident_enabled, start_resident_server, stop_resident_server,
    warm_up_local_llm, SETTING_KEEP_RESIDENT,
};
pub mod merge;
/// [recap-rich] Нарратив-минутки — отдельный write-проход после structured reduce.
pub mod narrative;
pub mod recap;
pub mod recovery_flow;
pub mod settings;
pub mod stage;
pub mod voice_backfill;

// [M13.1.3b] Per-chunk pipeline для chunked pipelined transcription.
// Standalone от pipeline::run — параллельный entry point, не подключён к
// recording flow до M13.1.5c sprint'а.
pub mod chunk_runner;

// [M13.1.5b step 1] Orchestrator main loop — silence-aware rotate triggering
// + chunk_runner enqueue. Standalone, тестируется через mock channels.
pub mod chunk_orchestrator;

// [M13.1.5d] Assembly per-chunk transcripts → две DiarizedTranscript с
// timestamp-offset. Используется в `run_local_inner` чтобы пропустить
// full-file STT когда chunks_completed > 0.
pub mod chunk_assembly;

// [T6/R14] Отсечка транскрипта по точке реза тихого хвоста — вторая половина
// подрезки (первая, файловая, в `audio::wav_trim`). Без неё рез бессмыслен:
// тихие чанки прошли whisper ещё во время записи.
pub mod transcript_cutoff;

// [Tech-debt P0.1] Конкатенация per-chunk WAV файлов в root mic.wav/system.wav.
// Без этого AudioScrubber играет только первый chunk вместо полной записи.
pub mod audio_merger;
// [M13 fix] Recovery сломанных chunked-записей из on-disk WAV'ов.
pub mod chunk_lang;
pub mod chunk_recovery;
/// [P1.3] Periodic `recap:progress` event emitter wrapper. Оборачивает
/// local LLM future чтобы каждые 15s emit'ить elapsed_sec — UI рендерит
/// «Пересоздаём… {sec}s».
pub mod recap_progress;
/// [P4] Best-segment selector для voice sample slice metadata —
/// pure-fn lookup в merged raw_stt.json, возвращает (start, end, track)
/// для voice_backfill INSERT.
pub mod voice_sample_picker;

// [M13.2.1] Global agglomerative single-link cosine clustering на
// per-chunk WeSpeaker embeddings — сводит local speaker:N tags к global
// IDs между chunks (один физ.спикер = один global tag).
pub mod speaker_reclustering;

// [M13 follow-up] Owner identification на mic-дорожке после диаризации
// (когда MIC_DIARIZATION_ENABLED ON). Two-stage: biometric → primary by
// duration fallback. M3.7 invariant preserved через perетеггинг
// выбранного local tag → OWNER_TAG.
pub mod owner_identify;

// [M14 foundation] Summary v2 schema types (CallType, ActionItemCategory,
// EvidenceAnchor, ActionItemV2, Decision, OpenQuestion, ParticipantV2,
// CallSummaryV2). Backbone для будущих фаз M14 (T-02..T-10).
pub mod summary_v2;

// [M14 foundation] Validator для summary v2: substring fuzzy match
// evidence quotes (≥ 0.9), schema range checks, dedup, strip-on-fail.
pub mod summary_validator;

// [M14 T-04] Lightweight LLM-call для определения call_type до основного
// v2 generation. Используется local_orchestrator на Phase A.
pub(crate) mod classifier;

// [M14 T-05 Phase B] Split transcript.md на token-windows по speaker-turn
// boundaries для chunked-генерации на длинных звонках.
pub(crate) mod chunker;

// [F1] Refine-чейн для длинных transcripts: chunk 0 → первичный рекап,
// каждый следующий чанк расширяет/правит накопленный CallSummaryV2.
// Заменил map-reduce (map-шаги были контекстно-слепы между чанками).
pub(crate) mod refine_chain;

// [F3] Sink пошаговых событий генерации рекапа (`recap:step`) — thinking-блок.
pub(crate) mod recap_steps;

// [B20.3] Render-side bold известных имён в recap.md (детерминированно).
pub(crate) mod recap_md;
pub(crate) mod recap_render;

// [Q] Per-resource очереди тяжёлых local-ресурсов (stt/diarization/llm,
// concurrency=1) + `queue:state` снапшоты для QueueMonitor.
pub mod resource_queue;

// [M14 T-07 Phase C] Per-call-type focused prompts (8+1 specialized vs
// universal v2). Используется orchestrator + refine_chain когда classifier
// даёт known_call_type.
pub(crate) mod expert_prompts;

// [M14 T-08 Phase D] Action-item post-pass — refinement отдельным LLM-call'ом
// после main/reduce. Re-validate categories, owner_confidence, dedup,
// drop non-verbatim evidence. Local-only (cloud skip — Phase D-bis).
pub(crate) mod action_item_post_pass;

// [M14 T-09 Phase E] GBNF grammar fallback wrapper для local LLM JSON
// parsing failures. Retry первой неудачной попытки с
// `--grammar-file <universal_json.gbnf>` который констрейнит output до
// valid JSON object.
pub(crate) mod gbnf;

// [F2] Унификация спикеров одного контакта на этапе сборки промпта:
// rewrite `**speaker:N**` заголовков → display_name подтверждённого контакта
// + person-level Known participants блок (дедуп по contact_id).
pub(crate) mod speaker_prompt_ctx;

// [M14 T-17] Lightweight title-only LLM regeneration (kebab menu action).
// Separate path от regenerate_recap — отдельный LLM-call ~150 max_tokens.
// pub(crate) для commands/pipeline::regenerate_title.
pub(crate) mod title_regen;

// [M14 T-12] Golden set + CI regression harness — 10 reference cases прогоняются
// через full parse/validate/strip/dedup pipeline и diff'ются against expected.
#[cfg(test)]
mod golden_eval;

// [M14 T-13] LLM-as-judge G-Eval scoring (coherence/faithfulness/relevance/
// conciseness). Foundation для quality eval; production usage (Tauri command,
// DB persistence, UI display) — backlog M14.5.
pub(crate) mod g_eval;

// [M14 T-10] Local engine orchestrator — chain classifier + main v2 gen.
// Короткий transcript → single-pass; длинный → [F1] refine-чейн.
pub(crate) mod local_orchestrator;

// [M14 follow-up] JSON Schemas для schema-constrained local generation —
// форсят v2-форму через llama `--json-schema-file` (вместо generic json.gbnf).
pub(crate) mod llm_schemas;

pub use merge::{merge_tracks, render_transcript_md, sanitize_merged};
pub use settings::PipelineSettings;
pub use stage::Stage;

/// Контекст одной транскрипции: пути к двум дорожкам, call_dir для артефактов,
/// device-id для managed-режима. `app_data_dir` нужен B3.6 cluster pipeline
/// для резолва пути к локальной ONNX-модели эмбеддера. Настройки
/// (provider/path/lang/proxy URL) и BYO-ключи читаются из БД внутри `run`.
pub struct PipelineCtx {
    pub call_id: String,
    pub call_dir: PathBuf,
    pub mic_path: PathBuf,
    pub system_path: PathBuf,
    /// [B3.6] Корень app-данных для поиска `models/embedder.onnx`. Cluster pipeline
    /// fallback'ит на StubEmbedder если модель отсутствует или ONNX feature off.
    pub app_data_dir: PathBuf,
}

/// [V6.2] Persist + emit `call:progress`. Ошибки не fatal — pipeline продолжает,
/// фронт переподнимет state на reload через get_call. Сoncurrent writer'ы
/// здесь не страшны: каждый step монотонно растёт, последний выигрывает.
pub(super) async fn emit_progress(
    pool: &SqlitePool,
    app: Option<&AppHandle>,
    call_id: &str,
    step: u8,
    pct: u8,
    eta_sec: Option<i64>,
    upload_bytes: Option<i64>,
) {
    if let Err(e) = db::set_call_progress(pool, call_id, step, pct, eta_sec, upload_bytes).await {
        log::warn!("set_call_progress {call_id} step={step}: {e}");
    }
    let bus = EventBus::new(app);
    bus.call_progress(&CallProgressEvent {
        call_id: call_id.to_string(),
        step,
        pct,
        eta_sec,
        upload_bytes,
    });
}

/// Запуск STT после остановки записи (M2.4-2.5 паспорта). Транскрибирует
/// [P13] Halt gate перед stage 2→3 transition в chunked path. Strict
/// all-or-nothing — если есть ЛЮБОЙ non-done chunk (pending/processing/
/// failed) → возвращает Err с явным reason для UI surface через
/// `recap_failed_reason`. Pipeline не должен build'ить partial transcript.
///
/// **0 chunks** → `Ok(())` (cloud / non-chunked path unaffected — там
/// pipeline идёт через full-file STT, halt не релевантен).
///
/// **All done** → `Ok(())` (proceed to merge/diarize/recap).
///
/// **Любой non-done** → `Err("chunks_need_retry: N of M ...")`. UI читает
/// через humanError pattern, P11.2 ChunkFailureAccordion уже показывает
/// retry buttons. После retry → P11.1 auto-resume → halt gate проходит.
///
/// Callsites:
/// - [`reprocess_call`] pre-flight (заменяет existing P1.1 warn).
/// - [`run_local_inner`] перед `chunk_assembly::load_chunked_transcripts`
///   (initial pipeline после stop_recording тоже gate'ится).
pub(crate) async fn ensure_all_chunks_done(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<(), AppError> {
    let chunks = db::chunks::list_chunks_by_call(pool, call_id).await?;
    if chunks.is_empty() {
        return Ok(());
    }
    let not_done: Vec<u32> = chunks
        .iter()
        .filter(|c| c.status != "done")
        .map(|c| c.chunk_idx)
        .collect();
    if !not_done.is_empty() {
        return Err(AppError::Other(format!(
            "chunks_need_retry: {} of {} chunks not done (idx: {:?})",
            not_done.len(),
            chunks.len(),
            not_done
        )));
    }
    Ok(())
}

/// [M13 fix] Готовность chunk'ов к сборке транскрипта. Мягче чем
/// `ensure_all_chunks_done`: различает «часть готова» (partial — собираем
/// done-подмножество) от «ничего не готово» (полный провал). Позволяет
/// показать частичный транскрипт вместо total loss при провале одного chunk'а.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChunkGate {
    /// У звонка нет chunk-строк → non-chunked / full-file путь.
    NoChunks,
    /// Все chunk'и `done`.
    AllDone,
    /// Часть `done`, часть не готова — собираем partial транскрипт.
    Partial {
        done: usize,
        total: usize,
        failed: Vec<u32>,
    },
    /// Ни одного `done` — полный провал chunked-пути.
    NoneDone { total: usize },
}

pub(crate) async fn chunks_ready(pool: &SqlitePool, call_id: &str) -> Result<ChunkGate, AppError> {
    let chunks = db::chunks::list_chunks_by_call(pool, call_id).await?;
    if chunks.is_empty() {
        return Ok(ChunkGate::NoChunks);
    }
    let total = chunks.len();
    let done = chunks.iter().filter(|c| c.status == "done").count();
    let failed: Vec<u32> = chunks
        .iter()
        .filter(|c| c.status != "done")
        .map(|c| c.chunk_idx)
        .collect();
    Ok(if failed.is_empty() {
        ChunkGate::AllDone
    } else if done == 0 {
        ChunkGate::NoneDone { total }
    } else {
        ChunkGate::Partial {
            done,
            total,
            failed,
        }
    })
}

/// [P-fix4] Язык звонка для пина обоих треков в auto-режиме full-file STT.
/// Звонок одноязычный; язык берём по треку с речью. **System-трек —
/// приоритетный якорь**: собеседник обычно говорит чётко с начала, тогда как
/// owner-mic часто молчит первые минуты → ненадёжный per-track detect (даёт
/// «en» → [FOREIGN] на русской речи). Fallback на mic если system почти пуст.
/// `None` если оба ниже порога речи — тогда пин не делаем.
pub(super) fn call_language(mic: &DiarizedTranscript, sys: &DiarizedTranscript) -> Option<String> {
    use crate::pipeline::chunk_lang::{real_word_count, MIN_WORDS_FOR_LANG_PIN};
    if real_word_count(sys) >= MIN_WORDS_FOR_LANG_PIN {
        if let Some(l) = sys.lang_detected.as_deref().filter(|s| !s.is_empty()) {
            return Some(l.to_string());
        }
    }
    if real_word_count(mic) >= MIN_WORDS_FOR_LANG_PIN {
        if let Some(l) = mic.lang_detected.as_deref().filter(|s| !s.is_empty()) {
            return Some(l.to_string());
        }
    }
    None
}

/// mic и system параллельно, сливает таймлайн, сохраняет `raw_stt.json` и
/// `transcript.md`, проставляет `calls.status = ready/failed`.
///
/// `app` — optional Tauri handle для emit события «pipeline finished». Если None
/// (тесты / headless), событие не отправляется.
pub async fn run(
    pool: &SqlitePool,
    ctx: PipelineCtx,
    app: Option<&AppHandle>,
) -> Result<(), AppError> {
    let bus = EventBus::new(app);
    // [B16] Emit `pipeline:started` для global progress indicator в topnav.
    bus.pipeline_started(&ctx.call_id);

    let result = run_inner(pool, &ctx, app).await;

    // [TD-12] Причина провала пайплайна (если был).
    let pipeline_err = result.as_ref().err().map(|e| {
        log::error!("pipeline {} failed: {e}", ctx.call_id);
        // M2.7 (#23): UX-readable reason для UI. Сама технодеталь в логах.
        match e {
            AppError::Other(s) => s.clone(),
            other => other.to_string(),
        }
    });

    // [TD-12] mark_call_ready может упасть ПОСЛЕ успешного пайплайна (busy
    // pool, disk full). Раньше здесь стоял `?`, и он выходил из run() ДО
    // bus.pipeline_finished и минуя fail_recording — звонок навсегда висел в
    // `processing`, а фронт не получал ни finished, ни failed. Теперь ошибка
    // не короткозамыкает, а становится failed-исходом; событие эмитится всегда.
    let mark_ready_err = if pipeline_err.is_none() {
        match db::mark_call_ready(pool, &ctx.call_id).await {
            Ok(()) => None,
            Err(e) => {
                log::error!(
                    "mark_call_ready({}) failed после успешного пайплайна: {e}",
                    ctx.call_id
                );
                Some(format!("не удалось записать статус ready: {e}"))
            }
        }
    } else {
        None
    };

    let event = finish_event(&ctx.call_id, pipeline_err, mark_ready_err);

    match event.status {
        "ready" => {
            // [M15.3] Индексация ассистента — fire-and-forget; headless
            // (app=None) добирается startup-backfill'ом. Только на настоящем ready.
            if let Some(app) = app {
                crate::assistant::indexer::spawn_index(app, &ctx.call_id);
            }
        }
        _ => {
            // failed (пайплайн ИЛИ mark_ready) — persist reason, чтобы звонок не
            // залип в `processing`. Один вызов покрывает оба случая.
            let _ =
                db::fail_recording_with_reason(pool, &ctx.call_id, event.failed_reason.as_deref())
                    .await;
        }
    }

    // [B5]: фронт слушает 'pipeline:finished' для realtime-обновления Calls list.
    // [TD-12] Эмитится безусловно — это и был сломанный инвариант.
    bus.pipeline_finished(&event);

    match event.status {
        "ready" => Ok(()),
        // mark-ready-fail и раньше давал Err (через `?`) — контракт потребителей
        // сохранён, но теперь с эмитом события и persist'ом статуса.
        _ => Err(AppError::Other(
            event
                .failed_reason
                .unwrap_or_else(|| "pipeline failed".to_string()),
        )),
    }
}

/// [TD-12] Решение о финальном событии пайплайна. Вынесено чистой функцией,
/// потому что сам `run()` требует pool + полный пайплайн и юнитом не тестируем
/// (тот же приём, что `classify_event` и `plan_final_chunk`).
///
/// Инвариант: если пайплайн успешен, но `mark_call_ready` не записался, звонок
/// ОБЯЗАН стать `failed` (артефакты на диске, юзер сможет reprocess), а не
/// остаться `ready`/висящим.
fn finish_event(
    call_id: &str,
    pipeline_err: Option<String>,
    mark_ready_err: Option<String>,
) -> PipelineFinishedEvent {
    let failed_reason = pipeline_err.or(mark_ready_err);
    match failed_reason {
        Some(reason) => PipelineFinishedEvent {
            call_id: call_id.to_string(),
            status: "failed",
            failed_reason: Some(reason),
        },
        None => PipelineFinishedEvent {
            call_id: call_id.to_string(),
            status: "ready",
            failed_reason: None,
        },
    }
}

/// Перезапустить полный pipeline (STT + recap) для существующего звонка.
/// Используется когда:
///   - предыдущая попытка зафейлилась по сети / квоте / API
///   - переключились с BYO на managed или наоборот
///   - сменили STT провайдера в Settings
///
/// Берёт mic.wav и system.wav с диска. Если их нет — Err. Сбрасывает
/// failed_reason и переводит статус в processing перед стартом.
pub async fn reprocess_call(
    pool: &SqlitePool,
    app_data_dir: &std::path::Path,
    call_id: &str,
    app: Option<&AppHandle>,
) -> Result<(), AppError> {
    // [B16] Validate существование row. Само значение `call` не нужно ниже —
    // только existence-check. `_` префикс подавляет dead-code warning без
    // искусственного `let _ = &call`.
    let _call = db::get_call(pool, call_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("call {call_id}")))?;

    let call_dir = app_data_dir.join("calls").join(call_id);
    let mic_path = call_dir.join("mic.wav");
    let system_path = call_dir.join("system.wav");

    if !mic_path.exists() && !system_path.exists() {
        return Err(AppError::Other(
            "Аудио файлы (mic.wav / system.wav) не найдены на диске — переобработка невозможна."
                .into(),
        ));
    }

    // [P13] Halt gate перед reprocess — pipeline strict all-or-nothing.
    // Если есть failed chunks → bail с явным reason, UI читает через
    // `recap_failed_reason` (existing banner pattern). User должен retry
    // failed chunks; P11.1 auto-resume подхватит pipeline когда все done.
    //
    // Заменяет existing P1.1 log::warn — раньше continued с partial data
    // и pipeline шёл по 1/3 как по 3/3, build'ил incomplete transcript.
    if let Err(e) = ensure_all_chunks_done(pool, call_id).await {
        let reason = e.to_string();
        log::warn!("reprocess_call {call_id} halt: {reason}");
        // [P16.2] Write `failed_reason` тоже (ErrorScreen его читает) +
        // recap_failed_reason для recap banner backward compat. Также
        // clears pipeline_* fields чтобы UI не показывал stale processing
        // UI если frontend optimistic patch не reverted.
        let _ = db::fail_recording_with_reason(pool, call_id, Some(&reason)).await;
        let _ = db::set_recap_failed_reason(pool, call_id, Some(&reason)).await;
        return Err(e);
    }

    // [TD-37] Оговорки прошлого прогона к новому результату отношения не
    // имеют — чистим до старта, а не после, чтобы UI не показывал их поверх
    // уже идущей переобработки.
    let _ = db::clear_degraded_flags(pool, call_id).await;

    // Reset status: was failed → processing, clear failed_reason.
    // Если был ready — тоже перетянем в processing, чтобы UI показывал прогресс
    // и не закешировал старый recap.
    // [V6.2] Заодно очищаем pipeline_* fields — старый прогресс не должен
    // мигнуть в UI до того как новый run эмитнёт step=1.
    sqlx::query(
        "UPDATE calls
         SET status = 'processing',
             failed_reason = NULL,
             pipeline_step = NULL,
             pipeline_pct = NULL,
             pipeline_eta_sec = NULL,
             upload_bytes = NULL,
             updated_at = ?1
         WHERE id = ?2",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(call_id)
    .execute(pool)
    .await?;

    let ctx = PipelineCtx {
        call_id: call_id.to_string(),
        call_dir,
        mic_path,
        system_path,
        app_data_dir: app_data_dir.to_path_buf(),
    };

    run(pool, ctx, app).await
}

/// [Phase 3 R2] Helper: эмитит `(step, 0)` перед `f.await`, и `(step, 100)`
/// при успехе. Ошибка пробрасывается БЕЗ финального emit'а — это даёт UI
/// сигнал «упали на шаге X с pct=0» через DB-state (set_call_progress
/// эмиссия pct=0 уже произошла).
///
/// `upload_bytes` опционально — нужен только для Stage::Upload, чтобы
/// UI показал «Загружено N МБ из M». Остальные stages передают None.
pub(super) async fn run_stage<F, T>(
    pool: &SqlitePool,
    app: Option<&AppHandle>,
    call_id: &str,
    stage: Stage,
    upload_bytes: Option<i64>,
    f: F,
) -> Result<T, AppError>
where
    F: std::future::Future<Output = Result<T, AppError>>,
{
    let step = stage.step();
    emit_progress(pool, app, call_id, step, 0, None, upload_bytes).await;
    let result = f.await?;
    emit_progress(pool, app, call_id, step, 100, None, upload_bytes).await;
    Ok(result)
}

async fn run_inner(
    pool: &SqlitePool,
    ctx: &PipelineCtx,
    app: Option<&AppHandle>,
) -> Result<(), AppError> {
    // [Phase 2 R3] Все настройки одним проходом, typed.
    let s = PipelineSettings::load(pool).await?;

    // Local-only: единственный движок обработки (whisper.cpp + sherpa-onnx
    // диаризация + llama.cpp, macOS). Cloud/proxy-путь удалён при переходе на
    // local-only. Не-macOS — движок недоступен (R9).
    #[cfg(target_os = "macos")]
    {
        run_local_inner(pool, ctx, app, &s).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ctx, app, &s);
        Err(AppError::Other(
            "Локальный движок недоступен на этой платформе (только macOS, R9).".into(),
        ))
    }
}

/// [Phase 3 R2] Stage 1 — Upload. В текущей реализации no-op
/// (real per-byte streaming требует middleware вокруг reqwest, которого ещё
/// нет). Хелпер существует чтобы run_inner был симметричный — каждая stage
/// это отдельная async fn.
///
/// Возвращает upload_bytes hint (для UI «Загружено N МБ»). None если оба
/// аудио-файла отсутствуют (test fixtures + edge cases).
pub(super) async fn stage_upload(upload_bytes_hint: Option<i64>) -> Result<Option<i64>, AppError> {
    Ok(upload_bytes_hint)
}

/// [Phase 3 R2] Stage 4 — merge tracks + persist artifacts. Раньше это был
/// `persist_artifacts` хелпер — теперь явно stage. Возвращает merged-сегменты
/// для последующих stages (recognize_speakers + recap).
pub(super) async fn stage_merge_artifacts(
    call_dir: &PathBuf,
    mic: &DiarizedTranscript,
    system: &DiarizedTranscript,
) -> Result<Vec<TranscriptSegment>, AppError> {
    tokio::fs::create_dir_all(call_dir).await?;

    // [P-fix] sanitize_merged — единый chokepoint очистки от whisper-галлюцинаций
    // ([FOREIGN], субтитр-credits) + repetition loops. Покрывает chunked /
    // full-file / cloud + переассемблируемые старые chunks (без re-STT).
    let merged = sanitize_merged(merge_tracks(mic, system));

    // M2.5: raw_stt.json держим чтобы перегенерировать рекап без повторной оплаты STT.
    let raw = json!({
        "version": 1,
        "mic": mic,
        "system": system,
        "merged": &merged,
    });
    tokio::fs::write(
        call_dir.join("raw_stt.json"),
        serde_json::to_vec_pretty(&raw)?,
    )
    .await?;

    let md = render_transcript_md(&merged);
    tokio::fs::write(call_dir.join("transcript.md"), md).await?;

    Ok(merged)
}

/// [Phase 3 R2] Stage 3 — voice clusters + matching → suggestion. Wrapper над
/// `run_cluster_pipeline` чтобы run_inner был симметричный.
pub(super) async fn stage_recognize_speakers(
    pool: &SqlitePool,
    ctx: &PipelineCtx,
    merged: &[TranscriptSegment],
) -> Result<(), AppError> {
    run_cluster_pipeline(
        pool,
        &ctx.call_id,
        merged,
        &ctx.mic_path,
        &ctx.system_path,
        &ctx.app_data_dir,
    )
    .await
}

/// [B11] M7.4: добавить placeholder rows в call_speakers для всех distinct
/// speaker_tag из транскрипта (кроме owner — у него уже confirmed). UI покажет
/// анонимных «S1/S2», юзер сможет привязать через select.
/// Non-fatal: warning при ошибке.
pub(super) async fn ensure_anonymous_speakers_present(
    pool: &SqlitePool,
    call_id: &str,
    merged: &[TranscriptSegment],
) {
    let mut seen_tags: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for seg in merged {
        if seg.speaker_tag != OWNER_TAG && !seg.speaker_tag.is_empty() {
            seen_tags.insert(seg.speaker_tag.clone());
        }
    }
    let tags_vec: Vec<String> = seen_tags.into_iter().collect();
    // [P-fix9] Реконсиляция (не add-only): сначала вычищаем устаревшие
    // анонимные теги, которых больше нет в merged (фантомы от прошлых прогонов,
    // например speaker:3 от mic-diar-ON). Делаем ВСЕГДА, в т.ч. при пустом
    // tags_vec (solo-звонок) — независимо от cluster-пайплайна (он non-fatal).
    match db::prune_call_speakers_not_in(pool, call_id, &tags_vec).await {
        Ok(n) if n > 0 => log::info!("reconcile call_speakers {call_id}: pruned {n} stale tags"),
        Ok(_) => {}
        Err(e) => log::warn!("prune_call_speakers_not_in {call_id} failed: {e}"),
    }
    if tags_vec.is_empty() {
        return;
    }
    if let Err(e) = db::ensure_call_speakers_present(pool, call_id, &tags_vec).await {
        log::warn!("ensure_call_speakers_present {call_id} failed: {e}");
    }
}

/// [V7] Auto-bind speakers с suggestion_score >= threshold/100 при включенной
/// настройке Settings → Транскрипция → «Автоматически привязывать собеседника».
///
/// Default OFF (R2 паспорта). Threshold parsing + clamping живёт в
/// `PipelineSettings::load` — здесь только применяем уже-typed config.
pub(super) async fn run_auto_bind(
    pool: &SqlitePool,
    app: Option<&AppHandle>,
    call_id: &str,
    s: &PipelineSettings,
) -> Result<(), AppError> {
    let Some(cfg) = &s.auto_bind else {
        return Ok(());
    };
    let threshold = cfg.threshold;
    let threshold_pct = (threshold * 100.0).round() as u8;

    let count = db::auto_bind_high_confidence_speakers(pool, call_id, threshold).await?;
    if count == 0 {
        return Ok(());
    }
    log::info!("auto-bound {count} speaker(s) for call {call_id} (threshold {threshold_pct}%)");
    let bus = EventBus::new(app);
    bus.call_auto_bound(&CallAutoBoundEvent {
        call_id: call_id.to_string(),
        count,
        threshold_pct,
    });
    Ok(())
}

/// [B3.3-3.4] Извлекает voice clusters per speaker_tag, persist'ит в DB,
/// запускает matching против consenting voice_samples, populate'ит
/// suggestion_contact_id/score/source. Non-fatal: ошибки сюда логятся
/// и пропускаются (recap всё равно сгенерируется).
///
/// [B3.6] Embedder выбирается dispatcher'ом — реальный OnnxEmbedder если
/// модель найдена в `app_data_dir/models/embedder.onnx` и фича `voice-onnx`
/// включена, иначе StubEmbedder (no-op → пустые clusters).
///
/// [Phase 3 R9] После persist'а cluster'а вызываем
/// `voice_backfill::maybe_backfill_voice_sample` — раньше эта логика жила
/// внутри `db::set_call_speaker_cluster`, теперь side-effect наружу.
async fn run_cluster_pipeline(
    pool: &SqlitePool,
    call_id: &str,
    merged: &[TranscriptSegment],
    mic_path: &Path,
    system_path: &Path,
    app_data_dir: &Path,
) -> Result<(), AppError> {
    let clusters = load_and_extract_clusters(
        merged.to_vec(),
        mic_path.to_path_buf(),
        system_path.to_path_buf(),
        app_data_dir,
        &format!("cluster pipeline {call_id}"),
    )
    .await?;
    if clusters.is_empty() {
        log::debug!("cluster pipeline {call_id}: no clusters extracted");
        return Ok(());
    }

    // [B3.4] Загружаем существующие voice_samples всех consenting контактов
    // ОДИН раз перед циклом — matching::list_consenting_samples делает join.
    let consenting = matching::list_consenting_samples(pool).await?;

    // [P4] Read merged raw_stt.json one time перед циклом — voice_backfill
    // использует для best-segment slice metadata. None если файла нет
    // (race / disk issue) → graceful fallback на legacy INSERT.
    let raw_stt_json: Option<String> = tokio::fs::read_to_string(
        app_data_dir
            .join("calls")
            .join(call_id)
            .join("raw_stt.json"),
    )
    .await
    .ok();

    for (tag, vector) in &clusters {
        let blob = embeddings::embedding_to_bytes(vector);
        if blob.is_empty() {
            continue;
        }
        if let Err(e) = db::set_call_speaker_cluster(pool, call_id, tag, &blob).await {
            log::warn!("set_call_speaker_cluster {tag}: {e}");
            continue;
        }

        // [Phase 3 R9] Reprocess backfill: если speaker уже confirmed + контакт
        // дал consent_voice — upsert'им voice_sample (idempotent). До Phase 3
        // эта логика жила внутри `set_call_speaker_cluster`. Non-fatal:
        // warning + continue.
        if let Err(e) = voice_backfill::maybe_backfill_voice_sample(
            pool,
            call_id,
            tag,
            &blob,
            raw_stt_json.as_deref(),
        )
        .await
        {
            log::warn!("voice_backfill {tag}: {e}");
        }

        // Owner-тег не matching'им — он привязан к owner-contact автоматически.
        if tag == OWNER_TAG {
            continue;
        }
        // Top-1 кандидат с min_score 0.5 (M3.4 default).
        let ranked = matching::rank_candidates(vector, &consenting, 0.5, 1);
        if let Some(top) = ranked.into_iter().next() {
            if let Err(e) = db::set_call_speaker_suggestion(
                pool,
                call_id,
                tag,
                Some(&top.contact_id),
                Some(top.score as f64),
                Some("embedding"),
            )
            .await
            {
                log::warn!("set_call_speaker_suggestion {tag}: {e}");
            } else {
                log::info!(
                    "voice match {tag} → {} ({:.3})",
                    top.display_name,
                    top.score
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
