//! [M14 T-10 Phase A] Local engine orchestrator — chain classifier + main v2.
//!
//! ## Phase A flow
//!
//! 1. `classifier::classify_call` (best-effort, lightweight ~256 tokens).
//! 2. `recap::build_v2_system_prompt` с `known_call_type` hint при success'е
//!    classifier'а; иначе `None` — LLM сам классифицирует.
//! 3. `LlmProvider::generate` с full transcript → JSON Value → caller
//!    (`pipeline::mod.rs::run_local_inner`) делает persist через
//!    существующий `recap::persist_recap_from_json`.
//!
//! ## Что НЕ делает (deferred)
//!
//! - **Phase B (T-05/T-06):** chunking + map-reduce. Длинный transcript
//!   (>8K tokens для 1.5b, >24K для 7b) сейчас будет ловить ctx overflow на
//!   main call — AppError propagates.
//! - **Phase C (T-07):** 8 expert prompts per call_type — заменить universal.
//! - **Phase D (T-08):** action-item post-pass — отдельный validate-call.
//! - **Phase E (T-09):** GBNF grammar fallback для бойких моделей.
//!
//! ## Тестирование
//!
//! Orchestrator принимает `&dyn LlmProvider`, что позволяет
//! mock-имплементацию в unit тестах (без sidecar). Production использует
//! `LocalLlamaProvider`.

use crate::pipeline::classifier;
use crate::pipeline::recap;
use crate::pipeline::summary_v2::CallType;
use crate::providers::llm::{LlmProvider, LlmRequest};
use crate::AppError;

const MAIN_MAX_TOKENS: u32 = 4096;

/// Контекст для одного оркестратор-runs'а.
pub(crate) struct LocalOrchestratorCtx<'a> {
    pub transcript_md: &'a str,
    pub lang_detected: Option<&'a str>,
    pub known_speakers: Option<&'a str>,
}

/// Запуск Phase A pipeline: classifier (best-effort) → main v2 generation.
///
/// Возвращает финальный JSON Value, который caller сразу скармливает в
/// `recap::persist_recap_from_json`. Никакой DB I/O здесь нет.
pub(crate) async fn run_v2_pipeline(
    provider: &dyn LlmProvider,
    ctx: LocalOrchestratorCtx<'_>,
) -> Result<serde_json::Value, AppError> {
    // 1. Classifier (best-effort).
    let head = classifier::extract_classifier_head(
        ctx.transcript_md,
        classifier::MAX_CLASSIFIER_HEAD_CHARS,
    );
    let cls_result = classifier::classify_call(provider, head, ctx.lang_detected).await;
    let known_type: Option<CallType> = match &cls_result {
        Ok(r) => {
            log::info!(
                "local classifier OK: call_type={:?} confidence={:.2} language={}",
                r.call_type,
                r.confidence,
                r.language
            );
            Some(r.call_type)
        }
        Err(e) => {
            log::warn!("local classifier failed (continuing без hint): {e}");
            None
        }
    };

    // 2. Main v2 generation с (опциональным) hint'ом.
    let system = recap::build_v2_system_prompt(ctx.lang_detected, ctx.known_speakers, known_type);
    let request = LlmRequest {
        model: None,
        system,
        input: ctx.transcript_md.to_string(),
        max_tokens: Some(MAIN_MAX_TOKENS),
    };
    provider
        .generate(request)
        .await
        .map_err(|e| AppError::Other(format!("local llm: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    use crate::providers::llm::LlmError;

    /// Mock LLM provider: возвращает scripted responses по очереди.
    /// Запоминает captured requests чтобы тесты могли assert'ить system prompt.
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

        fn captured_systems(&self) -> Vec<String> {
            self.captured
                .lock()
                .unwrap()
                .iter()
                .map(|r| r.system.clone())
                .collect()
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

    fn minimal_v2_json() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 2,
            "title": "stub",
            "summary": "stub",
            "key_points": [],
            "language": "ru",
            "call_type": "standup",
            "call_type_confidence": 0.8,
            "participants": [],
            "action_items": [],
            "decisions": [],
            "open_questions": [],
            "mom": "",
        })
    }

    #[tokio::test]
    async fn orchestrator_classifier_success_passes_hint_to_main() {
        let cls_response = serde_json::json!({
            "call_type": "standup",
            "confidence": 0.92,
            "language": "ru",
        });
        let mock = MockProvider::new(vec![Ok(cls_response), Ok(minimal_v2_json())]);
        let ctx = LocalOrchestratorCtx {
            transcript_md: "Speaker 0: Yesterday I did X. Today I'll do Y. No blockers.",
            lang_detected: Some("ru"),
            known_speakers: None,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap();
        assert_eq!(result["call_type"], "standup");

        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 2, "expected 2 LLM calls (classifier + main)");
        // Main call (index 1) must contain the classification hint.
        assert!(
            systems[1].contains("Classification hint"),
            "main prompt missing hint: {}",
            systems[1]
        );
        assert!(
            systems[1].contains("`standup`"),
            "main prompt missing call_type=standup hint"
        );
    }

    #[tokio::test]
    async fn orchestrator_classifier_failure_falls_back_to_no_hint() {
        let mock = MockProvider::new(vec![
            Err(LlmError::Provider("simulated classifier crash".into())),
            Ok(minimal_v2_json()),
        ]);
        let ctx = LocalOrchestratorCtx {
            transcript_md: "stub transcript",
            lang_detected: Some("en"),
            known_speakers: None,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap();
        assert_eq!(result["schema_version"], 2);

        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 2);
        // Main call still happens, but без hint blоka.
        assert!(
            !systems[1].contains("Classification hint"),
            "main prompt should NOT contain hint on classifier failure"
        );
    }

    #[tokio::test]
    async fn orchestrator_main_failure_propagates() {
        let cls_response = serde_json::json!({
            "call_type": "sales_demo",
            "confidence": 0.7,
        });
        let mock = MockProvider::new(vec![
            Ok(cls_response),
            Err(LlmError::Provider("main timeout".into())),
        ]);
        let ctx = LocalOrchestratorCtx {
            transcript_md: "transcript",
            lang_detected: None,
            known_speakers: None,
        };
        let err = run_v2_pipeline(&mock, ctx).await.unwrap_err();
        assert!(
            err.to_string().contains("local llm"),
            "expected wrapped main-llm error, got: {err}"
        );
    }
}
