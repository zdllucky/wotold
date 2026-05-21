use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};

use crate::{
    db,
    embeddings::{self, StubEmbedder},
    matching,
    pipeline::{clusters::extract_clusters, merge::OWNER_TAG},
    providers::{
        transcription::{
            failure_reason, transcribe_with_fallback, DiarizedTranscript, GladiaProvider,
            RetryConfig, SonioxProvider, TranscriptionError, TranscriptionOpts,
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

pub use merge::{merge_tracks, render_transcript_md};

const SETTING_STT_PROVIDER: &str = "stt_provider";
const SETTING_PROVIDER_PATH: &str = "provider_path";
const SETTING_STT_LANG: &str = "stt_lang";
const SETTING_LLM_MODEL: &str = "llm_model";
const SETTING_PROXY_BASE_URL: &str = "proxy_base_url";
/// [B13] Системный язык для LLM-output (рекап + action items). 'auto' = язык
/// детектированный STT, иначе BCP47 (ru, en, kk, ...). НЕ влияет на STT
/// auto-detect — STT остаётся multi-lang biased (см. proxy/lib/partners).
const SETTING_PREFERRED_LANGUAGE: &str = "preferred_language";

/// Default proxy URL — debug-сборки (cargo run / tauri dev) целятся на staging,
/// release — на production. User override через Settings → Прокси → Advanced.
#[cfg(debug_assertions)]
const DEFAULT_PROXY_BASE_URL: &str = "https://wotold-proxy-staging.animereader.workers.dev";
#[cfg(not(debug_assertions))]
const DEFAULT_PROXY_BASE_URL: &str = "https://wotold-proxy.animereader.workers.dev";

/// Контекст одной транскрипции: пути к двум дорожкам, call_dir для артефактов,
/// device-id для managed-режима. Настройки (provider/path/lang/proxy URL) и
/// BYO-ключи читаются из БД внутри `run`.
pub struct PipelineCtx {
    pub call_id: String,
    pub call_dir: PathBuf,
    pub mic_path: PathBuf,
    pub system_path: PathBuf,
    pub device_id: Arc<str>,
}

/// Событие [B5]: фронтенд слушает `pipeline:finished` чтобы обновить Calls list
/// без manual refresh.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PipelineFinishedEvent {
    pub call_id: String,
    /// `ready` | `failed`
    pub status: &'static str,
    pub failed_reason: Option<String>,
}

/// Запуск STT после остановки записи (M2.4-2.5 паспорта). Транскрибирует
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
    // [B16] Emit `pipeline:started` для global progress indicator в topnav.
    if let Some(handle) = app {
        #[derive(Clone, serde::Serialize)]
        struct Started {
            call_id: String,
        }
        if let Err(e) = handle.emit(
            "pipeline:started",
            Started { call_id: ctx.call_id.clone() },
        ) {
            log::warn!("emit pipeline:started failed: {e}");
        }
    }
    let result = run_inner(pool, &ctx).await;
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
    if let Some(handle) = app {
        if let Err(e) = handle.emit("pipeline:finished", &event) {
            log::warn!("emit pipeline:finished failed: {e}");
        }
    }

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
        .ok_or_else(|| AppError::Other(format!("call {call_id} not found")))?;

    let call_dir = app_data_dir.join("calls").join(call_id);
    let mic_path = call_dir.join("mic.wav");
    let system_path = call_dir.join("system.wav");

    if !mic_path.exists() && !system_path.exists() {
        return Err(AppError::Other(
            "Аудио файлы (mic.wav / system.wav) не найдены на диске — переобработка невозможна.".into(),
        ));
    }

    // Reset status: was failed → processing, clear failed_reason.
    // Если был ready — тоже перетянем в processing, чтобы UI показывал прогресс
    // и не закешировал старый recap.
    sqlx::query(
        "UPDATE calls
         SET status = 'processing',
             failed_reason = NULL,
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
    };

    run(pool, ctx, app).await
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
) -> Result<(), AppError> {
    let call = db::get_call(pool, call_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("call {call_id} not found")))?;

    let call_dir = app_data_dir.join("calls").join(call_id);
    let transcript_path = call_dir.join("transcript.md");
    let transcript_md = tokio::fs::read_to_string(&transcript_path)
        .await
        .map_err(|e| AppError::Other(format!("transcript.md отсутствует: {e}")))?;

    let provider_path = read_setting(pool, SETTING_PROVIDER_PATH, "managed").await?;
    let llm_model = read_setting(pool, SETTING_LLM_MODEL, "").await?;
    let preferred_language = read_setting(pool, SETTING_PREFERRED_LANGUAGE, "auto").await?;
    let proxy_base_url = db::get_setting(pool, SETTING_PROXY_BASE_URL)
        .await?
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PROXY_BASE_URL.to_string());

    let model_override = if llm_model.is_empty() {
        None
    } else {
        Some(llm_model.as_str())
    };
    // [B13] preferred_language='auto' → используем lang_detected от STT,
    // иначе override (например 'ru' даже для en-транскрипта).
    let effective_lang: Option<String> = if preferred_language == "auto" || preferred_language.is_empty() {
        call.lang_detected.clone()
    } else {
        Some(preferred_language.clone())
    };

    let recap_ctx = recap::RecapCtx {
        call_id,
        call_dir: &call_dir,
        transcript_md: &transcript_md,
        lang_detected: effective_lang.as_deref(),
        proxy_base_url: &proxy_base_url,
        device_id,
        provider_path: &provider_path,
        model_override,
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
            let _ = db::set_recap_failed_reason(pool, call_id, Some(&e.to_string())).await;
            Err(e)
        }
    }
}

async fn run_inner(pool: &SqlitePool, ctx: &PipelineCtx) -> Result<(), AppError> {
    let provider_id = read_setting(pool, SETTING_STT_PROVIDER, "auto").await?;
    let provider_path = read_setting(pool, SETTING_PROVIDER_PATH, "managed").await?;
    let lang = read_setting(pool, SETTING_STT_LANG, "auto").await?;
    let llm_model = read_setting(pool, SETTING_LLM_MODEL, "").await?;
    let proxy_base_url = db::get_setting(pool, SETTING_PROXY_BASE_URL)
        .await?
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PROXY_BASE_URL.to_string());

    let providers = build_providers(
        &provider_id,
        &provider_path,
        &proxy_base_url,
        &ctx.device_id,
    )?;
    let opts = TranscriptionOpts {
        lang: lang.clone(),
        diarization: true,
    };
    let retry_cfg = RetryConfig::default();

    log::info!(
        "pipeline {} start: provider={provider_id} path={provider_path} lang={lang} fallbacks={}",
        ctx.call_id,
        providers.len()
    );

    let (mic_res, sys_res) = tokio::join!(
        transcribe_with_fallback(&providers, &ctx.mic_path, opts.clone(), retry_cfg),
        transcribe_with_fallback(&providers, &ctx.system_path, opts.clone(), retry_cfg),
    );

    let mic_t = mic_res.map_err(stt_to_app_error)?;
    let sys_t = sys_res.map_err(stt_to_app_error)?;

    let merged = persist_artifacts(&ctx.call_dir, &mic_t, &sys_t).await?;

    let lang_detected = mic_t
        .lang_detected
        .clone()
        .or_else(|| sys_t.lang_detected.clone());
    let provider_used = sys_t.provider.clone();

    db::set_call_meta(pool, &ctx.call_id, lang_detected.as_deref(), &provider_used).await?;

    // M3.7: mic-дорожка по определению принадлежит владельцу устройства.
    // Создаём confirmed=1 row сразу — пользователю не нужно подтверждать
    // самого себя. Не нарушает R2 (никакой автопривязки): owner == юзер.
    let owner = db::ensure_owner_contact(pool).await?;
    if let Err(e) = db::auto_bind_owner_speaker(pool, &ctx.call_id, &owner.id, OWNER_TAG).await {
        log::warn!("auto_bind_owner_speaker {} failed: {e}", ctx.call_id);
    }

    // [B11]: добавить placeholder rows для всех distinct speaker_tag из транскрипта
    // (кроме owner, у которого уже confirmed). UI покажет даже анонимных «S1/S2»,
    // юзер сможет привязать через select или «+ Добавить как контакт».
    let mut seen_tags: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for seg in &merged {
        if seg.speaker_tag != OWNER_TAG && !seg.speaker_tag.is_empty() {
            seen_tags.insert(seg.speaker_tag.clone());
        }
    }
    let tags_vec: Vec<String> = seen_tags.into_iter().collect();
    if !tags_vec.is_empty() {
        if let Err(e) = db::ensure_call_speakers_present(pool, &ctx.call_id, &tags_vec).await {
            log::warn!("ensure_call_speakers_present {} failed: {e}", ctx.call_id);
        }
    }

    // [B3.3-3.4] Voice cluster extraction + matching → suggestion. Embedder
    // сейчас Stub (no-op до B3.6), pipeline ничего не извлекает. После B3.6
    // → реальный OnnxEmbedder и flow заработает на existing звонках через
    // `regenerate_recap` (M4.5) + новых через эту ветку.
    if let Err(e) = run_cluster_pipeline(
        pool,
        &ctx.call_id,
        &merged,
        &ctx.mic_path,
        &ctx.system_path,
    )
    .await
    {
        log::warn!(
            "cluster pipeline {} failed (non-fatal — skip voice match): {e}",
            ctx.call_id
        );
    }

    // M4 chain: транскрипт → LLM рекап. Ошибки рекапа НЕ роняют пайплайн —
    // транскрипт сохранён, рекап можно регенерировать вручную (M4.5).
    let transcript_md = render_transcript_md(&merged);
    let model_override = if llm_model.is_empty() {
        None
    } else {
        Some(llm_model.as_str())
    };
    // [B13] preferred_language override для LLM (см. regenerate_recap).
    let preferred_language = read_setting(pool, SETTING_PREFERRED_LANGUAGE, "auto").await?;
    let effective_lang: Option<String> = if preferred_language == "auto" || preferred_language.is_empty() {
        lang_detected.clone()
    } else {
        Some(preferred_language.clone())
    };
    let recap_ctx = recap::RecapCtx {
        call_id: &ctx.call_id,
        call_dir: &ctx.call_dir,
        transcript_md: &transcript_md,
        lang_detected: effective_lang.as_deref(),
        proxy_base_url: &proxy_base_url,
        device_id: &ctx.device_id,
        provider_path: &provider_path,
        model_override,
    };
    if let Err(e) = recap::run(pool, recap_ctx).await {
        let reason = e.to_string();
        log::warn!("recap {} skipped: {reason}", ctx.call_id);
        // [B16]: persist recap failure для UI banner. status='ready' остаётся —
        // транскрипт есть, юзер видит «не получилось саммари: ...» + кнопка retry.
        if let Err(e2) =
            db::set_recap_failed_reason(pool, &ctx.call_id, Some(&reason)).await
        {
            log::error!("set_recap_failed_reason {} failed: {e2}", ctx.call_id);
        }
    } else {
        // Очищаем если был старый recap_failed_reason (например после reprocess).
        let _ = db::set_recap_failed_reason(pool, &ctx.call_id, None).await;
    }

    Ok(())
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
async fn run_cluster_pipeline(
    pool: &SqlitePool,
    call_id: &str,
    merged: &[crate::providers::transcription::TranscriptSegment],
    mic_path: &Path,
    system_path: &Path,
) -> Result<(), AppError> {
    let embedder = StubEmbedder; // B3.6 swaps на OnnxEmbedder.
    let clusters = extract_clusters(merged, mic_path, system_path, &embedder)?;
    if clusters.is_empty() {
        // Stub embedder возвращает empty → нет clusters. Это OK pre-B3.6.
        log::debug!("cluster pipeline {call_id}: no clusters (stub embedder)");
        return Ok(());
    }

    // [B3.4] Загружаем существующие voice_samples всех consenting контактов
    // ОДИН раз перед циклом — matching::list_consenting_samples делает join.
    let consenting = matching::list_consenting_samples(pool).await?;

    for (tag, vector) in &clusters {
        let blob = embeddings::embedding_to_bytes(vector);
        if blob.is_empty() {
            continue;
        }
        if let Err(e) = db::set_call_speaker_cluster(pool, call_id, tag, &blob).await {
            log::warn!("set_call_speaker_cluster {tag}: {e}");
            continue;
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

async fn persist_artifacts(
    call_dir: &PathBuf,
    mic: &DiarizedTranscript,
    system: &DiarizedTranscript,
) -> Result<Vec<crate::providers::transcription::TranscriptSegment>, AppError> {
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

async fn read_setting(
    pool: &SqlitePool,
    key: &str,
    default_value: &str,
) -> Result<String, AppError> {
    Ok(db::get_setting(pool, key)
        .await?
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_value.to_string()))
}
