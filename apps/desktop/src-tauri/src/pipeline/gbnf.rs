//! [M14 T-09 Phase E] GBNF grammar fallback для local LLM JSON parsing.
//!
//! ## Зачем
//!
//! Маленькие модели (Qwen 1.5B особенно) изредка возвращают garbage JSON:
//! лишний text до/после `{...}`, обрезанные объекты, mixed prose.
//! `extract_json_object()` в `local_engine/llm.rs:216` пытается выудить
//! первый balanced `{...}` — но при tail prose или вложенной confusion'е
//! fail'ится → `LlmError::Provider("malformed JSON: ...")`. Это убивает
//! recap.
//!
//! ## Flow
//!
//! `generate_with_grammar_fallback` дёргает provider:
//! 1. **First attempt** — без grammar (естественный output, быстрее).
//! 2. **On Provider error** → retry с
//!    `grammar = Some(UNIVERSAL_JSON_OBJECT_GRAMMAR)`. Llama.cpp
//!    `--grammar-file` констрейнит output до valid JSON object.
//! 3. **Second failure** → propagate (no infinite loop).
//!
//! ## Где используется
//!
//! Во всех **local-side** LLM call sites:
//! - `classifier::classify_call`
//! - `local_orchestrator::run_v2_pipeline` (single-pass main)
//! - `map_reduce::run_map_reduce` (per-chunk map + final reduce)
//! - `action_item_post_pass::refine_action_items`
//!
//! Cloud (`recap::run` + `AnthropicProvider`) не использует — proxy
//! гарантирует JSON через native API contracts.
//!
//! ## Phase E scope (PRD §5.7)
//!
//! Один **universal JSON-object grammar** (standard llama.cpp json.gbnf).
//! Не констрейним shape поля per call_type — только outer object validity.
//! Per-type grammars — backlog.

use serde_json::Value;

use crate::providers::llm::{LlmError, LlmProvider, LlmRequest};

/// Standard llama.cpp JSON-object grammar. Констрейнит output до single
/// parseable JSON object — outer shape, не per-type schema.
pub(crate) const UNIVERSAL_JSON_OBJECT_GRAMMAR: &str = r#"
root   ::= object
value  ::= object | array | string | number | ("true" | "false" | "null") ws
object ::= "{" ws (string ":" ws value ("," ws string ":" ws value)*)? "}" ws
array  ::= "[" ws (value ("," ws value)*)? "]" ws
string ::= "\"" ([^"\\] | "\\" (["\\bfnrt] | "u" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F]))* "\"" ws
number ::= ("-"? ([0-9] | [1-9] [0-9]*)) ("." [0-9]+)? ([eE] [-+]? [0-9]+)? ws
ws     ::= [ \t\n\r]*
"#;

/// Retry wrapper: первая попытка БЕЗ grammar; на `LlmError::Provider`
/// (включая JSON parse failures из `LocalLlamaProvider::generate`) —
/// retry с grammar set. На second failure → propagate.
pub(crate) async fn generate_with_grammar_fallback(
    provider: &dyn LlmProvider,
    mut request: LlmRequest,
) -> Result<Value, LlmError> {
    // First attempt — без grammar.
    request.grammar = None;
    match provider.generate(request.clone()).await {
        Ok(v) => Ok(v),
        Err(first_err) => {
            // Только LlmError::Provider triggers retry. Auth / Network /
            // QuotaExceeded / NotImplemented — не parse failures, retry
            // не поможет, propagate сразу.
            if !matches!(first_err, LlmError::Provider(_)) {
                return Err(first_err);
            }
            log::warn!("local LLM call failed ({first_err}), retrying с GBNF grammar fallback");
            request.grammar = Some(UNIVERSAL_JSON_OBJECT_GRAMMAR.to_string());
            provider.generate(request).await.map_err(|e| {
                log::warn!("GBNF fallback also failed: {e} (propagating original parse failure)");
                e
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockProvider {
        responses: Mutex<Vec<Result<Value, LlmError>>>,
        captured: Mutex<Vec<LlmRequest>>,
    }
    impl MockProvider {
        fn new(responses: Vec<Result<Value, LlmError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                captured: Mutex::new(Vec::new()),
            }
        }
        fn call_count(&self) -> usize {
            self.captured.lock().unwrap().len()
        }
        fn captured(&self) -> Vec<LlmRequest> {
            self.captured.lock().unwrap().clone()
        }
    }
    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn generate(&self, request: LlmRequest) -> Result<Value, LlmError> {
            self.captured.lock().unwrap().push(request);
            let mut guard = self.responses.lock().unwrap();
            if guard.is_empty() {
                return Err(LlmError::Provider("no scripted response".into()));
            }
            guard.remove(0)
        }
    }

    fn dummy_request() -> LlmRequest {
        LlmRequest {
            model: None,
            system: "system".into(),
            input: "input".into(),
            max_tokens: Some(1024),
            grammar: None,
        }
    }

    #[test]
    fn grammar_string_non_empty_and_contains_json_rules() {
        let g = UNIVERSAL_JSON_OBJECT_GRAMMAR;
        assert!(!g.trim().is_empty());
        assert!(g.contains("root"));
        assert!(g.contains("object"));
        assert!(g.contains("array"));
        assert!(g.contains("string"));
        assert!(g.contains("number"));
    }

    #[tokio::test]
    async fn fallback_first_attempt_success_no_retry() {
        let mock = MockProvider::new(vec![Ok(serde_json::json!({"ok": true}))]);
        let result = generate_with_grammar_fallback(&mock, dummy_request())
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(mock.call_count(), 1);
        // First call grammar=None.
        assert!(mock.captured()[0].grammar.is_none());
    }

    #[tokio::test]
    async fn fallback_retries_on_provider_error_with_grammar_set() {
        let mock = MockProvider::new(vec![
            Err(LlmError::Provider("malformed JSON".into())),
            Ok(serde_json::json!({"recovered": true})),
        ]);
        let result = generate_with_grammar_fallback(&mock, dummy_request())
            .await
            .unwrap();
        assert_eq!(result["recovered"], true);
        assert_eq!(mock.call_count(), 2);
        let captured = mock.captured();
        // First call — no grammar; second call — grammar set.
        assert!(captured[0].grammar.is_none());
        assert_eq!(
            captured[1].grammar.as_deref(),
            Some(UNIVERSAL_JSON_OBJECT_GRAMMAR)
        );
    }

    #[tokio::test]
    async fn fallback_propagates_error_when_retry_also_fails() {
        let mock = MockProvider::new(vec![
            Err(LlmError::Provider("first crash".into())),
            Err(LlmError::Provider("retry crash".into())),
        ]);
        let err = generate_with_grammar_fallback(&mock, dummy_request())
            .await
            .unwrap_err();
        match err {
            LlmError::Provider(msg) => assert!(msg.contains("retry crash")),
            other => panic!("expected Provider variant, got {other:?}"),
        }
        assert_eq!(mock.call_count(), 2);
    }

    #[tokio::test]
    async fn fallback_does_not_retry_on_non_provider_error() {
        // Auth / Network / QuotaExceeded — retry бессмыслен.
        let mock = MockProvider::new(vec![Err(LlmError::QuotaExceeded)]);
        let err = generate_with_grammar_fallback(&mock, dummy_request())
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::QuotaExceeded));
        assert_eq!(
            mock.call_count(),
            1,
            "non-Provider error should NOT trigger retry"
        );
    }
}
