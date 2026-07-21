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
//! - `refine_chain::run_refine_chain` (initial + refine шаги) [F1]
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

/// [P8.3] Generate с **обязательной** GBNF grammar на первой попытке. Маленькие
/// модели (Qwen 1.5-3B) почти всегда дают невалидный JSON без grammar
/// constraint — first-attempt-without-grammar pattern удваивал LLM calls
/// на каждом локальном стейдже (classifier, recap, action_items, decisions).
///
/// На provider error retry НЕ делаем — grammar gives best-effort JSON
/// constraint, повторный вызов с тем же grammar даст тот же результат.
/// Если grammar+model не справляется — error propagate'ится в caller для
/// surface через `recap_failed_reason`.
///
/// Backward-compat alias `generate_with_grammar_fallback` сохранён на этот
/// commit; callers переименуются в follow-up если нужно.
pub(crate) async fn generate_with_grammar_fallback(
    provider: &dyn LlmProvider,
    mut request: LlmRequest,
) -> Result<Value, LlmError> {
    request.grammar = Some(UNIVERSAL_JSON_OBJECT_GRAMMAR.to_string());
    provider.generate(request).await
}

/// [M14 follow-up] Generate под **строгую JSON Schema** (сильнее generic
/// grammar). `LocalLlamaProvider` передаёт `--json-schema-file` → llama.cpp
/// конвертит схему в GBNF и форсит ИМЕННО форму (required-поля, enum, массивы).
/// Используется на стейджах с known shape: classifier (`CLASSIFIER_JSON_SCHEMA`)
/// и main/reduce summary (`SUMMARY_V2_JSON_SCHEMA`). Cloud-провайдер игнорит
/// `json_schema`. Single attempt — без retry (как grammar-вариант).
pub(crate) async fn generate_with_schema(
    provider: &dyn LlmProvider,
    mut request: LlmRequest,
    json_schema: &str,
) -> Result<Value, LlmError> {
    request.json_schema = Some(json_schema.to_string());
    provider.generate(request).await
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
            json_schema: None,
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

    // [P8.3] gbnf wrapper теперь всегда передаёт grammar на первой попытке.
    // Retry-pattern удалён — маленькие local модели слишком ненадёжны без
    // grammar constraint, retry с grammar после free-form attempt лишь
    // удваивал LLM calls на каждом этапе pipeline'а.

    #[tokio::test]
    async fn grammar_applied_on_first_attempt() {
        let mock = MockProvider::new(vec![Ok(serde_json::json!({"ok": true}))]);
        let result = generate_with_grammar_fallback(&mock, dummy_request())
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(mock.call_count(), 1);
        // grammar set на первую же попытку — без free-form attempt.
        let captured = mock.captured();
        assert_eq!(
            captured[0].grammar.as_deref(),
            Some(UNIVERSAL_JSON_OBJECT_GRAMMAR),
            "grammar должна быть set с первого вызова"
        );
    }

    #[tokio::test]
    async fn no_retry_on_provider_error() {
        // Раньше Provider error → retry с grammar. Теперь grammar всегда
        // активен — retry не имеет смысла, error propagate'ится сразу.
        let mock = MockProvider::new(vec![Err(LlmError::Provider("malformed JSON".into()))]);
        let err = generate_with_grammar_fallback(&mock, dummy_request())
            .await
            .unwrap_err();
        match err {
            LlmError::Provider(msg) => assert!(msg.contains("malformed JSON")),
            other => panic!("expected Provider variant, got {other:?}"),
        }
        assert_eq!(mock.call_count(), 1, "no retry — single attempt only");
    }

    #[tokio::test]
    async fn no_retry_on_non_provider_error() {
        // Auth / Network / QuotaExceeded — propagate как раньше.
        let mock = MockProvider::new(vec![Err(LlmError::QuotaExceeded)]);
        let err = generate_with_grammar_fallback(&mock, dummy_request())
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::QuotaExceeded));
        assert_eq!(mock.call_count(), 1);
    }

    #[tokio::test]
    async fn caller_grammar_overridden_with_universal() {
        // Если caller передал свою grammar — wrapper её overrides универсальной.
        // Per-shape grammar — backlog (Phase E PRD §5.7).
        let mut req = dummy_request();
        req.grammar = Some("root ::= \"custom\"".to_string());
        let mock = MockProvider::new(vec![Ok(serde_json::json!({}))]);
        let _ = generate_with_grammar_fallback(&mock, req).await.unwrap();
        let captured = mock.captured();
        assert_eq!(
            captured[0].grammar.as_deref(),
            Some(UNIVERSAL_JSON_OBJECT_GRAMMAR)
        );
    }

    #[tokio::test]
    async fn schema_variant_sets_json_schema_not_grammar() {
        // generate_with_schema форсит форму через json_schema (→ --json-schema-file),
        // grammar при этом НЕ ставится.
        let mock = MockProvider::new(vec![Ok(serde_json::json!({"ok": true}))]);
        let schema = r#"{"type":"object","required":["call_type"]}"#;
        let _ = generate_with_schema(&mock, dummy_request(), schema)
            .await
            .unwrap();
        let captured = mock.captured();
        assert_eq!(captured[0].json_schema.as_deref(), Some(schema));
        assert!(
            captured[0].grammar.is_none(),
            "в schema-режиме grammar не ставится"
        );
        assert_eq!(mock.call_count(), 1, "single attempt");
    }
}
