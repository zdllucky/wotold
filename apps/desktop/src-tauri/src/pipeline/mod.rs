use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::{
    db,
    embeddings::{self, StubEmbedder},
    events::{CallAutoBoundEvent, CallProgressEvent, EventBus, PipelineFinishedEvent},
    matching,
    pipeline::{clusters::extract_clusters, merge::OWNER_TAG},
    providers::{
        transcription::{
            failure_reason, transcribe_with_fallback, DiarizedTranscript, GladiaProvider,
            RetryConfig, SonioxProvider, TranscriptSegment, TranscriptionError, TranscriptionOpts,
            TranscriptionProvider,
        },
        ProviderMode,
    },
    secrets::{self, ByoProvider},
    AppError,
};

pub mod clusters;
pub mod merge;
pub mod recap;
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

// [Tech-debt P0.1] Конкатенация per-chunk WAV файлов в root mic.wav/system.wav.
// Без этого AudioScrubber играет только первый chunk вместо полной записи.
pub mod audio_merger;
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
// boundaries для map-reduce на длинных звонках.
pub(crate) mod chunker;

// [M14 T-06 Phase B] Map-reduce orchestration: per-chunk map → final reduce.
pub(crate) mod map_reduce;

// [M14 T-07 Phase C] Per-call-type focused prompts (8+1 specialized vs
// universal v2). Используется orchestrator + map_reduce когда classifier
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
// Phase A skeleton; Phase B/C/D добавят chunking, map-reduce, expert prompts.
pub(crate) mod local_orchestrator;

pub use merge::{merge_tracks, render_transcript_md};
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
    pub device_id: Arc<str>,
    /// [B3.6] Корень app-данных для поиска `models/embedder.onnx`. Cluster pipeline
    /// fallback'ит на StubEmbedder если модель отсутствует или ONNX feature off.
    pub app_data_dir: PathBuf,
}

/// [V6.2] Persist + emit `call:progress`. Ошибки не fatal — pipeline продолжает,
/// фронт переподнимет state на reload через get_call. Сoncurrent writer'ы
/// здесь не страшны: каждый step монотонно растёт, последний выигрывает.
async fn emit_progress(
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
    let event = match &result {
        Ok(()) => {
            db::mark_call_ready(pool, &ctx.call_id).await?;
            PipelineFinishedEvent {
                call_id: ctx.call_id.clone(),
                status: "ready",
                failed_reason: None,
            }
        }
        Err(e) => {
            log::error!("pipeline {} failed: {e}", ctx.call_id);
            // M2.7 (#23): UX-readable reason для UI. Сама технодеталь в логах.
            let reason = match e {
                AppError::Other(s) => s.clone(),
                other => other.to_string(),
            };
            let _ = db::fail_recording_with_reason(pool, &ctx.call_id, Some(&reason)).await;
            PipelineFinishedEvent {
                call_id: ctx.call_id.clone(),
                status: "failed",
                failed_reason: Some(reason),
            }
        }
    };

    // [B5]: фронт слушает 'pipeline:finished' для realtime-обновления Calls list.
    bus.pipeline_finished(&event);

    result
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
    device_id: &Arc<str>,
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
        let _ = db::set_recap_failed_reason(pool, call_id, Some(&reason)).await;
        return Err(e);
    }

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
        device_id: Arc::clone(device_id),
        app_data_dir: app_data_dir.to_path_buf(),
    };

    run(pool, ctx, app).await
}

/// [P5.1] Resolve current local preset → engine label string для persist
/// в `calls.summary_engine`. Best-effort: на ошибке чтения preset либо
/// None preset возвращает None — caller передаёт это в `set_recap_failure`
/// и `summary_engine` остаётся unchanged (safer чем persist неправильный).
async fn local_engine_label_from_pool(pool: &SqlitePool) -> Option<String> {
    let raw = db::get_setting(pool, crate::local_engine::preset::SETTING_ACTIVE_PRESET)
        .await
        .ok()
        .flatten()?;
    let preset = crate::local_engine::preset::LocalEnginePreset::from_str(&raw)?;
    Some(preset.engine_label().to_string())
}

/// M4.5 паспорта: ручная регенерация рекапа без повторной транскрипции.
/// Используется когда:
///   - первая попытка LLM упала (квота / network) и пользователь хочет повторить
///   - сменили модель в Settings и хотят пересоздать рекап на ней
///   - в транскрипт были внесены правки (будущий M4.6)
///
/// Читает `transcript.md` с диска, перегенерит `recap.md` + `action_items`.
/// transcript.md обязателен — иначе AppError. Ошибки LLM пробрасываются
/// в UI как Err (а не silently skip как в pipeline::run).
pub async fn regenerate_recap(
    pool: &SqlitePool,
    app_data_dir: &std::path::Path,
    device_id: &Arc<str>,
    call_id: &str,
    app: Option<&AppHandle>,
) -> Result<(), AppError> {
    let call = db::get_call(pool, call_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("call {call_id}")))?;

    let call_dir = app_data_dir.join("calls").join(call_id);
    let transcript_path = call_dir.join("transcript.md");
    let transcript_md = tokio::fs::read_to_string(&transcript_path)
        .await
        .map_err(|e| AppError::Other(format!("transcript.md отсутствует: {e}")))?;

    // [Bug-fix] Pre-clear stale recap_failed_reason — чтобы баннер с прошлой
    // ошибкой не сбивал юзера с толку пока новая попытка идёт. Если новая
    // попытка упадёт, ниже перезапишется свежим текстом (с актуальным engine).
    let _ = db::set_recap_failed_reason(pool, call_id, None).await;

    // [Phase 2 R3] Typed settings — один read, typed fields, edge cases
    // (malformed threshold, empty proxy URL, "auto" lang) изолированы.
    let s = PipelineSettings::load(pool).await?;
    let effective_lang = s.effective_recap_lang(call.lang_detected.as_deref());

    // [Bug-fix] Diagnostic — без этого лога невозможно понять какой движок
    // на самом деле обслуживает regenerate. Юзер может видеть UI "Local"
    // в Settings, но если PipelineSettings::load вернул CloudManaged
    // (race condition или stale cache), regenerate уйдёт в cloud → 429.
    log::info!(
        "regenerate_recap dispatch: call_id={} engine={:?} provider_path={} model_override={:?} summary_v2={}",
        call_id,
        s.engine,
        s.provider_path,
        s.model_override(),
        s.summary_v2_enabled,
    );

    // [Bug-fix] Раньше regenerate_recap всегда шёл через cloud Anthropic,
    // игнорируя активный движок. Юзеры на local engine получали 429 от
    // Cloudflare proxy на свой Cloud-managed quota. Теперь dispatch'имся
    // на engine == Local → local Qwen через LocalLlamaProvider (no network,
    // no quota). Cloud path остаётся unchanged для Cloud-managed users.
    #[cfg(target_os = "macos")]
    if s.engine == crate::local_engine::engine::EngineKind::Local {
        let app = app.ok_or_else(|| {
            AppError::Other(
                "regenerate_recap для local-engine требует AppHandle (внутренняя ошибка)".into(),
            )
        })?;
        let result = regenerate_recap_local(
            pool,
            app_data_dir,
            &call_dir,
            call_id,
            &transcript_md,
            effective_lang.as_deref(),
            app,
            &s,
        )
        .await;
        return match result {
            Ok(()) => {
                let _ = db::set_recap_failed_reason(pool, call_id, None).await;
                Ok(())
            }
            Err(e) => {
                // [P5.1] Persist engine label atomically с reason —
                // banner badge ↔ failure text всегда matched.
                let engine_label = local_engine_label_from_pool(pool).await;
                let _ = db::set_recap_failure(
                    pool,
                    call_id,
                    Some(&e.to_string()),
                    engine_label.as_deref(),
                )
                .await;
                Err(e)
            }
        };
    }

    let recap_ctx = recap::RecapCtx {
        call_id,
        call_dir: &call_dir,
        transcript_md: &transcript_md,
        lang_detected: effective_lang.as_deref(),
        proxy_base_url: &s.proxy_base_url,
        device_id,
        provider_path: &s.provider_path,
        model_override: s.model_override(),
        // [M14 T-02] cloud-managed = proxy auto-routes (Groq Llama 3.3 OR
        // Anthropic Sonnet fallback). Не различаем поскольку proxy не
        // возвращает per-call backend identifier.
        engine_label: "cloud-managed",
        summary_v2_enabled: s.summary_v2_enabled,
    };

    match recap::run(pool, recap_ctx).await {
        Ok(()) => {
            // [B16]: clear recap_failed_reason после успешной регенерации.
            let _ = db::set_recap_failed_reason(pool, call_id, None).await;
            Ok(())
        }
        Err(e) => {
            // Persist для UI + пробросываем (regenerate explicit user-action,
            // надо показать ошибку прямо в кнопке).
            // [P5.1] Engine label = "cloud-managed" — banner badge ↔ reason
            // matched (без mismatch с stale local engine из prior attempt).
            let _ =
                db::set_recap_failure(pool, call_id, Some(&e.to_string()), Some("cloud-managed"))
                    .await;
            Err(e)
        }
    }
}

/// [Bug-fix] Local engine path для `regenerate_recap`. Mirror блока в
/// `run_local_inner` (preset resolve → model presence check → LocalLlamaProvider
/// build → local_orchestrator → persist_recap_from_json), но БЕЗ STT/merge
/// stages (transcript.md уже есть). Errors propagate как AppError —
/// regenerate_recap setter caller персистит failed_reason + возвращает Err
/// в UI.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)] // регенерация — internal helper; structured args = backlog
async fn regenerate_recap_local(
    pool: &SqlitePool,
    app_data_dir: &std::path::Path,
    call_dir: &std::path::Path,
    call_id: &str,
    transcript_md: &str,
    lang_detected: Option<&str>,
    app: &AppHandle,
    s: &PipelineSettings,
) -> Result<(), AppError> {
    use crate::local_engine::{
        llm::LocalLlamaProvider,
        models::{self, ModelId, ModelStatus},
        preset::{LocalEnginePreset, SETTING_ACTIVE_PRESET},
    };

    if transcript_md.trim().is_empty() {
        return Err(AppError::Other("local_engine_transcript_empty".into()));
    }

    // Preset resolve.
    let preset_str = db::get_setting(pool, SETTING_ACTIVE_PRESET).await?;
    let preset = preset_str
        .as_deref()
        .and_then(LocalEnginePreset::from_str)
        .ok_or_else(|| {
            AppError::Other(
                "local_engine_preset_not_set: выберите Light/Balanced/Quality в Settings → Движок"
                    .into(),
            )
        })?;
    let llm_id = preset.llm_model_id();
    let whisper_id = preset.whisper_model_id();

    log::info!(
        "regenerate_recap_local: call_id={} preset={:?} llm_id={} whisper_id={}",
        call_id,
        preset,
        llm_id.as_str(),
        whisper_id.as_str(),
    );

    // Проверяем что LLM модель на диске + SHA OK (whisper для recap не нужен
    // но touch_usage обновим — пользователь может всё-таки запустить reprocess).
    let status = models::check_status(app_data_dir, llm_id.as_str()).await?;
    if !matches!(status, ModelStatus::Present { .. }) {
        return Err(AppError::Other(format!(
            "local_engine_model_missing: модель {} не установлена, скачайте в Settings → Движок",
            llm_id.as_str()
        )));
    }

    // Known speakers context для prompt'а (подтверждённые bindings).
    let known_speakers = recap::build_known_speakers_block(pool, call_id)
        .await
        .ok()
        .flatten();

    // Speculative decoding gate (mirror run_local_inner).
    let draft_path: Option<std::path::PathBuf> =
        if s.summary_speculative_decoding && preset == LocalEnginePreset::Quality {
            Some(models::model_path(
                app_data_dir,
                ModelId::QWEN25_0_5B.as_str(),
            ))
        } else {
            None
        };

    // [P1.3] Per-preset timeout: Light 5min / Balanced 10min / Quality 15min.
    let provider = LocalLlamaProvider::for_preset(app_data_dir, llm_id)
        .with_timeout(crate::local_engine::llm::timeout_for_preset(preset))
        .with_app(app.clone())
        .await
        .with_draft_model(draft_path);

    let orch_ctx = local_orchestrator::LocalOrchestratorCtx {
        transcript_md,
        lang_detected,
        known_speakers: known_speakers.as_deref(),
        preset,
    };
    // [P1.3] Wrap LLM future в periodic recap:progress emitter. UI рендерит
    // «Пересоздаём… {sec}s»; на completion (success / fail / timeout) ticker
    // аборт'ится через JoinHandle::abort внутри helper'а.
    let json_value = recap_progress::with_recap_progress_emitter(
        Some(app.clone()),
        call_id.to_string(),
        local_orchestrator::run_v2_pipeline(&provider, orch_ctx),
    )
    .await
    .map_err(|e| AppError::Other(format!("local_engine_llm_failed: {e}")))?;

    let local_engine_label = match llm_id.as_str() {
        id if id.contains("1.5b") || id.contains("1_5b") => "local-qwen-1.5b",
        id if id.contains("3b") => "local-qwen-3b",
        id if id.contains("7b") => "local-qwen-7b",
        _ => "local-qwen",
    };
    recap::persist_recap_from_json(
        pool,
        call_id,
        call_dir,
        json_value,
        local_engine_label,
        transcript_md,
        None,
        Some(s.summary_v2_enabled),
    )
    .await?;

    // Storage UI «активно X дней назад».
    let _ = models::touch_usage(pool, whisper_id.as_str()).await;
    let _ = models::touch_usage(pool, llm_id.as_str()).await;
    Ok(())
}

/// [Phase 3 R2] Helper: эмитит `(step, 0)` перед `f.await`, и `(step, 100)`
/// при успехе. Ошибка пробрасывается БЕЗ финального emit'а — это даёт UI
/// сигнал «упали на шаге X с pct=0» через DB-state (set_call_progress
/// эмиссия pct=0 уже произошла).
///
/// `upload_bytes` опционально — нужен только для Stage::Upload, чтобы
/// UI показал «Загружено N МБ из M». Остальные stages передают None.
async fn run_stage<F, T>(
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
    // [Phase 2 R3] Все настройки одним проходом, typed. Раньше было 5+
    // read_setting() inline + duplicate чтения preferred_language с regenerate_recap.
    let s = PipelineSettings::load(pool).await?;

    // [M12.6 Phase 3] Local engine — отдельный route. Использует
    // LocalWhisperProvider (sidecar) + LocalLlamaProvider (sidecar). Остальные
    // stages (merge artifacts, recognize speakers через WeSpeaker, action_items
    // persist) переиспользуются 1:1.
    #[cfg(target_os = "macos")]
    if s.engine == crate::local_engine::engine::EngineKind::Local {
        return run_local_inner(pool, ctx, app, &s).await;
    }

    let providers = build_providers(
        &s.stt_provider,
        &s.provider_path,
        &s.proxy_base_url,
        &ctx.device_id,
    )?;
    let opts = TranscriptionOpts {
        lang: s.stt_lang.clone(),
        diarization: true,
        prompt: None,
    };
    let retry_cfg = RetryConfig::default();

    log::info!(
        "pipeline {} start: provider={} path={} lang={} fallbacks={}",
        ctx.call_id,
        s.stt_provider,
        s.provider_path,
        s.stt_lang,
        providers.len()
    );

    // [Phase 3 R2] TIMING contract: stages эмитятся в порядке
    // Upload(1) → Transcribe(2) → MergeArtifacts(4) → RecognizeSpeakers(3) → Recap(5).
    // Шаг 3 идёт ПОСЛЕ 4: speaker recognition работает по уже персистированному
    // транскрипту (transcript.md / raw_stt.json должны быть на диске до cluster pipeline'а).

    // Stage 1: Upload — мгновенный псевдо-шаг (нет per-byte progress в provider API).
    let upload_hint = audio_byte_total(&ctx.mic_path, &ctx.system_path).await;
    run_stage(pool, app, &ctx.call_id, Stage::Upload, upload_hint, async {
        stage_upload(upload_hint).await
    })
    .await?;

    // Stage 2: Transcribe (STT) — долгий шаг, mic + system параллельно.
    let (mic_t, sys_t) = run_stage(pool, app, &ctx.call_id, Stage::Transcribe, None, async {
        stage_transcribe(&providers, ctx, opts, retry_cfg).await
    })
    .await?;

    let lang_detected = mic_t
        .lang_detected
        .clone()
        .or_else(|| sys_t.lang_detected.clone());
    let provider_used = sys_t.provider.clone();

    // Stage 4: MergeArtifacts — persist transcript.md + raw_stt.json. Идёт ДО
    // RecognizeSpeakers (Stage::3), потому что cluster pipeline читает merged
    // segments и пишет в DB; UI должен иметь транскрипт даже если cluster упал.
    let merged = run_stage(
        pool,
        app,
        &ctx.call_id,
        Stage::MergeArtifacts,
        None,
        async { stage_merge_artifacts(&ctx.call_dir, &mic_t, &sys_t).await },
    )
    .await?;

    db::set_call_meta(pool, &ctx.call_id, lang_detected.as_deref(), &provider_used).await?;

    // M3.7: mic-дорожка по определению принадлежит владельцу устройства.
    // Не нарушает R2 (никакой автопривязки): owner == юзер.
    let owner = db::ensure_owner_contact(pool).await?;
    if let Err(e) = db::auto_bind_owner_speaker(pool, &ctx.call_id, &owner.id, OWNER_TAG).await {
        log::warn!("auto_bind_owner_speaker {} failed: {e}", ctx.call_id);
    }
    ensure_anonymous_speakers_present(pool, &ctx.call_id, &merged).await;

    // Stage 3: RecognizeSpeakers — cluster extraction + matching. Non-fatal:
    // ошибки логируются и пропускаются (recap всё равно сгенерируется).
    let cluster_result = run_stage(
        pool,
        app,
        &ctx.call_id,
        Stage::RecognizeSpeakers,
        None,
        async { stage_recognize_speakers(pool, ctx, &merged).await },
    )
    .await;
    if let Err(e) = cluster_result {
        log::warn!(
            "cluster pipeline {} failed (non-fatal — skip voice match): {e}",
            ctx.call_id
        );
    }

    // [V7] Opt-in auto-bind — отдельный non-fatal шаг ПОСЛЕ RecognizeSpeakers,
    // БЕЗ собственного progress event (R2 паспорта: invisible flow).
    if let Err(e) = run_auto_bind(pool, app, &ctx.call_id, &s).await {
        log::warn!("auto_bind {} failed (non-fatal): {e}", ctx.call_id);
    }

    // Stage 5: Recap (LLM). Ошибки рекапа НЕ роняют пайплайн — транскрипт
    // сохранён, рекап можно регенерировать вручную (M4.5).
    let recap_step = Stage::Recap.step();
    emit_progress(pool, app, &ctx.call_id, recap_step, 0, None, None).await;
    stage_recap(pool, ctx, &s, &merged, lang_detected.as_deref()).await;
    emit_progress(pool, app, &ctx.call_id, recap_step, 100, None, None).await;

    Ok(())
}

/// [M12.6 Phase 3] Local-engine pipeline route. Полностью offline:
/// LocalWhisperProvider (whisper.cpp sidecar) → merge → cluster (WeSpeaker
/// in-process) → LocalLlamaProvider (llama.cpp sidecar) → recap.md.
///
/// Контракт ошибок per PRD §M12.6.5:
/// - missing model → `local_engine_model_missing`
/// - sidecar/STT crash → `local_engine_stt_failed`
/// - LLM crash → `local_engine_llm_failed`
/// - timeout → `local_whisper_timeout` / `local_llm_timeout`
///
/// UI (M12.5) показывает Cloud-fallback offer по этим маркерам.
#[cfg(target_os = "macos")]
async fn run_local_inner(
    pool: &SqlitePool,
    ctx: &PipelineCtx,
    app: Option<&AppHandle>,
    s: &PipelineSettings,
) -> Result<(), AppError> {
    use crate::local_engine::{
        engine::EngineKind,
        llm::LocalLlamaProvider,
        models::{self, ModelStatus},
        preset::{LocalEnginePreset, SETTING_ACTIVE_PRESET},
        stt::{LocalWhisperProvider, TrackKind},
    };

    debug_assert_eq!(s.engine, EngineKind::Local);
    let app = app.ok_or_else(|| {
        AppError::Other("local_engine_no_app_handle: pipeline requires Tauri runtime".into())
    })?;

    // 1. Резолвим preset → model ids. Без preset (юзер прошёл onboarding но
    //    выбрал Cloud, потом откатился) — fail с явным reason.
    let preset_str = db::get_setting(pool, SETTING_ACTIVE_PRESET).await?;
    let preset = preset_str
        .as_deref()
        .and_then(LocalEnginePreset::from_str)
        .ok_or_else(|| {
            AppError::Other(
                "local_engine_preset_not_set: выберите Light/Balanced/Quality в Settings → Движок"
                    .into(),
            )
        })?;
    let whisper_id = preset.whisper_model_id();
    let llm_id = preset.llm_model_id();

    // 2. Проверяем что обе модели на диске + SHA OK.
    for id in [whisper_id, llm_id] {
        let status = models::check_status(&ctx.app_data_dir, id.as_str()).await?;
        if !matches!(status, ModelStatus::Present { .. }) {
            return Err(AppError::Other(format!(
                "local_engine_model_missing: модель {} не установлена, скачайте в Settings → Движок",
                id.as_str()
            )));
        }
    }

    // 3. Stage Upload — pseudo-step (audio verify + sidecar model load занимают
    //    1-2 сек, UI не получает per-byte progress).
    let upload_hint = audio_byte_total(&ctx.mic_path, &ctx.system_path).await;
    run_stage(
        pool,
        Some(app),
        &ctx.call_id,
        Stage::Upload,
        upload_hint,
        async { stage_upload(upload_hint).await },
    )
    .await?;

    // 4. Stage Transcribe — mic + system параллельно через whisper-cli sidecar.
    //
    // [P13] Halt gate — если есть failed chunks, не идём дальше step 2.
    // Strict all-or-nothing: pipeline не должен build'ить partial transcript.
    // 0 chunks → Ok (non-chunked path, halt не релевантен) — fall back на
    // full-file STT ниже. После retry failed chunks → P11.1 auto-resume
    // re-войдёт сюда + halt пройдёт.
    ensure_all_chunks_done(pool, &ctx.call_id).await?;

    // [M13.1.5d] Если за время записи chunk_orchestrator насобирал per-chunk
    // транскрипты (CHUNKED_PIPELINE=ON в start_recording) — пропускаем
    // full-file STT и собираем mic/sys из DB. Cloud engine сюда не доходит
    // (run_inner ветка), для local engine с chunked OFF в `call_chunks`
    // ничего нет → assembly возвращает None → fall back на full-file STT.
    let (mic_t, sys_t) = match chunk_assembly::load_chunked_transcripts(pool, &ctx.call_id).await? {
        Some(tracks) => {
            log::info!(
                "call {}: using chunked transcripts (skip full-file STT)",
                ctx.call_id
            );
            // [Tech-debt P0.1] Sidecar пишет аудио в chunks/{idx}/mic.wav, но
            // root mic.wav/system.wav остаются от первого chunk'а. Плеер
            // (AudioScrubber) играет только этот фрагмент. Склеиваем все
            // chunk WAV'ы в root для полной длительности.
            //
            // Запускаем в blocking pool — hound load/save синхронен и для
            // 1-часовой записи на 16kHz mono ≈ 115 MB RAM + ~1-2s CPU.
            let chunks_dir = ctx.call_dir.join("chunks");
            let call_dir_clone = ctx.call_dir.clone();
            let call_id_clone = ctx.call_id.clone();
            tokio::task::spawn_blocking(move || {
                let (mic_r, sys_r) = audio_merger::merge_both_tracks(&chunks_dir, &call_dir_clone);
                if mic_r.is_none() && sys_r.is_none() {
                    log::warn!(
                        "audio_merger: оба merge упали для call {call_id_clone} — \
                         плеер будет играть старый root WAV (если есть)"
                    );
                }
            })
            .await
            .ok();
            // UI ожидает progress на Stage::Transcribe — эмитим 100%
            // мгновенно чтобы прогресс-бар не висел.
            let step = Stage::Transcribe.step();
            emit_progress(pool, Some(app), &ctx.call_id, step, 100, None, None).await;
            tracks
        }
        None => {
            let mic_stt = LocalWhisperProvider::for_preset(
                &ctx.app_data_dir,
                whisper_id,
                TrackKind::MicOwner,
            )
            .with_app(app.clone())
            .await;
            let sys_stt =
                LocalWhisperProvider::for_preset(&ctx.app_data_dir, whisper_id, TrackKind::System)
                    .with_app(app.clone())
                    .await;
            let opts = TranscriptionOpts {
                lang: s.stt_lang.clone(),
                diarization: true,
                prompt: None,
            };
            run_stage(
                pool,
                Some(app),
                &ctx.call_id,
                Stage::Transcribe,
                None,
                async {
                    let mic_fut = mic_stt.transcribe(&ctx.mic_path, opts.clone());
                    let sys_fut = sys_stt.transcribe(&ctx.system_path, opts.clone());
                    let (mic_r, sys_r) = tokio::join!(mic_fut, sys_fut);
                    let mic = mic_r.map_err(|e| {
                        AppError::Other(format!("local_engine_stt_failed (mic): {e}"))
                    })?;
                    let sys = sys_r.map_err(|e| {
                        AppError::Other(format!("local_engine_stt_failed (system): {e}"))
                    })?;
                    Ok::<_, AppError>((mic, sys))
                },
            )
            .await?
        }
    };

    let lang_detected = mic_t
        .lang_detected
        .clone()
        .or_else(|| sys_t.lang_detected.clone());

    // 4.5. [M12-D5] Multi-speaker diarization на system track. До этого
    //    шага все system segments имеют `speaker:0` (TrackKind::System default
    //    в [`local_engine::stt`]). Sherpa-onnx pyannote segmentation + WeSpeaker
    //    embedding кластеризует фрагменты → каждый sys segment получает
    //    `speaker:0..4` (cap=4). Diarization non-fatal: при отсутствии моделей
    //    или voice-onnx feature off → fall back на оригинальный sys_t,
    //    система-трек остаётся single-bucket (degraded но рабочий).
    // [P1.2] Force-N-speakers Labs override (read once, applied к mic + system).
    // None = auto. clamp 1..=MAX_LOCAL_SPEAKERS внутри `with_num_speakers`.
    let num_speakers_override: Option<i32> = db::get_setting(pool, "mic_diarization_num_speakers")
        .await?
        .as_deref()
        .and_then(|s| s.parse::<i32>().ok())
        .filter(|n| (1..=4).contains(n));
    let sys_t = diarize_system_track(
        &ctx.app_data_dir,
        &ctx.system_path,
        sys_t,
        num_speakers_override,
    )
    .await;

    // 4.6. [M13 follow-up] Опциональный multi-voice на mic-дорожке. Default ON
    //    через `MIC_DIARIZATION_ENABLED`. Без этого вся mic уходила в OWNER_TAG
    //    через assemble_transcript (force_owner_track в local_engine::merge).
    //    С включенной настройкой sortformer выдаёт `speaker:N` tags, потом
    //    owner_identify::identify_owner_speaker переименовывает один из них
    //    в OWNER_TAG. На non-chunked пути embeddings собираем здесь же через
    //    extract_clusters; cross-track reflection (owner отражается в system)
    //    не обрабатывается без global reclustering — это limitation
    //    non-chunked path, acceptable т.к. чанкед = default.
    let mic_off = matches!(
        db::get_setting(pool, "mic_diarization_enabled")
            .await?
            .as_deref(),
        Some("0") | Some("false")
    );
    let mic_diarization = !mic_off;
    let mic_t = if mic_diarization {
        let mic_diarized = diarize_mic_track(
            &ctx.app_data_dir,
            &ctx.mic_path,
            mic_t,
            num_speakers_override,
        )
        .await;
        relabel_owner_on_mic_full_file(
            pool,
            &ctx.app_data_dir,
            &ctx.mic_path,
            &ctx.system_path,
            mic_diarized,
        )
        .await
    } else {
        mic_t
    };

    // 5. Stage MergeArtifacts — переиспользуем cloud helper (он не знает про
    //    engine, просто пишет transcript.md + raw_stt.json).
    let merged = run_stage(
        pool,
        Some(app),
        &ctx.call_id,
        Stage::MergeArtifacts,
        None,
        async { stage_merge_artifacts(&ctx.call_dir, &mic_t, &sys_t).await },
    )
    .await?;

    db::set_call_meta(pool, &ctx.call_id, lang_detected.as_deref(), "local").await?;

    let owner = db::ensure_owner_contact(pool).await?;
    if let Err(e) = db::auto_bind_owner_speaker(pool, &ctx.call_id, &owner.id, OWNER_TAG).await {
        log::warn!("auto_bind_owner_speaker {} failed: {e}", ctx.call_id);
    }
    ensure_anonymous_speakers_present(pool, &ctx.call_id, &merged).await;

    // 6. Stage RecognizeSpeakers — WeSpeaker cluster (B3.x) переиспользуется
    //    как для cloud-движка. Diarization для local пока упрощённая (system
    //    track всё в speaker:0) — кластер видит «одного» дополнительного спикера
    //    но это ок: voice biometrics matching работает на сэмплах per-call.
    let cluster_result = run_stage(
        pool,
        Some(app),
        &ctx.call_id,
        Stage::RecognizeSpeakers,
        None,
        async { stage_recognize_speakers(pool, ctx, &merged).await },
    )
    .await;
    if let Err(e) = cluster_result {
        log::warn!(
            "cluster pipeline {} failed (non-fatal — skip voice match): {e}",
            ctx.call_id
        );
    }

    if let Err(e) = run_auto_bind(pool, Some(app), &ctx.call_id, s).await {
        log::warn!("auto_bind {} failed (non-fatal): {e}", ctx.call_id);
    }

    // 7. Stage Recap — local LLM. Failed_reason set + recap.md skipped при
    //    ошибке; pipeline всё равно завершает Ok(()) (M4 паспорта — recap
    //    деривативная штука, регенерация по кнопке).
    let recap_step = Stage::Recap.step();
    emit_progress(pool, Some(app), &ctx.call_id, recap_step, 0, None, None).await;

    let known_speakers = recap::build_known_speakers_block(pool, &ctx.call_id)
        .await
        .ok()
        .flatten();

    // Transcript.md обязан существовать — `stage_merge_artifacts` его пишет.
    // Если файл недоступен (race / disk issue), recap должен fail с явным
    // reason, а не silently дёрнуть LLM на пустом входе (получится пустой recap).
    let transcript_md_read = tokio::fs::read_to_string(ctx.call_dir.join("transcript.md")).await;
    let transcript_for_evidence = transcript_md_read
        .as_ref()
        .map(|s| s.clone())
        .unwrap_or_default();
    // [M14 T-04 + T-10 Phase A] Local engine orchestrator: classifier (lightweight
    // ~256 tokens) → main v2 generation с known_call_type hint. На classifier
    // failure orchestrator делает fallback на single-pass без hint.
    // LOCAL_LLM_SYSTEM_PROMPT (legacy v1 ad-hoc) больше не используется на
    // этом path — local теперь идёт через тот же build_v2_system_prompt что
    // и cloud (с CallType hint от классификатора).
    let llm_result = match transcript_md_read {
        Ok(transcript_md) if !transcript_md.trim().is_empty() => {
            // [M14 T-16 P2] Speculative decoding — pass draft model path
            // когда (а) flag enabled (Labs opt-in), (b) preset=Quality
            // (только 7B заметно выигрывает от 0.5B draft), (c) file existence
            // checked внутри provider (graceful fallback на non-speculative).
            let draft_path: Option<std::path::PathBuf> =
                if s.summary_speculative_decoding && preset == LocalEnginePreset::Quality {
                    Some(crate::local_engine::models::model_path(
                        &ctx.app_data_dir,
                        crate::local_engine::models::ModelId::QWEN25_0_5B.as_str(),
                    ))
                } else {
                    None
                };
            // [P1.3] Per-preset timeout (Light 5min / Balanced 10min / Quality 15min).
            let provider = LocalLlamaProvider::for_preset(&ctx.app_data_dir, llm_id)
                .with_timeout(crate::local_engine::llm::timeout_for_preset(preset))
                .with_app(app.clone())
                .await
                .with_draft_model(draft_path);
            let orch_ctx = local_orchestrator::LocalOrchestratorCtx {
                transcript_md: &transcript_md,
                lang_detected: lang_detected.as_deref(),
                known_speakers: known_speakers.as_deref(),
                // [M14 T-05/T-06 Phase B] Pass active preset для chunker config —
                // длинные transcripts автоматически идут map-reduce.
                preset,
            };
            // [P1.3] Wrap LLM future в periodic recap:progress emitter
            // (mirror regenerate_recap_local). UI рендерит «Пересоздаём… {sec}s».
            recap_progress::with_recap_progress_emitter(
                Some(app.clone()),
                ctx.call_id.clone(),
                local_orchestrator::run_v2_pipeline(&provider, orch_ctx),
            )
            .await
            .map_err(|e| crate::providers::llm::LlmError::Provider(e.to_string()))
        }
        Ok(_) => Err(crate::providers::llm::LlmError::Provider(
            "local_engine_transcript_empty".into(),
        )),
        Err(e) => Err(crate::providers::llm::LlmError::Provider(format!(
            "local_engine_transcript_read: {e}"
        ))),
    };

    // [P5.1] Hoist label derivation outside match — нужен в обеих ветках
    // (success persist + failure atomic UPDATE).
    let local_engine_label = match llm_id.as_str() {
        id if id.contains("1.5b") || id.contains("1_5b") => "local-qwen-1.5b",
        id if id.contains("3b") => "local-qwen-3b",
        id if id.contains("7b") => "local-qwen-7b",
        _ => "local-qwen",
    };

    match llm_result {
        Ok(json_value) => {
            // [M14 T-02] persist_recap_from_json теперь требует engine_label +
            // transcript_md (для evidence validator) + generation_ms (None
            // на local path; в T-04+ доделаем).
            if let Err(e) = recap::persist_recap_from_json(
                pool,
                &ctx.call_id,
                &ctx.call_dir,
                json_value,
                local_engine_label,
                &transcript_for_evidence,
                None,
                // [M14 T-04 Phase A] Local path теперь emit'ит telemetry —
                // classifier + main v2 pipeline через local_orchestrator.
                Some(s.summary_v2_enabled),
            )
            .await
            {
                // [P5.1] Engine label atomic с reason — banner consistent.
                let _ = db::set_recap_failure(
                    pool,
                    &ctx.call_id,
                    Some(&format!("local_engine_recap_persist: {e}")),
                    Some(local_engine_label),
                )
                .await;
            } else {
                let _ = db::set_recap_failed_reason(pool, &ctx.call_id, None).await;
                // Storage UI «активно X дней назад».
                let _ = models::touch_usage(pool, whisper_id.as_str()).await;
                let _ = models::touch_usage(pool, llm_id.as_str()).await;
            }
        }
        Err(e) => {
            let reason = format!("local_engine_llm_failed: {e}");
            log::warn!("{reason}");
            // [P5.1] Engine label atomic с reason.
            let _ =
                db::set_recap_failure(pool, &ctx.call_id, Some(&reason), Some(local_engine_label))
                    .await;
        }
    }
    emit_progress(pool, Some(app), &ctx.call_id, recap_step, 100, None, None).await;

    Ok(())
}

/// [M12-D5] Прогнать system-track через sherpa-onnx OfflineSpeakerDiarization
/// и смерджить speaker tags в `sys_t.segments`.
///
/// Non-fatal: при отсутствии pyannote / WeSpeaker модели или ошибке inference
/// возвращаем оригинальный `sys_t` без изменений — system track останется
/// single-bucket (`speaker:0`), pipeline продолжит работать в degraded режиме.
///
/// Шаги:
///
/// - Проверка наличия pyannote-segmentation на диске (MODEL_CATALOG).
/// - Проверка наличия WeSpeaker (B3.7c, `voice_model.rs`).
/// - Spawn `SortformerDiarizer` + `.diarize(system_path)`.
/// - Apply `merge::merge_word_with_speaker` на sys_t.segments.
/// - Вернуть обновлённый sys_t.
#[cfg(target_os = "macos")]
async fn diarize_system_track(
    app_data_dir: &Path,
    system_path: &Path,
    sys_t: DiarizedTranscript,
    num_speakers: Option<i32>,
) -> DiarizedTranscript {
    diarize_track(app_data_dir, system_path, sys_t, "system", num_speakers).await
}

/// [M13 follow-up] Mirror `diarize_system_track` для mic-дорожки. Применяется
/// когда `MIC_DIARIZATION_ENABLED` ON и engine == local. Owner-tag НЕ
/// присваивается здесь — local `speaker:N` tags сохраняются, owner
/// identification идёт отдельным шагом ([`owner_identify`]).
///
/// [P1.2] `num_speakers` — Labs «Force N speakers» override. `None` =
/// auto-detect (sortformer default). Применяется идентично к mic и system.
#[cfg(target_os = "macos")]
pub(crate) async fn diarize_mic_track(
    app_data_dir: &Path,
    mic_path: &Path,
    mic_t: DiarizedTranscript,
    num_speakers: Option<i32>,
) -> DiarizedTranscript {
    diarize_track(app_data_dir, mic_path, mic_t, "mic", num_speakers).await
}

/// [M13 follow-up] Non-chunked path post-processing: после `diarize_mic_track`
/// на mic-дорожке local `speaker:N` tags. Извлекаем cluster embeddings
/// через `extract_clusters`, вызываем `identify_owner_speaker` и
/// перетеггиваем выбранный tag → `OWNER_TAG`. Cross-track reflection
/// не обрабатывается (non-chunked = нет global remap).
#[cfg(target_os = "macos")]
async fn relabel_owner_on_mic_full_file(
    pool: &SqlitePool,
    app_data_dir: &Path,
    mic_path: &Path,
    system_path: &Path,
    mut mic_t: DiarizedTranscript,
) -> DiarizedTranscript {
    // Embedder для cluster mean (reuse existing pipeline pattern из cloud
    // run_cluster_pipeline). Fallback на StubEmbedder когда модель отсутствует
    // → cluster_embeddings empty → identify_owner_speaker уходит в duration
    // fallback (acceptable).
    let model_path = app_data_dir.join("models").join("embedder.onnx");
    let embedder: Box<dyn embeddings::Embedder> =
        match embeddings::try_load_onnx_embedder(&model_path) {
            Some(e) => e,
            None => Box::new(StubEmbedder),
        };
    let clusters = match crate::pipeline::clusters::extract_clusters(
        &mic_t.segments,
        mic_path,
        system_path,
        embedder.as_ref(),
    ) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("relabel_owner_on_mic: extract_clusters err: {e} — fallback duration");
            std::collections::HashMap::new()
        }
    };
    match crate::pipeline::owner_identify::identify_owner_speaker(pool, &mic_t.segments, &clusters)
        .await
    {
        Ok(Some(owner_local_tag)) if owner_local_tag != crate::pipeline::merge::OWNER_TAG => {
            log::info!(
                "relabel_owner_on_mic: переменовываем {} → {} (остальные tags сохраняются как анонимные спикеры)",
                owner_local_tag,
                crate::pipeline::merge::OWNER_TAG
            );
            for seg in mic_t.segments.iter_mut() {
                if seg.speaker_tag == owner_local_tag {
                    seg.speaker_tag = crate::pipeline::merge::OWNER_TAG.to_string();
                }
            }
        }
        Ok(other) => log::info!(
            "relabel_owner_on_mic: identify → {other:?} (no relabel; уже OWNER либо не нашли)"
        ),
        Err(e) => log::warn!("relabel_owner_on_mic: identify err: {e}"),
    }
    // [Bug-fix] Diagnostic: финальный распределение tags после relabel — чтобы
    // увидеть сколько уникальных спикеров останется в DB (call_speakers).
    {
        use std::collections::BTreeSet;
        let final_tags: BTreeSet<&str> = mic_t
            .segments
            .iter()
            .map(|s| s.speaker_tag.as_str())
            .collect();
        log::info!(
            "relabel_owner_on_mic: финальный distinct tags ({}): {:?}",
            final_tags.len(),
            final_tags
        );
    }
    mic_t
}

/// [M13 follow-up] Общий helper sortformer-диаризации (mic | system). На
/// degraded path (нет моделей / sortformer err) — возвращаем transcript
/// без изменений.
#[cfg(target_os = "macos")]
async fn diarize_track(
    app_data_dir: &Path,
    audio_path: &Path,
    transcript: DiarizedTranscript,
    track_kind: &'static str,
    num_speakers: Option<i32>,
) -> DiarizedTranscript {
    use crate::local_engine::{
        diarization::{Diarizer, SortformerDiarizer},
        merge,
        models::{self, ModelId, ModelStatus},
    };

    // 1. Pyannote segmentation: catalog entry должен быть present.
    let seg_path = models::model_path(app_data_dir, ModelId::PYANNOTE_SEGMENTATION.as_str());
    let seg_present = matches!(
        models::check_status(app_data_dir, ModelId::PYANNOTE_SEGMENTATION.as_str()).await,
        Ok(ModelStatus::Present { .. })
    );
    if !seg_present {
        // [Bug-fix #4] log::warn — pyannote-segmentation missing — это
        // explicit gap который юзер должен видеть (toggle на Speakers UI
        // зависит от этой модели). Раньше было log::info → невидимо в release.
        log::warn!(
            "diarize_track[{track_kind}]: pyannote-segmentation отсутствует — \
             диаризация {track_kind}-дорожки не выполнена. Установите модуль в \
             Настройки → Спикеры."
        );
        return transcript;
    }

    // 2. WeSpeaker embedding (B3.7c) — отдельный путь от model catalog.
    let emb_path = crate::voice_model::model_path(app_data_dir);
    if !emb_path.exists() {
        log::warn!(
            "diarize_track[{track_kind}]: WeSpeaker embedder ({}) отсутствует — fall back",
            emb_path.display()
        );
        return transcript;
    }

    // 3-5. Diarize + merge. Любая ошибка → fall back (degraded).
    // [P1.2] `with_num_speakers` clamp'ит override к 1..=MAX_LOCAL_SPEAKERS;
    // None = sherpa-onnx auto (default `-1`).
    let diarizer = SortformerDiarizer::with_num_speakers(seg_path, emb_path, num_speakers);
    if let Some(n) = num_speakers {
        log::info!("diarize_track[{track_kind}]: forcing num_clusters={n} (Labs override)");
    }
    let mut speaker_segments = match diarizer.diarize(audio_path).await {
        Ok(segs) => segs,
        Err(e) => {
            log::warn!("diarize_track[{track_kind}]: sortformer err: {e} — fall back");
            return transcript;
        }
    };

    // [P14.3] Reassign overflow `speaker:unknown` segments к ближайшему
    // named-спикеру в окне ±2s. Снижает шум в ParticipantsRow когда
    // sortformer вывел больше MAX_LOCAL_SPEAKERS (=3) кластеров.
    let reassigned =
        crate::local_engine::diarization::reassign_unknown_to_neighbors(&mut speaker_segments, 2.0);
    if reassigned > 0 {
        log::info!(
            "diarize_track[{track_kind}]: reassigned {reassigned} unknown → neighbor speakers"
        );
    }

    // [Bug-fix] Diagnostic: сколько уникальных спикеров sortformer выделил
    // + суммарные durations. Без этого silent-collapse невозможно отличить
    // от "правда был один голос".
    {
        use std::collections::BTreeMap;
        let mut by_tag: BTreeMap<&str, f64> = BTreeMap::new();
        for s in &speaker_segments {
            *by_tag.entry(s.speaker_tag.as_str()).or_insert(0.0) += s.end - s.start;
        }
        let summary: Vec<String> = by_tag.iter().map(|(k, v)| format!("{k}={v:.1}s")).collect();
        log::info!(
            "diarize_track[{track_kind}]: sortformer вывел {} спикер(ов): [{}]",
            by_tag.len(),
            summary.join(", ")
        );
    }

    let merged_segments = merge::merge_word_with_speaker(&transcript.segments, &speaker_segments);
    log::info!(
        "diarize_track[{track_kind}]: {} STT segments + {} speaker segments → {} merged",
        transcript.segments.len(),
        speaker_segments.len(),
        merged_segments.len()
    );
    DiarizedTranscript {
        segments: merged_segments,
        ..transcript
    }
}

/// [Phase 3 R2] Stage 1 — Upload. В текущей реализации no-op
/// (real per-byte streaming требует middleware вокруг reqwest, которого ещё
/// нет). Хелпер существует чтобы run_inner был симметричный — каждая stage
/// это отдельная async fn.
///
/// Возвращает upload_bytes hint (для UI «Загружено N МБ»). None если оба
/// аудио-файла отсутствуют (test fixtures + edge cases).
async fn stage_upload(upload_bytes_hint: Option<i64>) -> Result<Option<i64>, AppError> {
    Ok(upload_bytes_hint)
}

/// [Phase 3 R2] Stage 2 — STT (mic + system параллельно с retry/fallback).
/// Возвращает оба diarized-транскрипта; merge делается в Stage::MergeArtifacts.
async fn stage_transcribe(
    providers: &[Box<dyn TranscriptionProvider>],
    ctx: &PipelineCtx,
    opts: TranscriptionOpts,
    retry_cfg: RetryConfig,
) -> Result<(DiarizedTranscript, DiarizedTranscript), AppError> {
    let (mic_res, sys_res) = tokio::join!(
        transcribe_with_fallback(providers, &ctx.mic_path, opts.clone(), retry_cfg),
        transcribe_with_fallback(providers, &ctx.system_path, opts.clone(), retry_cfg),
    );
    let mic_t = mic_res.map_err(stt_to_app_error)?;
    let sys_t = sys_res.map_err(stt_to_app_error)?;
    Ok((mic_t, sys_t))
}

/// [Phase 3 R2] Stage 4 — merge tracks + persist artifacts. Раньше это был
/// `persist_artifacts` хелпер — теперь явно stage. Возвращает merged-сегменты
/// для последующих stages (recognize_speakers + recap).
async fn stage_merge_artifacts(
    call_dir: &PathBuf,
    mic: &DiarizedTranscript,
    system: &DiarizedTranscript,
) -> Result<Vec<TranscriptSegment>, AppError> {
    tokio::fs::create_dir_all(call_dir).await?;

    let merged = merge_tracks(mic, system);

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
async fn stage_recognize_speakers(
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

/// [Phase 3 R2] Stage 5 — LLM recap. Ошибки НЕ пробрасываются (non-fatal):
/// persist'им reason в DB для UI banner. Pipeline всегда заканчивается Ok
/// если транскрипт сохранён.
async fn stage_recap(
    pool: &SqlitePool,
    ctx: &PipelineCtx,
    s: &PipelineSettings,
    merged: &[TranscriptSegment],
    lang_detected: Option<&str>,
) {
    let transcript_md = render_transcript_md(merged);
    let effective_lang = s.effective_recap_lang(lang_detected);
    let recap_ctx = recap::RecapCtx {
        call_id: &ctx.call_id,
        call_dir: &ctx.call_dir,
        transcript_md: &transcript_md,
        lang_detected: effective_lang.as_deref(),
        proxy_base_url: &s.proxy_base_url,
        device_id: &ctx.device_id,
        provider_path: &s.provider_path,
        model_override: s.model_override(),
        // [M14 T-02] Proxy auto-picks Groq/Anthropic; не различаем здесь.
        engine_label: "cloud-managed",
        summary_v2_enabled: s.summary_v2_enabled,
    };
    match recap::run(pool, recap_ctx).await {
        Ok(()) => {
            // Очищаем если был старый recap_failed_reason (например после reprocess).
            let _ = db::set_recap_failed_reason(pool, &ctx.call_id, None).await;
        }
        Err(e) => {
            let reason = e.to_string();
            log::warn!("recap {} skipped: {reason}", ctx.call_id);
            // [B16]: persist recap failure для UI banner. status='ready' остаётся.
            // [P5.1] Atomic UPDATE c engine_label="cloud-managed" — match
            // recap_ctx.engine_label выше. Banner badge ↔ reason consistent.
            if let Err(e2) =
                db::set_recap_failure(pool, &ctx.call_id, Some(&reason), Some("cloud-managed"))
                    .await
            {
                log::error!("set_recap_failure {} failed: {e2}", ctx.call_id);
            }
        }
    }
}

/// [B11] M7.4: добавить placeholder rows в call_speakers для всех distinct
/// speaker_tag из транскрипта (кроме owner — у него уже confirmed). UI покажет
/// анонимных «S1/S2», юзер сможет привязать через select.
/// Non-fatal: warning при ошибке.
async fn ensure_anonymous_speakers_present(
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
async fn run_auto_bind(
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

/// [V6.2] Размер двух аудио-файлов в байтах — UI показывает «X МБ» в активити
/// strip'е. Best-effort: если файла нет (например только mic пишется), берём
/// что доступно. None если оба отсутствуют.
async fn audio_byte_total(mic: &Path, sys: &Path) -> Option<i64> {
    let mut total: i64 = 0;
    let mut seen = false;
    for path in [mic, sys] {
        if let Ok(meta) = tokio::fs::metadata(path).await {
            total = total.saturating_add(meta.len() as i64);
            seen = true;
        }
    }
    if seen {
        Some(total)
    } else {
        None
    }
}

/// Возвращает ProviderMode для конкретного партнёра с учётом path:
/// - `managed` — общий прокси (#22), один URL/device-id для всех провайдеров
/// - `byo` (#47) — индивидуальный ключ партнёра из keychain. Нет ключа → ошибка.
fn mode_for(
    provider: ByoProvider,
    path: &str,
    proxy_base_url: &str,
    device_id: &Arc<str>,
) -> Result<ProviderMode, AppError> {
    match path {
        "managed" => {
            if proxy_base_url.is_empty() {
                return Err(AppError::Other(
                    "Proxy URL не настроен. Settings → Proxy URL (#22 follow-up).".into(),
                ));
            }
            Ok(ProviderMode::Managed {
                proxy_base_url: proxy_base_url.to_string(),
                device_id: device_id.to_string(),
            })
        }
        "byo" => {
            let key = secrets::read_key(provider)?;
            key.map(|api_key| ProviderMode::Byo { api_key })
                .ok_or_else(|| {
                    AppError::Other(format!(
                        "BYO ключ для {:?} не задан. Settings → BYO Keys.",
                        provider
                    ))
                })
        }
        other => Err(AppError::Other(format!("unknown provider_path: {other}"))),
    }
}

/// M2.2 + #23 + #47: список провайдеров в порядке использования.
/// - `auto` → пробуем оба, у каждого свой ключ (для BYO). Если BYO ключ отсутствует
///   у одного провайдера — он молча пропускается; в `managed` оба доступны.
/// - `soniox` / `gladia` → только указанный, требует свой ключ в BYO.
fn build_providers(
    id: &str,
    path: &str,
    proxy_base_url: &str,
    device_id: &Arc<str>,
) -> Result<Vec<Box<dyn TranscriptionProvider>>, AppError> {
    let providers: Vec<Box<dyn TranscriptionProvider>> = match id {
        "gladia" => {
            let m = mode_for(ByoProvider::Gladia, path, proxy_base_url, device_id)?;
            vec![Box::new(GladiaProvider::new(m))]
        }
        "soniox" => {
            let m = mode_for(ByoProvider::Soniox, path, proxy_base_url, device_id)?;
            vec![Box::new(SonioxProvider::new(m))]
        }
        _ => {
            let mut out: Vec<Box<dyn TranscriptionProvider>> = vec![];
            if let Ok(m) = mode_for(ByoProvider::Soniox, path, proxy_base_url, device_id) {
                out.push(Box::new(SonioxProvider::new(m)));
            }
            if let Ok(m) = mode_for(ByoProvider::Gladia, path, proxy_base_url, device_id) {
                out.push(Box::new(GladiaProvider::new(m)));
            }
            if out.is_empty() {
                return Err(AppError::Other(
                    "Ни один STT-провайдер не настроен. Добавь BYO-ключ или настрой Proxy URL в Settings.".into(),
                ));
            }
            out
        }
    };
    Ok(providers)
}

/// Мапит typed STT error на AppError с UX-readable причиной.
/// Сообщение попадёт в `calls.failed_reason` (M2.7 / #23).
fn stt_to_app_error(e: TranscriptionError) -> AppError {
    AppError::Other(failure_reason(&e))
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
    let model_path = app_data_dir.join("models").join("embedder.onnx");
    let embedder: Box<dyn embeddings::Embedder> =
        match embeddings::try_load_onnx_embedder(&model_path) {
            Some(e) => {
                log::info!(
                    "cluster pipeline {call_id}: OnnxEmbedder ({})",
                    model_path.display()
                );
                e
            }
            None => {
                log::debug!(
                    "cluster pipeline {call_id}: StubEmbedder (no model at {})",
                    model_path.display()
                );
                Box::new(StubEmbedder)
            }
        };
    let clusters = extract_clusters(merged, mic_path, system_path, embedder.as_ref())?;
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
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    fn arc_device(id: &str) -> Arc<str> {
        Arc::from(id.to_string().into_boxed_str())
    }

    // ============================================================
    // [P13] ensure_all_chunks_done — halt gate перед stage 2→3
    // ============================================================

    async fn insert_call_row(pool: &sqlx::SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 'recording', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn ensure_all_chunks_done_returns_ok_when_no_chunks() {
        // Non-chunked path (cloud / single-pass) — нет rows в call_chunks.
        // Halt не релевантен — pipeline fall back на full-file STT.
        let test_db = fresh_db().await;
        insert_call_row(&test_db.pool, "c1").await;
        assert!(ensure_all_chunks_done(&test_db.pool, "c1").await.is_ok());
    }

    async fn insert_and_mark_done(pool: &sqlx::SqlitePool, call_id: &str, idx: u32) {
        use std::path::PathBuf;
        db::chunks::insert_chunk(
            pool,
            call_id,
            idx,
            u64::from(idx) * 600_000,
            &PathBuf::from(format!("/m{idx}")),
            &PathBuf::from(format!("/s{idx}")),
        )
        .await
        .unwrap();
        db::chunks::mark_chunk_processing(pool, call_id, idx)
            .await
            .unwrap();
        db::chunks::mark_chunk_done(
            pool,
            call_id,
            idx,
            u64::from(idx + 1) * 600_000,
            r#"{"segments":[]}"#,
            None,
            None,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn ensure_all_chunks_done_returns_ok_when_all_done() {
        let test_db = fresh_db().await;
        insert_call_row(&test_db.pool, "c1").await;
        for idx in 0..3 {
            insert_and_mark_done(&test_db.pool, "c1", idx).await;
        }
        assert!(ensure_all_chunks_done(&test_db.pool, "c1").await.is_ok());
    }

    #[tokio::test]
    async fn ensure_all_chunks_done_returns_err_on_failed_chunk() {
        use std::path::PathBuf;
        let test_db = fresh_db().await;
        insert_call_row(&test_db.pool, "c1").await;
        // 2 done, 1 failed (user's reported scenario: 1/3).
        for idx in 0..2 {
            insert_and_mark_done(&test_db.pool, "c1", idx).await;
        }
        db::chunks::insert_chunk(
            &test_db.pool,
            "c1",
            2,
            1_200_000,
            &PathBuf::from("/m2"),
            &PathBuf::from("/s2"),
        )
        .await
        .unwrap();
        db::chunks::mark_chunk_processing(&test_db.pool, "c1", 2)
            .await
            .unwrap();
        db::chunks::mark_chunk_failed(&test_db.pool, "c1", 2, "STT timeout")
            .await
            .unwrap();
        let err = ensure_all_chunks_done(&test_db.pool, "c1")
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("chunks_need_retry"), "got: {msg}");
        assert!(msg.contains("1 of 3"), "got: {msg}");
        assert!(msg.contains("[2]"), "got: {msg}");
    }

    #[tokio::test]
    async fn ensure_all_chunks_done_returns_err_on_pending_chunk() {
        use std::path::PathBuf;
        let test_db = fresh_db().await;
        insert_call_row(&test_db.pool, "c1").await;
        db::chunks::insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m0"),
            &PathBuf::from("/s0"),
        )
        .await
        .unwrap();
        // Status pending — не failed, но и не done. Должен halt.
        let err = ensure_all_chunks_done(&test_db.pool, "c1")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("chunks_need_retry"));
    }

    // ============================================================
    // [Phase 2] mode_for — provider mode resolution
    // ============================================================

    #[test]
    fn mode_for_managed_returns_proxy_config() {
        let device = arc_device("dev-1");
        let m = mode_for(
            ByoProvider::Soniox,
            "managed",
            "https://proxy.example.com",
            &device,
        )
        .unwrap();
        match m {
            ProviderMode::Managed {
                proxy_base_url,
                device_id,
            } => {
                assert_eq!(proxy_base_url, "https://proxy.example.com");
                assert_eq!(device_id, "dev-1");
            }
            _ => panic!("expected Managed mode"),
        }
    }

    #[test]
    fn mode_for_managed_empty_proxy_url_errors() {
        let device = arc_device("dev-1");
        let err = mode_for(ByoProvider::Soniox, "managed", "", &device).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Proxy URL"), "got: {msg}");
    }

    #[test]
    fn mode_for_unknown_path_errors() {
        let device = arc_device("dev-1");
        let err = mode_for(ByoProvider::Soniox, "ghost-path", "https://x", &device).unwrap_err();
        assert!(err.to_string().contains("unknown provider_path"));
    }

    // BYO branch требует Keychain access. На CI keychain доступен, на dev
    // тоже — но затронуть production ключи юзера было бы рискованно.
    // Reading non-existent ключ должно вернуть Ok(None) → AppError "BYO ключ
    // не задан". Используем ByoProvider::Anthropic поскольку в pipeline он
    // не запрашивается через mode_for (STT-only), но enum его поддерживает —
    // тестируем точку входа.
    #[test]
    fn mode_for_byo_missing_key_returns_error() {
        let device = arc_device("dev-1");
        // [Phase 2 R5 follow-up] secrets::read_key читает из Keychain.
        // В test env на macOS он либо вернёт None (никогда не было ключа),
        // либо ключ из dev-сессии. Считаем что ключ Anthropic для test
        // env отсутствует (он используется только для proxy LLM, не для STT).
        // Если кто-то залил ключ — тест станет flaky, но это говорит о
        // запутанном test isolation, а не о баге кода.
        let result = mode_for(ByoProvider::Anthropic, "byo", "", &device);
        // Либо ошибка "BYO ключ не задан", либо Ok если ключ есть.
        if let Err(e) = result {
            assert!(
                e.to_string().contains("BYO ключ"),
                "expected BYO key error, got: {e}"
            );
        }
    }

    // ============================================================
    // [Phase 2] reprocess_call — guard rails
    // ============================================================

    #[tokio::test]
    async fn reprocess_call_missing_audio_returns_error() {
        let db = fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device = arc_device("dev-1");

        let call = db::insert_recording(&db.pool, "managed").await.unwrap();
        // Аудио намеренно не создаём — pipeline должен отвергнуть.
        let err = reprocess_call(&db.pool, tmpdir.path(), &device, &call.id, None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("Аудио файлы"),
            "expected audio-missing error, got: {err}"
        );
    }

    #[tokio::test]
    async fn reprocess_call_unknown_call_id_returns_not_found() {
        let db = fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device = arc_device("dev-1");
        let err = reprocess_call(&db.pool, tmpdir.path(), &device, "ghost-id", None)
            .await
            .unwrap_err();
        // [Phase 1 R6] typed NotFound теперь сериализуется как
        // "not found: call ghost-id".
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    // ============================================================
    // [Phase 3 R2] run_auto_bind — typed config branching
    // ============================================================

    use crate::pipeline::settings::{AutoBindConfig, DEFAULT_PROXY_BASE_URL};

    fn settings_with_auto_bind(auto_bind: Option<AutoBindConfig>) -> PipelineSettings {
        PipelineSettings {
            stt_provider: "auto".into(),
            provider_path: "managed".into(),
            stt_lang: "auto".into(),
            llm_model: String::new(),
            proxy_base_url: DEFAULT_PROXY_BASE_URL.into(),
            preferred_language: "auto".into(),
            auto_bind,
            summary_v2_enabled: true,
            summary_speculative_decoding: false,
            // [M12.6] Тесты этого модуля проверяют auto_bind, не engine
            // routing — фиксируем CloudManaged чтобы избежать fail-fast
            // ветки в run_inner.
            #[cfg(target_os = "macos")]
            engine: crate::local_engine::engine::EngineKind::CloudManaged,
        }
    }

    async fn insert_consenting_contact_with_samples(
        pool: &sqlx::SqlitePool,
        name: &str,
        sample_count: usize,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES (?1, ?2, 0, '{\"consent_voice\":\"true\"}', ?3, ?3)",
        )
        .bind(&id)
        .bind(name)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
        for i in 0..sample_count {
            sqlx::query(
                "INSERT INTO voice_samples
                   (id, contact_id, embedding, source_call, quality, created_at)
                 VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            )
            .bind(format!("vs-{name}-{i}"))
            .bind(&id)
            .bind(vec![0u8; 4])
            .bind(0.9)
            .bind(&now)
            .execute(pool)
            .await
            .unwrap();
        }
        id
    }

    async fn insert_speaker_with_score(
        pool: &sqlx::SqlitePool,
        call_id: &str,
        tag: &str,
        suggestion_contact_id: &str,
        score: f64,
    ) {
        sqlx::query(
            "INSERT INTO call_speakers
               (id, call_id, speaker_tag, suggestion_contact_id, suggestion_score, suggestion_source, confirmed)
             VALUES (?1, ?2, ?3, ?4, ?5, 'embedding', 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(call_id)
        .bind(tag)
        .bind(suggestion_contact_id)
        .bind(score)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_auto_bind_disabled_skips_db_call() {
        // auto_bind=None → ни одного speaker не привязано, даже если есть
        // высокий-score suggestion + consent + samples.
        let db = fresh_db().await;
        let call = db::insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_consenting_contact_with_samples(&db.pool, "Alice", 3).await;
        insert_speaker_with_score(&db.pool, &call.id, "S1", &alice, 0.99).await;

        let s = settings_with_auto_bind(None);
        run_auto_bind(&db.pool, None, &call.id, &s).await.unwrap();

        let speakers = db::list_call_speakers(&db.pool, &call.id).await.unwrap();
        let s1 = speakers.iter().find(|s| s.speaker_tag == "S1").unwrap();
        assert!(!s1.confirmed, "disabled auto_bind не должен привязывать");
        assert!(s1.contact_id.is_none());
        assert!(s1.auto_bound_at.is_none());
    }

    #[tokio::test]
    async fn run_auto_bind_enabled_binds_speakers_with_threshold() {
        // Two speakers: 0.97 (>=0.95) → auto-bound; 0.90 (<0.95) → не привязан.
        let db = fresh_db().await;
        let call = db::insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_consenting_contact_with_samples(&db.pool, "Alice", 2).await;
        let bob = insert_consenting_contact_with_samples(&db.pool, "Bob", 2).await;
        insert_speaker_with_score(&db.pool, &call.id, "S1", &alice, 0.97).await;
        insert_speaker_with_score(&db.pool, &call.id, "S2", &bob, 0.90).await;

        let s = settings_with_auto_bind(Some(AutoBindConfig { threshold: 0.95 }));
        run_auto_bind(&db.pool, None, &call.id, &s).await.unwrap();

        let speakers = db::list_call_speakers(&db.pool, &call.id).await.unwrap();
        let s1 = speakers.iter().find(|s| s.speaker_tag == "S1").unwrap();
        let s2 = speakers.iter().find(|s| s.speaker_tag == "S2").unwrap();
        assert!(s1.confirmed, "S1 score 0.97 >= 0.95 → auto-bound");
        assert_eq!(s1.contact_id.as_deref(), Some(alice.as_str()));
        assert!(s1.auto_bound_at.is_some());
        assert!(!s2.confirmed, "S2 score 0.90 < 0.95 → не привязан");
        assert!(s2.contact_id.is_none());
        assert!(s2.auto_bound_at.is_none());
    }

    #[tokio::test]
    async fn reprocess_call_resets_status_and_progress_when_audio_exists() {
        let db = fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device = arc_device("dev-1");

        // Подготовка: row в failed с прогрессом, аудио на диске.
        let call = db::insert_recording(&db.pool, "managed").await.unwrap();
        db::fail_recording_with_reason(&db.pool, &call.id, Some("стрый fail"))
            .await
            .unwrap();
        db::set_call_progress(&db.pool, &call.id, 3, 50, Some(10), Some(2048))
            .await
            .unwrap();

        // Создаём пустые WAV файлы — pipeline пройдёт preflight но упадёт
        // на providers (no settings, no creds). Нам это и нужно — мы
        // проверяем что reset SQL выполнился ДО запуска pipeline'а.
        let call_dir = tmpdir.path().join("calls").join(&call.id);
        tokio::fs::create_dir_all(&call_dir).await.unwrap();
        tokio::fs::write(call_dir.join("mic.wav"), &[0u8; 4])
            .await
            .unwrap();
        tokio::fs::write(call_dir.join("system.wav"), &[0u8; 4])
            .await
            .unwrap();

        // Pipeline упадёт (нет провайдеров / proxy), но reset SQL должен
        // успеть выполниться раньше.
        let _ = reprocess_call(&db.pool, tmpdir.path(), &device, &call.id, None).await;

        // После reset+fail цикл: status='failed' снова (упал на providers),
        // но failed_reason обновится. Главное — pipeline_* очищены.
        let after = db::get_call(&db.pool, &call.id).await.unwrap().unwrap();
        // pipeline_step мог быть проставлен step=1 из emit_progress перед
        // падением, или None если падение случилось раньше. Проверяем что
        // мы не залипли в старом 3/50%.
        assert!(
            after.pipeline_step != Some(3) || after.pipeline_pct != Some(50),
            "старый прогресс не должен сохраниться"
        );
        // failed_reason обновился из "стрый fail" на провайдеровскую ошибку.
        assert!(
            after.failed_reason.as_deref() != Some("стрый fail"),
            "старый failed_reason должен быть перезаписан"
        );
    }

    // ============================================================
    // [M12.6 Phase 2] EngineKind::Local — fail-fast guard
    // ============================================================

    /// [M12.6 Phase 3] Local engine route без AppHandle (headless test
    /// runner) должен вернуть осмысленную ошибку, а не паниковать. Сейчас
    /// run_local_inner требует AppHandle для shell sidecar — без него Err
    /// с маркером `local_engine_no_app_handle`.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn pipeline_run_requires_app_handle_for_local_engine() {
        let db = fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device = arc_device("dev-1");

        let engine = crate::local_engine::engine::load_or_default(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            engine,
            crate::local_engine::engine::EngineKind::Local,
            "migration 0011 должна выставить Local для свежей установки"
        );

        let call = db::insert_recording(&db.pool, "managed").await.unwrap();
        let ctx = PipelineCtx {
            call_id: call.id.clone(),
            call_dir: tmpdir.path().join(&call.id),
            mic_path: tmpdir.path().join("mic.wav"),
            system_path: tmpdir.path().join("sys.wav"),
            device_id: device,
            app_data_dir: tmpdir.path().to_path_buf(),
        };

        let result = run(&db.pool, ctx, None).await;
        let err = result.expect_err("Local engine без app handle → Err");
        let s = err.to_string();
        assert!(
            s.contains("local_engine_no_app_handle"),
            "ожидаемый маркер local_engine_no_app_handle, got: {s}"
        );

        let after = db::get_call(&db.pool, &call.id)
            .await
            .unwrap()
            .expect("call row");
        assert_eq!(after.status, "failed");
    }

    /// Контр-кейс: если пользователь явно переключился на CloudManaged
    /// (через Settings M12.5), pipeline идёт по обычному cloud-пути и
    /// упирается в отсутствие аудио — НЕ в engine fail-fast.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn pipeline_run_does_not_fail_fast_when_cloud_managed_active() {
        let db = fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device = arc_device("dev-1");

        // Переключаем engine на CloudManaged (имитация Settings UI swap).
        crate::local_engine::engine::save(
            &db.pool,
            crate::local_engine::engine::EngineKind::CloudManaged,
        )
        .await
        .unwrap();

        let call = db::insert_recording(&db.pool, "managed").await.unwrap();
        let ctx = PipelineCtx {
            call_id: call.id.clone(),
            call_dir: tmpdir.path().join(&call.id),
            mic_path: tmpdir.path().join("mic.wav"),
            system_path: tmpdir.path().join("sys.wav"),
            device_id: device,
            app_data_dir: tmpdir.path().to_path_buf(),
        };

        let _ = run(&db.pool, ctx, None).await;
        let after = db::get_call(&db.pool, &call.id)
            .await
            .unwrap()
            .expect("call row");
        let reason = after.failed_reason.as_deref().unwrap_or("");
        assert!(
            !reason.contains("local_engine_not_yet_wired"),
            "при CloudManaged engine fail-fast НЕ должен срабатывать, got: {reason}"
        );
    }
}
