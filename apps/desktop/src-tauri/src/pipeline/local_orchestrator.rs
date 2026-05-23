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

use crate::local_engine::preset::LocalEnginePreset;
use crate::pipeline::action_item_post_pass;
use crate::pipeline::chunker;
use crate::pipeline::classifier;
use crate::pipeline::expert_prompts;
use crate::pipeline::map_reduce;
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
    /// [Phase B] Active preset — определяет chunk size + trigger threshold.
    pub preset: LocalEnginePreset,
}

/// Запуск local v2 pipeline. Phase A — single-pass для коротких transcripts.
/// Phase B — map-reduce когда transcript превышает per-preset threshold.
///
/// Возвращает финальный JSON Value, который caller сразу скармливает в
/// `recap::persist_recap_from_json`. Никакой DB I/O здесь нет.
pub(crate) async fn run_v2_pipeline(
    provider: &dyn LlmProvider,
    ctx: LocalOrchestratorCtx<'_>,
) -> Result<serde_json::Value, AppError> {
    // 1. Classifier (best-effort) — используется в обоих path'ях.
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

    // 2. Dispatch: Phase B map-reduce для длинных transcripts.
    let chunk_cfg = chunker::ChunkConfig::for_preset(ctx.preset);
    let mut summary_json = if chunker::needs_chunking(ctx.transcript_md, &chunk_cfg) {
        let chunks = chunker::chunk_transcript(ctx.transcript_md, &chunk_cfg);
        log::info!(
            "local map-reduce: {} chunks (transcript ~{} tokens, preset={:?})",
            chunks.len(),
            chunker::estimate_tokens(ctx.transcript_md),
            ctx.preset
        );
        map_reduce::run_map_reduce(
            provider,
            &chunks,
            ctx.lang_detected,
            known_type,
            ctx.known_speakers,
        )
        .await?
    } else {
        // 3. Phase A: single-pass main v2 generation.
        // [M14 T-07 Phase C] Expert prompt когда classifier дал call_type;
        // universal fallback на classifier failure (no regression).
        let system = match known_type {
            Some(t) => {
                expert_prompts::build_expert_system_prompt(t, ctx.lang_detected, ctx.known_speakers)
            }
            None => recap::build_v2_system_prompt(ctx.lang_detected, ctx.known_speakers, None),
        };
        let request = LlmRequest {
            model: None,
            system,
            input: ctx.transcript_md.to_string(),
            max_tokens: Some(MAIN_MAX_TOKENS),
        };
        provider
            .generate(request)
            .await
            .map_err(|e| AppError::Other(format!("local llm: {e}")))?
    };

    // 4. [M14 T-08 Phase D] Action-item post-pass — best-effort refinement
    // действующих action_items (categories, owner_confidence, dedup, evidence
    // re-check). На LLM failure / garbage output → keep original без regression.
    // Skip когда action_items пустой массив.
    let action_items = summary_json
        .get("action_items")
        .cloned()
        .unwrap_or(serde_json::Value::Array(Vec::new()));
    let refined = action_item_post_pass::refine_action_items(
        provider,
        action_items,
        ctx.transcript_md,
        ctx.lang_detected,
    )
    .await;
    summary_json = action_item_post_pass::merge_refined_action_items(summary_json, refined);
    Ok(summary_json)
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

        fn call_count(&self) -> usize {
            self.captured.lock().unwrap().len()
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
            preset: LocalEnginePreset::Light,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap();
        assert_eq!(result["call_type"], "standup");

        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 2, "expected 2 LLM calls (classifier + main)");
        // [M14 T-07 Phase C] Main call (index 1) теперь использует expert
        // prompt, не universal с Classification hint. Проверяем SPECIALIZED
        // GUIDE marker + standup slug present.
        assert!(
            systems[1].contains("SPECIALIZED GUIDE"),
            "main prompt missing expert guide: {}",
            &systems[1][..200.min(systems[1].len())]
        );
        assert!(
            systems[1].contains("`standup`"),
            "main prompt missing call_type=standup focus"
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
            preset: LocalEnginePreset::Light,
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
            preset: LocalEnginePreset::Light,
        };
        let err = run_v2_pipeline(&mock, ctx).await.unwrap_err();
        assert!(
            err.to_string().contains("local llm"),
            "expected wrapped main-llm error, got: {err}"
        );
    }

    fn make_long_transcript(turns: usize, chars_per_turn: usize) -> String {
        let mut out = String::from("# Transcript\n\n");
        for i in 0..turns {
            out.push_str(&format!("**Speaker {}** [{}:00]:\n", i % 3, i));
            out.push_str(&"a".repeat(chars_per_turn));
            out.push_str("\n\n");
        }
        out
    }

    #[tokio::test]
    async fn orchestrator_short_path_uses_expert_when_known_type() {
        // Classifier returns standup → main call должен использовать expert
        // prompt (focused) — содержит ## Yesterday / ## Today / ## Blockers
        // и НЕ содержит ## Customer pain / ## Demo flow.
        let cls_response = serde_json::json!({
            "call_type": "standup",
            "confidence": 0.9,
        });
        let mock = MockProvider::new(vec![Ok(cls_response), Ok(minimal_v2_json())]);
        let ctx = LocalOrchestratorCtx {
            transcript_md: "**A** [0:00]:\nyesterday I did x",
            lang_detected: Some("en"),
            known_speakers: None,
            preset: LocalEnginePreset::Light,
        };
        run_v2_pipeline(&mock, ctx).await.unwrap();
        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 2);
        let main_prompt = &systems[1];
        // Focused headers for standup.
        assert!(
            main_prompt.contains("## Yesterday"),
            "expert main prompt missing standup headers: {}",
            &main_prompt[..200.min(main_prompt.len())]
        );
        assert!(main_prompt.contains("## Today"));
        // Other types' SPECIALIZED headers absent.
        assert!(!main_prompt.contains("## Customer pain"));
        assert!(!main_prompt.contains("## Demo flow"));
        assert!(!main_prompt.contains("## Job to be done"));
        // SPECIALIZED GUIDE marker present.
        assert!(main_prompt.contains("SPECIALIZED GUIDE"));
    }

    #[tokio::test]
    async fn orchestrator_short_transcript_uses_single_pass() {
        let cls_response = serde_json::json!({
            "call_type": "standup",
            "confidence": 0.9,
        });
        let mock = MockProvider::new(vec![Ok(cls_response), Ok(minimal_v2_json())]);
        let ctx = LocalOrchestratorCtx {
            transcript_md: "**A** [0:00]:\nshort transcript content",
            lang_detected: Some("ru"),
            known_speakers: None,
            preset: LocalEnginePreset::Light,
        };
        run_v2_pipeline(&mock, ctx).await.unwrap();
        let systems = mock.captured_systems();
        assert_eq!(
            systems.len(),
            2,
            "short transcript expects 2 calls (classifier + main), got {}",
            systems.len()
        );
        // Main call (index 1) — full v2 prompt, не reduce.
        assert!(systems[1].contains("OUTPUT SCHEMA"));
        assert!(!systems[1].contains("MAP_OUTPUTS"));
    }

    #[tokio::test]
    async fn orchestrator_long_transcript_uses_map_reduce() {
        // Light preset: trigger_threshold = 24_000 chars. 10 turns × 3000 chars = 30K body
        // + headers — превышает.
        let long = make_long_transcript(10, 3000);
        // Точно посчитаем сколько chunks chunker произведёт — это определяет
        // количество map calls (1 на chunk) + reduce в конце.
        let chunk_count = crate::pipeline::chunker::chunk_transcript(
            &long,
            &crate::pipeline::chunker::ChunkConfig::for_preset(LocalEnginePreset::Light),
        )
        .len();
        // Stack: classifier + N maps + 1 reduce.
        let mut responses: Vec<Result<serde_json::Value, LlmError>> = Vec::new();
        responses.push(Ok(serde_json::json!({
            "call_type": "standup",
            "confidence": 0.85,
        })));
        for i in 0..chunk_count {
            responses.push(Ok(serde_json::json!({
                "chunk_idx": i,
                "facts": [format!("fact {i}")],
                "decisions_candidates": [],
                "action_candidates": [],
                "open_questions_candidates": [],
                "topic_tags": [],
                "participants_mentioned": [],
            })));
        }
        responses.push(Ok(minimal_v2_json()));
        let mock = MockProvider::new(responses);
        let ctx = LocalOrchestratorCtx {
            transcript_md: &long,
            lang_detected: Some("ru"),
            known_speakers: None,
            preset: LocalEnginePreset::Light,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap();
        assert_eq!(result["schema_version"], 2);
        let systems = mock.captured_systems();
        assert!(
            systems.len() >= 4,
            "long transcript should trigger map-reduce (≥4 calls), got {}",
            systems.len()
        );
        // Последний call — reduce, содержит MAP_OUTPUTS.
        let last = systems.last().unwrap();
        assert!(
            last.contains("MAP_OUTPUTS"),
            "last call should be reduce, got: {}",
            &last[..200.min(last.len())]
        );
    }

    fn v2_json_with_action_items() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 2,
            "title": "stub",
            "summary": "stub",
            "key_points": [],
            "language": "ru",
            "call_type": "standup",
            "call_type_confidence": 0.8,
            "participants": [],
            "action_items": [
                {
                    "id": "a1",
                    "text": "Ship by Friday",
                    "owner_hint": "Alice",
                    "owner_confidence": 0.95,
                    "due": null,
                    "due_confidence": 0.0,
                    "category": "commitment",
                    "evidence": { "quote": "I'll ship it", "speaker": "Alice" }
                }
            ],
            "decisions": [],
            "open_questions": [],
            "mom": "",
        })
    }

    #[tokio::test]
    async fn orchestrator_runs_post_pass_after_main_when_action_items_non_empty() {
        // classifier OK + main OK (с non-empty action_items) → post-pass triggered.
        // 3 LLM calls total. Refined action_items replace original.
        let refined = serde_json::json!([
            {
                "id": "a1",
                "text": "Ship by Friday — refined",
                "owner_hint": "Alice",
                "owner_confidence": 0.9,
                "due": null,
                "due_confidence": 0.0,
                "category": "commitment",
                "evidence": { "quote": "I'll ship it", "speaker": "Alice" }
            }
        ]);
        let mock = MockProvider::new(vec![
            Ok(serde_json::json!({ "call_type": "standup", "confidence": 0.9 })),
            Ok(v2_json_with_action_items()),
            Ok(serde_json::json!({ "action_items": refined.clone() })),
        ]);
        let ctx = LocalOrchestratorCtx {
            transcript_md: "**A** [0:00]:\nI'll ship it",
            lang_detected: Some("en"),
            known_speakers: None,
            preset: LocalEnginePreset::Light,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap();
        assert_eq!(
            mock.call_count(),
            3,
            "expected 3 calls (classifier + main + post-pass)"
        );
        assert_eq!(result["action_items"], refined);
        // Other fields preserved from main response.
        assert_eq!(result["title"], "stub");
        assert_eq!(result["call_type"], "standup");
    }

    #[tokio::test]
    async fn orchestrator_post_pass_failure_keeps_original_action_items() {
        // classifier OK + main OK + post-pass FAIL → original action_items preserved.
        let original_main = v2_json_with_action_items();
        let original_items = original_main["action_items"].clone();
        let mock = MockProvider::new(vec![
            Ok(serde_json::json!({ "call_type": "standup", "confidence": 0.9 })),
            Ok(original_main),
            Err(LlmError::Provider("post-pass crash".into())),
        ]);
        let ctx = LocalOrchestratorCtx {
            transcript_md: "**A** [0:00]:\nI'll ship it",
            lang_detected: Some("en"),
            known_speakers: None,
            preset: LocalEnginePreset::Light,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap();
        // Post-pass crashed → original kept.
        assert_eq!(result["action_items"], original_items);
        assert_eq!(mock.call_count(), 3);
    }
}
