//! [M14 T-08 Phase D] Action-item post-pass — refinement отдельным LLM-call'ом.
//!
//! ## Зачем
//!
//! На 2-7B локальных моделях (Qwen 1.5B/3B/7B) main v2 generation часто
//! ошибается в action_items:
//! - Wrong category: "we could try X" помечен как `commitment` вместо `proposal`.
//! - Bogus owner_confidence: 0.9+ при простом упоминании имени, без accept.
//! - Garbage evidence_quote: текст не verbatim из transcript'а.
//! - Дубли из overlap region между chunks (map-reduce path).
//!
//! Post-pass дёргает LLM второй раз ТОЛЬКО на action_items + transcript head,
//! получает refined array, заменяет в финальном CallSummaryV2 JSON.
//!
//! ## Best-effort guarantees
//!
//! - LLM failure / garbage output → возвращаем original action_items (no regression).
//! - Empty input array → short-circuit без LLM call.
//! - Post-pass не меняет другие поля summary (decisions, open_questions, etc).
//!
//! ## Cloud / Local scope
//!
//! Phase D — только local. Cloud (Groq Llama 3.3 70B / Anthropic Sonnet 4)
//! уже выдаёт хорошие action_items, добавление третьего call'а только
//! увеличит латентность. Backport на cloud = Phase D-bis (backlog).

use crate::providers::llm::{LlmProvider, LlmRequest};

const POST_PASS_MAX_TOKENS: u32 = 2048;
/// ~3K tokens ≈ 12K chars transcript fragment для evidence-checking.
const TRANSCRIPT_CONTEXT_CHARS: usize = 12_000;

/// Берёт первые `max_chars` символов transcript'а. UTF-8 safe truncation.
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

pub(crate) fn build_post_pass_prompt(lang_detected: Option<&str>) -> String {
    let lang = lang_detected.unwrap_or("ru");
    format!(
        "You are an action-item quality reviewer. You receive an ACTION_ITEMS array (extracted by earlier LLM pass) and the CALL TRANSCRIPT (head, for evidence-checking). Output language: {lang}.\n\
\n\
## YOUR JOB\n\
\n\
Re-validate and return CORRECTED action_items array. Fix common errors:\n\
\n\
1. `owner_confidence`: 0.9+ ONLY если transcript показывает explicit accept ('I'll do it', 'я возьму', 'I will take that'). 0.5 если inferred (e.g. assigned without explicit accept). 0.0 если no owner.\n\
2. `category`: \"commitment\" = explicit accept; \"proposal\" = suggested но not accepted; \"idea\" = raised без clear action.\n\
3. Dedup: identical `text` ИЛИ одинаковый `evidence.quote` → keep ONE (prefer higher owner_confidence, or larger evidence quote).\n\
4. Evidence verification: если `evidence.quote` НЕ verbatim в transcript (case-insensitive substring) — DROP item entirely (better empty array than fabricated).\n\
5. Preserve all other fields exactly: `id`, `text`, `due`, `due_confidence`, `owner_hint`, `evidence.speaker`.\n\
6. Output ONLY ONE JSON object: {{ \"action_items\": [...] }}. No prose, no markdown fences.\n\
\n\
## INPUT FORMAT\n\
\n\
You receive (in the user input) a JSON object: {{ \"action_items\": [...], \"transcript_head\": \"...\" }}.\n\
\n\
Output the refined `action_items` array. NEVER add new items not present in input — only re-validate existing ones."
    )
}

/// Best-effort refinement. На любую ошибку (LLM error, parse fail, missing
/// `action_items` key в output) возвращает original array.
/// При пустом input array — short-circuit без LLM call.
pub(crate) async fn refine_action_items(
    provider: &dyn LlmProvider,
    action_items: serde_json::Value,
    transcript_md: &str,
    lang_detected: Option<&str>,
) -> serde_json::Value {
    // Short-circuit: empty array → no LLM call.
    let is_empty = action_items
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true);
    if is_empty {
        return action_items;
    }

    let head = extract_transcript_head(transcript_md, TRANSCRIPT_CONTEXT_CHARS);
    let payload = serde_json::json!({
        "action_items": action_items,
        "transcript_head": head,
    });
    let payload_str = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("post-pass: serialize payload failed (keeping original): {e}");
            return action_items;
        }
    };

    let request = LlmRequest {
        model: None,
        system: build_post_pass_prompt(lang_detected),
        input: payload_str,
        max_tokens: Some(POST_PASS_MAX_TOKENS),
    };
    let llm_result = provider.generate(request).await;
    let refined_json = match llm_result {
        Ok(v) => v,
        Err(e) => {
            log::warn!("post-pass: LLM call failed (keeping original): {e}");
            return action_items;
        }
    };

    // Output must be { "action_items": [...] }. Anything else → keep original.
    match refined_json.get("action_items") {
        Some(arr) if arr.is_array() => arr.clone(),
        _ => {
            log::warn!("post-pass: output missing valid 'action_items' array (keeping original)");
            action_items
        }
    }
}

/// Заменить `action_items` поле в финальном summary JSON. Возвращает
/// модифицированный JSON. Если `refined` не array — original остаётся.
pub(crate) fn merge_refined_action_items(
    mut summary_json: serde_json::Value,
    refined: serde_json::Value,
) -> serde_json::Value {
    if !refined.is_array() {
        return summary_json;
    }
    if let Some(obj) = summary_json.as_object_mut() {
        obj.insert("action_items".to_string(), refined);
    }
    summary_json
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    use crate::providers::llm::LlmError;

    struct MockProvider {
        responses: Mutex<Vec<Result<serde_json::Value, LlmError>>>,
        captured: Mutex<Vec<LlmRequest>>,
    }
    impl MockProvider {
        fn new(responses: Vec<Result<serde_json::Value, LlmError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                captured: Mutex::new(Vec::new()),
            }
        }
        fn call_count(&self) -> usize {
            self.captured.lock().unwrap().len()
        }
    }
    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn generate(&self, request: LlmRequest) -> Result<serde_json::Value, LlmError> {
            self.captured.lock().unwrap().push(request);
            let mut guard = self.responses.lock().unwrap();
            if guard.is_empty() {
                return Err(LlmError::Provider("no scripted response".into()));
            }
            guard.remove(0)
        }
    }

    fn sample_action_items() -> serde_json::Value {
        serde_json::json!([
            {
                "id": "a1",
                "text": "Ship pricing draft by Friday",
                "owner_hint": "Alice",
                "owner_confidence": 0.95,
                "due": "2025-01-10",
                "due_confidence": 0.8,
                "category": "commitment",
                "evidence": { "quote": "I'll ship it by Friday", "speaker": "Alice" }
            }
        ])
    }

    #[test]
    fn build_post_pass_prompt_includes_rules() {
        let p = build_post_pass_prompt(Some("ru"));
        assert!(p.contains("owner_confidence"));
        assert!(p.contains("category"));
        assert!(p.contains("Dedup"));
        assert!(p.contains("evidence"));
        assert!(p.contains("verbatim"));
        assert!(p.contains("action_items"));
    }

    #[test]
    fn extract_transcript_head_respects_max_chars() {
        // Кириллица 10 chars, max=5.
        assert_eq!(extract_transcript_head("абвгдежзик", 5), "абвгд");
        // Short → whole.
        assert_eq!(extract_transcript_head("hi", 100), "hi");
    }

    #[test]
    fn merge_refined_action_items_replaces_array_only() {
        let summary = serde_json::json!({
            "schema_version": 2,
            "title": "Original",
            "action_items": [{ "id": "old", "text": "old" }],
            "decisions": [{ "id": "d1", "text": "decision stays" }]
        });
        let refined = serde_json::json!([{ "id": "new", "text": "refined" }]);
        let merged = merge_refined_action_items(summary, refined);
        assert_eq!(merged["title"], "Original");
        assert_eq!(merged["action_items"][0]["id"], "new");
        // Other fields preserved.
        assert_eq!(merged["decisions"][0]["text"], "decision stays");
    }

    #[test]
    fn merge_refined_action_items_keeps_original_on_non_array() {
        let summary = serde_json::json!({
            "title": "X",
            "action_items": [{ "id": "old" }]
        });
        // Refined не array — например LLM выдал object вместо array.
        let refined = serde_json::json!({ "wrong": "shape" });
        let merged = merge_refined_action_items(summary, refined);
        assert_eq!(merged["action_items"][0]["id"], "old");
    }

    #[tokio::test]
    async fn refine_action_items_empty_input_skips_llm() {
        let mock = MockProvider::new(vec![]); // нет responses — LLM call упадёт
        let empty = serde_json::json!([]);
        let result = refine_action_items(&mock, empty.clone(), "transcript", None).await;
        assert_eq!(result, empty);
        assert_eq!(mock.call_count(), 0, "empty input must NOT call LLM");
    }

    #[tokio::test]
    async fn refine_action_items_llm_failure_returns_original() {
        let mock = MockProvider::new(vec![Err(LlmError::Provider("crash".into()))]);
        let original = sample_action_items();
        let result =
            refine_action_items(&mock, original.clone(), "stub transcript", Some("en")).await;
        assert_eq!(result, original);
        assert_eq!(mock.call_count(), 1);
    }

    #[tokio::test]
    async fn refine_action_items_garbage_output_returns_original() {
        let mock = MockProvider::new(vec![Ok(serde_json::json!({
            "wrong_key": "no action_items"
        }))]);
        let original = sample_action_items();
        let result = refine_action_items(&mock, original.clone(), "transcript", None).await;
        assert_eq!(result, original);
    }

    #[tokio::test]
    async fn refine_action_items_llm_success_returns_refined_array() {
        let refined = serde_json::json!([
            {
                "id": "a1",
                "text": "Ship pricing draft by Friday",
                "owner_hint": "Alice",
                "owner_confidence": 0.9,
                "due": "2025-01-10",
                "due_confidence": 0.8,
                "category": "commitment",
                "evidence": { "quote": "I'll ship it by Friday", "speaker": "Alice" }
            }
        ]);
        let mock = MockProvider::new(vec![Ok(serde_json::json!({
            "action_items": refined.clone()
        }))]);
        let original = sample_action_items();
        let result = refine_action_items(&mock, original, "transcript", None).await;
        // Refined array swapped в.
        assert_eq!(result, refined);
    }
}
