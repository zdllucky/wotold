use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};

use crate::{
    db,
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

pub mod merge;
pub mod recap;

pub use merge::{merge_tracks, render_transcript_md};

const SETTING_STT_PROVIDER: &str = "stt_provider";
const SETTING_PROVIDER_PATH: &str = "provider_path";
const SETTING_STT_LANG: &str = "stt_lang";
const SETTING_LLM_MODEL: &str = "llm_model";
const SETTING_PROXY_BASE_URL: &str = "proxy_base_url";

/// Default production proxy URL — managed-режим работает out-of-the-box,
/// user override через Settings → Прокси → Advanced.
const DEFAULT_PROXY_BASE_URL: &str = "https://wotold-proxy.workers.dev";

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

    // M4 chain: транскрипт → LLM рекап. Ошибки рекапа НЕ роняют пайплайн —
    // транскрипт сохранён, рекап можно регенерировать вручную (M4.5).
    let transcript_md = render_transcript_md(&merged);
    let model_override = if llm_model.is_empty() {
        None
    } else {
        Some(llm_model.as_str())
    };
    let recap_ctx = recap::RecapCtx {
        call_id: &ctx.call_id,
        call_dir: &ctx.call_dir,
        transcript_md: &transcript_md,
        lang_detected: lang_detected.as_deref(),
        proxy_base_url: &proxy_base_url,
        device_id: &ctx.device_id,
        provider_path: &provider_path,
        model_override,
    };
    if let Err(e) = recap::run(pool, recap_ctx).await {
        log::warn!("recap {} skipped: {e}", ctx.call_id);
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
