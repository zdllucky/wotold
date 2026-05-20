use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    db,
    providers::{
        transcription::{
            DiarizedTranscript, GladiaProvider, SonioxProvider, TranscriptionOpts,
            TranscriptionProvider,
        },
        ProviderMode,
    },
    AppError,
};

pub mod merge;

pub use merge::{merge_tracks, render_transcript_md};

const SETTING_STT_PROVIDER: &str = "stt_provider";
const SETTING_PROVIDER_PATH: &str = "provider_path";
const SETTING_STT_LANG: &str = "stt_lang";
const SETTING_PROXY_BASE_URL: &str = "proxy_base_url";

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

/// Запуск STT после остановки записи (M2.4-2.5 паспорта). Транскрибирует
/// mic и system параллельно, сливает таймлайн, сохраняет `raw_stt.json` и
/// `transcript.md`, проставляет `calls.status = ready/failed`.
pub async fn run(pool: &SqlitePool, ctx: PipelineCtx) -> Result<(), AppError> {
    let result = run_inner(pool, &ctx).await;
    match &result {
        Ok(()) => {
            db::mark_call_ready(pool, &ctx.call_id).await?;
        }
        Err(e) => {
            log::error!("pipeline {} failed: {e}", ctx.call_id);
            // Не пропагируем ошибку обратно после fail_recording —
            // вызвавший её уже залогировали.
            let _ = db::fail_recording(pool, &ctx.call_id).await;
        }
    }
    result
}

async fn run_inner(pool: &SqlitePool, ctx: &PipelineCtx) -> Result<(), AppError> {
    let provider_id = read_setting(pool, SETTING_STT_PROVIDER, "auto").await?;
    let provider_path = read_setting(pool, SETTING_PROVIDER_PATH, "managed").await?;
    let lang = read_setting(pool, SETTING_STT_LANG, "auto").await?;
    let proxy_base_url = db::get_setting(pool, SETTING_PROXY_BASE_URL)
        .await?
        .unwrap_or_default();

    let mode = build_mode(&provider_path, &proxy_base_url, &ctx.device_id)?;
    let provider = build_provider(&provider_id, mode);
    let opts = TranscriptionOpts {
        lang: lang.clone(),
        diarization: true,
    };

    log::info!(
        "pipeline {} start: provider={provider_id} path={provider_path} lang={lang}",
        ctx.call_id
    );

    let (mic_res, sys_res) = tokio::join!(
        provider.transcribe(&ctx.mic_path, opts.clone()),
        provider.transcribe(&ctx.system_path, opts.clone()),
    );

    let mic_t = mic_res.map_err(|e| AppError::Other(format!("mic stt: {e}")))?;
    let sys_t = sys_res.map_err(|e| AppError::Other(format!("system stt: {e}")))?;

    persist_artifacts(&ctx.call_dir, &mic_t, &sys_t).await?;

    let lang_detected = mic_t
        .lang_detected
        .clone()
        .or_else(|| sys_t.lang_detected.clone());
    let provider_used = sys_t.provider.clone();

    db::set_call_meta(pool, &ctx.call_id, lang_detected.as_deref(), &provider_used).await?;

    Ok(())
}

fn build_mode(
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
            // BYO ключи живут в keychain (#47); пока их не подняли — fail с подсказкой.
            Err(AppError::Other(
                "BYO-ключи ещё не подключены. См. #47 в roadmap.".into(),
            ))
        }
        other => Err(AppError::Other(format!("unknown provider_path: {other}"))),
    }
}

fn build_provider(id: &str, mode: ProviderMode) -> Box<dyn TranscriptionProvider> {
    // M2.2: auto = Soniox primary. Fallback на Gladia при ошибках Soniox —
    // отдельный шаг в #23.
    match id {
        "gladia" => Box::new(GladiaProvider::new(mode)),
        _ => Box::new(SonioxProvider::new(mode)),
    }
}

async fn persist_artifacts(
    call_dir: &PathBuf,
    mic: &DiarizedTranscript,
    system: &DiarizedTranscript,
) -> Result<(), AppError> {
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

    Ok(())
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
