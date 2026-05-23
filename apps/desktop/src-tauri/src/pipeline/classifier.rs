//! [M14 T-04] Lightweight LLM-pass для определения `call_type` ДО основного
//! v2 generation. Отдельный модуль, потому что:
//! - Маленький max_tokens (~256 vs 4096 main) экономит латентность.
//! - Простой output schema — { call_type, confidence, language } — меньше
//!   шансов на garbage JSON по сравнению с full CallSummaryV2.
//! - Orchestrator (T-10) использует hint в main v2 system prompt, чтобы LLM
//!   не классифицировал заново и сразу заполнял правильный type_specific_block.
//!
//! **Best-effort:** на любую ошибку (parse failure, timeout, sidecar crash)
//! orchestrator делает fallback на single-pass без hint — текущее T-02 поведение.
//!
//! **Phase A constraints:**
//! - Берёт первые `MAX_CLASSIFIER_HEAD_CHARS` символов transcript (~1500 tokens
//!   при 4 chars/token). Достаточно чтобы LLM понял тип звонка.
//! - Phase B (T-05): когда добавим chunking, classifier будет дёрнут на
//!   первом chunk (тот же head).

use serde::Deserialize;

use crate::pipeline::summary_v2::CallType;
use crate::providers::llm::{LlmProvider, LlmRequest};
use crate::AppError;

/// Конверт результат лёгкого classifier-pass'а.
#[derive(Debug, Clone)]
pub(crate) struct ClassifierResult {
    pub call_type: CallType,
    pub confidence: f32,
    #[allow(dead_code)] // Phase B: language может использоваться для override preferred_language
    pub language: String,
}

/// Сколько символов transcript отдаём классификатору. ~1500 tokens при
/// 4 chars/token среднем. Phase A: лимитируем head'ом — звонок начинается
/// с greeting + intro, тип обычно понятен из первой минуты.
pub(crate) const MAX_CLASSIFIER_HEAD_CHARS: usize = 6000;

/// Берёт первые `max_chars` символов transcript. Не разрезает посреди
/// UTF-8 grapheme — использует `char_indices()` для safe truncation.
pub(crate) fn extract_classifier_head(transcript_md: &str, max_chars: usize) -> &str {
    if transcript_md.chars().count() <= max_chars {
        return transcript_md;
    }
    // char_indices даёт byte-positions для каждого char. После max_chars char'ов
    // берём следующий byte_idx как cutoff. unwrap_or — fallback на whole string
    // если по какой-то причине не нашли (не должно случиться при count > max).
    let cutoff_byte = transcript_md
        .char_indices()
        .nth(max_chars)
        .map(|(b, _)| b)
        .unwrap_or(transcript_md.len());
    &transcript_md[..cutoff_byte]
}

/// Сырой JSON-shape, который мы хотим от классификатора. Поля типобезопасные —
/// `call_type` парсится через `CallType::from_str`, при mismatch возвращаем
/// `CallType::Other` (defensive parsing).
#[derive(Debug, Deserialize)]
struct ClassifierJson {
    call_type: String,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    language: Option<String>,
}

/// Построить prompt для classifier-pass'а. Output language подбирается из
/// `lang_detected` (или 'ru' по умолчанию) — LLM должен ответить JSON-only.
pub(crate) fn build_classifier_prompt(lang_detected: Option<&str>) -> String {
    let lang = lang_detected.unwrap_or("ru");
    format!(
        "You are a meeting classifier. Read the FIRST PART of a corporate call transcript and classify the call type. Output language for the JSON value strings: {lang}.\n\
\n\
## RULES\n\
1. Output ONLY ONE JSON object matching schema below. No prose, no markdown fences.\n\
2. If unsure between two types, pick the more likely one and lower `confidence`.\n\
3. If clearly none of the typed categories fits → `\"other\"`.\n\
\n\
## CALL TYPES (pick exactly one)\n\
- `sales_discovery` — vendor rep + prospect, exploring pain/budget/timeline.\n\
- `sales_demo` — vendor rep walks through product capabilities, prospect asks questions.\n\
- `product_sync` — internal team sync about product progress, roadmap, blockers.\n\
- `standup` — short daily team status (yesterday/today/blockers per person).\n\
- `customer_interview` — research call с existing/potential user, qualitative feedback.\n\
- `one_on_one` — 1:1 manager↔report check-in (personal feedback, growth, career).\n\
- `strategy_brainstorm` — open-ended ideation, decisions, options exploration.\n\
- `status_update` — formal progress report (project status, milestones, RAG).\n\
- `other` — none of the above.\n\
\n\
## SCHEMA\n\
\n\
{{\n\
  \"call_type\": \"sales_discovery\"|\"sales_demo\"|\"product_sync\"|\"standup\"|\"customer_interview\"|\"one_on_one\"|\"strategy_brainstorm\"|\"status_update\"|\"other\",\n\
  \"confidence\": 0..1,                          // Уверенность 0.5+ если signals clear, иначе lower\n\
  \"language\": \"ru\" | \"en\" | \"kk\" | \"mixed\"\n\
}}\n\
\n\
Output ONLY the JSON object. No prose. No markdown fences."
    )
}

/// Дёрнуть классификатор. На любую ошибку (LLM error, JSON parse) возвращает
/// `AppError::Other` — orchestrator делает fallback на single-pass без hint.
pub(crate) async fn classify_call(
    provider: &dyn LlmProvider,
    transcript_head: &str,
    lang_detected: Option<&str>,
) -> Result<ClassifierResult, AppError> {
    let request = LlmRequest {
        model: None,
        system: build_classifier_prompt(lang_detected),
        input: transcript_head.to_string(),
        max_tokens: Some(256), // Compact output — экономим латентность
    };
    let json_value = provider
        .generate(request)
        .await
        .map_err(|e| AppError::Other(format!("classifier llm: {e}")))?;
    parse_classifier_response(json_value)
}

/// Распарсить JSON-ответ классификатора. На неизвестный call_type → `Other`.
/// На отсутствие confidence → 0.5 (neutral). На отсутствие language → 'ru' fallback.
pub(crate) fn parse_classifier_response(
    json_value: serde_json::Value,
) -> Result<ClassifierResult, AppError> {
    let parsed: ClassifierJson = serde_json::from_value(json_value)
        .map_err(|e| AppError::Other(format!("classifier JSON shape: {e}")))?;
    let call_type = CallType::from_str(&parsed.call_type).unwrap_or(CallType::Other);
    let confidence = parsed.confidence.unwrap_or(0.5).clamp(0.0, 1.0);
    let language = parsed.language.unwrap_or_else(|| "ru".to_string());
    Ok(ClassifierResult {
        call_type,
        confidence,
        language,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_classifier_prompt_includes_all_call_types() {
        let prompt = build_classifier_prompt(Some("ru"));
        for variant in [
            "sales_discovery",
            "sales_demo",
            "product_sync",
            "standup",
            "customer_interview",
            "one_on_one",
            "strategy_brainstorm",
            "status_update",
            "other",
        ] {
            assert!(
                prompt.contains(variant),
                "prompt missing call_type variant: {variant}"
            );
        }
        assert!(prompt.contains("JSON object"));
    }

    #[test]
    fn extract_classifier_head_respects_max_chars() {
        // 12 char ASCII string, max=5 → first 5 chars.
        let head = extract_classifier_head("abcdefghijkl", 5);
        assert_eq!(head, "abcde");
    }

    #[test]
    fn extract_classifier_head_short_transcript_returns_whole() {
        let s = "short";
        let head = extract_classifier_head(s, 100);
        assert_eq!(head, s);
    }

    #[test]
    fn extract_classifier_head_handles_unicode_boundary() {
        // Кириллица: каждый char = 2 bytes. 10 chars = 20 bytes.
        // max=5 chars → cutoff at byte_idx of 5th char's start.
        let s = "абвгдежзик";
        let head = extract_classifier_head(s, 5);
        assert_eq!(head, "абвгд");
    }

    #[test]
    fn parse_classifier_response_handles_unknown_call_type() {
        let v = serde_json::json!({
            "call_type": "made_up_garbage",
            "confidence": 0.9,
            "language": "en",
        });
        let r = parse_classifier_response(v).unwrap();
        assert_eq!(r.call_type, CallType::Other);
        assert!((r.confidence - 0.9).abs() < 1e-6);
        assert_eq!(r.language, "en");
    }

    #[test]
    fn parse_classifier_response_clamps_confidence() {
        let v = serde_json::json!({
            "call_type": "standup",
            "confidence": 2.5,
        });
        let r = parse_classifier_response(v).unwrap();
        assert_eq!(r.call_type, CallType::Standup);
        assert!((r.confidence - 1.0).abs() < 1e-6);
        assert_eq!(r.language, "ru"); // default fallback
    }

    #[test]
    fn parse_classifier_response_missing_optional_fields_uses_defaults() {
        let v = serde_json::json!({ "call_type": "sales_demo" });
        let r = parse_classifier_response(v).unwrap();
        assert_eq!(r.call_type, CallType::SalesDemo);
        assert!((r.confidence - 0.5).abs() < 1e-6);
        assert_eq!(r.language, "ru");
    }

    #[test]
    fn parse_classifier_response_garbage_json_returns_error() {
        let v = serde_json::json!({ "wrong_key": 42 });
        let err = parse_classifier_response(v).unwrap_err();
        assert!(err.to_string().contains("classifier JSON"));
    }
}
