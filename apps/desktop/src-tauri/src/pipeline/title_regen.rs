//! [M14 T-17] Lightweight title-only LLM regeneration.
//!
//! ## Зачем
//!
//! Full `regenerate_recap` — heavy: ~4096 max_tokens LLM call + post-pass +
//! persist всех v2 структур (decisions, open_questions, action_items,
//! recap.md re-render). Если пользователю не нравится ТОЛЬКО заголовок
//! (например генеривался при пустом transcript head → дефолтный fallback),
//! полная регенерация — overkill (latency + quota).
//!
//! T-17 даёт separate path:
//! - ~150 max_tokens (заголовок 3-7 слов + JSON envelope)
//! - Focused prompt только на title
//! - Engine-aware: Local-движок → локальный Qwen sidecar (TITLE_JSON_SCHEMA),
//!   иначе → `AnthropicProvider::Managed` (cloud proxy)
//! - На fallback: `db::set_call_title` без trigger downstream events
//!
//! ## Engine dispatch
//!
//! Mirror `regenerate_recap`: при `EngineKind::Local` название генерится локально
//! (~5-10s, no cloud/quota — консистентно с саммари); cloud-движок — мгновенно.

use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::db;
use crate::pipeline::settings::PipelineSettings;
use crate::providers::llm::{AnthropicProvider, LlmProvider, LlmRequest};
use crate::providers::ProviderMode;
use crate::AppError;

const TITLE_MAX_TOKENS: u32 = 150;
/// Берём первые ~6000 chars transcript для context (matches classifier head).
pub(crate) const TRANSCRIPT_HEAD_CHARS: usize = 6000;
/// Fallback title когда LLM выдаёт пусто / garbage. ru-only ok — UI
/// эмулирует через `simpleDateTitle` если empty.
const DEFAULT_TITLE_FALLBACK: &str = "Без названия";

#[derive(Debug, Deserialize)]
struct TitleJson {
    #[serde(default)]
    title: Option<String>,
}

/// Build focused title-only prompt. Output language подбирается из
/// `lang_detected` (или 'ru' по умолчанию).
pub(crate) fn build_title_prompt(lang_detected: Option<&str>) -> String {
    let lang = lang_detected.unwrap_or("ru");
    format!(
        "You are a meeting title generator. Read the FIRST PART of a corporate call transcript and produce a single concise headline-style title (3-7 words). Output language: {lang}.\n\
\n\
## RULES\n\
\n\
1. Concrete и specific. Никаких 'Звонок про X' / 'Discussion about Y'.\n\
2. Capture the central topic — product launch, hiring decision, customer pain, sprint review etc.\n\
3. Output ONLY ONE JSON object: {{ \"title\": string }}. No prose, no markdown fences.\n\
4. Title MUST be в {lang} language (mixed transcripts → dominant language).\n\
5. Если transcript пустой или мусорный — title = '{fallback}'.\n\
\n\
Output ONLY the JSON object.",
        fallback = DEFAULT_TITLE_FALLBACK
    )
}

/// UTF-8 safe truncation transcript head.
pub(crate) fn extract_transcript_head(transcript_md: &str, max_chars: usize) -> &str {
    if transcript_md.chars().count() <= max_chars {
        return transcript_md;
    }
    let cutoff_byte = transcript_md
        .char_indices()
        .nth(max_chars)
        .map(|(b, _)| b)
        .unwrap_or(transcript_md.len());
    &transcript_md[..cutoff_byte]
}

/// Parse `{ "title": string }`. На garbage / empty → fallback.
pub(crate) fn parse_title_response(json_value: serde_json::Value) -> String {
    let parsed: Result<TitleJson, _> = serde_json::from_value(json_value);
    match parsed {
        Ok(t) => match t.title {
            Some(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    DEFAULT_TITLE_FALLBACK.to_string()
                } else {
                    trimmed.to_string()
                }
            }
            None => DEFAULT_TITLE_FALLBACK.to_string(),
        },
        Err(_) => DEFAULT_TITLE_FALLBACK.to_string(),
    }
}

/// Lightweight title regen. Engine-aware: при движке Local — локальный Qwen
/// (sidecar, ~5-10s, no cloud/quota, mirror `regenerate_recap`); иначе — cloud
/// Anthropic proxy через `ProviderMode::Managed` (мгновенно). На любой LLM-error
/// → propagate (caller покажет setError).
///
/// `app` нужен только для local-движка (LocalLlamaProvider требует AppHandle
/// для sidecar). Cloud-путь его игнорит.
///
/// Returns: новый title (уже persisted в DB через `db::set_call_title`).
pub async fn regenerate_title(
    pool: &SqlitePool,
    app_data_dir: &Path,
    device_id: &Arc<str>,
    call_id: &str,
    app: Option<&AppHandle>,
) -> Result<String, AppError> {
    let call = db::get_call(pool, call_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("call {call_id}")))?;

    let call_dir = app_data_dir.join("calls").join(call_id);
    let transcript_path = call_dir.join("transcript.md");
    let transcript_md = tokio::fs::read_to_string(&transcript_path)
        .await
        .map_err(|e| AppError::Other(format!("transcript.md отсутствует: {e}")))?;

    let s = PipelineSettings::load(pool).await?;
    let effective_lang = s.effective_recap_lang(call.lang_detected.as_deref());
    let head = extract_transcript_head(&transcript_md, TRANSCRIPT_HEAD_CHARS);

    // [Local title] При активном Local-движке генерим название локальным Qwen —
    // консистентно с саммари, без облака/квоты. Mirror regenerate_recap dispatch.
    // TITLE_JSON_SCHEMA форсит `{ "title": string }` у слабой модели.
    #[cfg(target_os = "macos")]
    if s.engine == crate::local_engine::engine::EngineKind::Local {
        let app = app.ok_or_else(|| {
            AppError::Other("regenerate_title для local-engine требует AppHandle".into())
        })?;
        let (provider, _preset) =
            crate::pipeline::build_local_llm_provider(pool, app_data_dir, app, &s).await?;
        let request = LlmRequest {
            model: None,
            system: build_title_prompt(effective_lang.as_deref()),
            input: head.to_string(),
            max_tokens: Some(TITLE_MAX_TOKENS),
            grammar: None,
            json_schema: Some(crate::pipeline::llm_schemas::TITLE_JSON_SCHEMA.to_string()),
        };
        let json_value = provider
            .generate(request)
            .await
            .map_err(|e| AppError::Other(format!("local title llm: {e}")))?;
        let new_title = parse_title_response(json_value);
        db::set_call_title(pool, call_id, &new_title).await?;
        return Ok(new_title);
    }

    // Cloud path (Anthropic proxy через ProviderMode::Managed).
    let mode = match s.provider_path.as_str() {
        "managed" => {
            if s.proxy_base_url.is_empty() {
                return Err(AppError::Other(
                    "Proxy URL не настроен. Settings → Proxy URL.".into(),
                ));
            }
            ProviderMode::Managed {
                proxy_base_url: s.proxy_base_url.clone(),
                device_id: device_id.to_string(),
            }
        }
        "byo" => {
            return Err(AppError::Other(
                "BYO LLM key ещё не подключён для title regen.".into(),
            ));
        }
        other => return Err(AppError::Other(format!("unknown provider_path: {other}"))),
    };

    let request = LlmRequest {
        model: s.model_override().map(str::to_string),
        system: build_title_prompt(effective_lang.as_deref()),
        input: head.to_string(),
        max_tokens: Some(TITLE_MAX_TOKENS),
        grammar: None,
        json_schema: None,
    };

    let provider = AnthropicProvider::new(mode);
    let json_value = provider
        .generate(request)
        .await
        .map_err(|e| AppError::Other(format!("title llm: {e}")))?;

    let new_title = parse_title_response(json_value);
    db::set_call_title(pool, call_id, &new_title).await?;
    Ok(new_title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_title_prompt_includes_lang_and_rules() {
        let p = build_title_prompt(Some("en"));
        assert!(p.contains("Output language: en"));
        assert!(p.contains("RULES"));
        assert!(p.contains("\"title\""));
        assert!(p.contains("3-7 words"));
        assert!(p.contains("Без названия")); // Fallback mentioned

        let p_default = build_title_prompt(None);
        assert!(p_default.contains("Output language: ru"));
    }

    #[test]
    fn extract_transcript_head_respects_max_chars() {
        assert_eq!(extract_transcript_head("abcdefghij", 5), "abcde");
        assert_eq!(extract_transcript_head("short", 100), "short");
        // Кириллица: 5 chars cutoff.
        assert_eq!(extract_transcript_head("абвгдежзик", 5), "абвгд");
    }

    #[test]
    fn parse_title_response_valid_returns_trimmed_title() {
        let v = serde_json::json!({ "title": "  Sprint planning Q1  " });
        assert_eq!(parse_title_response(v), "Sprint planning Q1");
    }

    #[test]
    fn parse_title_response_empty_title_falls_back() {
        let v = serde_json::json!({ "title": "   " });
        assert_eq!(parse_title_response(v), DEFAULT_TITLE_FALLBACK);
    }

    #[test]
    fn parse_title_response_missing_title_falls_back() {
        let v = serde_json::json!({ "wrong_key": "irrelevant" });
        assert_eq!(parse_title_response(v), DEFAULT_TITLE_FALLBACK);
    }

    #[test]
    fn parse_title_response_garbage_json_falls_back() {
        // Array вместо object.
        let v = serde_json::json!(["not", "an", "object"]);
        assert_eq!(parse_title_response(v), DEFAULT_TITLE_FALLBACK);
    }

    #[tokio::test]
    async fn regenerate_title_missing_transcript_returns_error() {
        let db = crate::db::test_support::fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device: Arc<str> = Arc::from("dev-1");
        let call = db::insert_recording(&db.pool, "managed").await.unwrap();
        let err = regenerate_title(&db.pool, tmpdir.path(), &device, &call.id, None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("transcript.md"),
            "expected transcript.md error, got: {err}"
        );
    }

    /// [Local title] При движке Local + app=None → ошибка про AppHandle
    /// (до sidecar, который не покрыть юнит-тестом). transcript.md на месте.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn regenerate_title_local_engine_requires_app_handle() {
        let db = crate::db::test_support::fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device: Arc<str> = Arc::from("dev-1");
        db::set_setting(&db.pool, "local_engine.active", "local")
            .await
            .unwrap();
        let call = db::insert_recording(&db.pool, "managed").await.unwrap();
        let call_dir = tmpdir.path().join("calls").join(&call.id);
        tokio::fs::create_dir_all(&call_dir).await.unwrap();
        tokio::fs::write(call_dir.join("transcript.md"), "S1: привет")
            .await
            .unwrap();
        let err = regenerate_title(&db.pool, tmpdir.path(), &device, &call.id, None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("AppHandle"),
            "expected AppHandle error, got: {err}"
        );
    }
}
