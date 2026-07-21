//! [M14 T-10] Local engine orchestrator — chain classifier + main v2.
//!
//! ## Flow
//!
//! 1. `classifier::classify_call` (best-effort, lightweight ~256 tokens).
//! 2. Короткий transcript → single-pass (expert/universal v2 prompt).
//!    Длинный (> `trigger_tokens`) → [F1] последовательный refine-чейн
//!    (`refine_chain::run_refine_chain`): каждый чанк расширяет/правит
//!    накопленный CallSummaryV2.
//! 3. Action-item post-pass + narrative — на финальном JSON.
//! 4. Caller (`pipeline::mod.rs`) делает persist через
//!    `recap::persist_recap_from_json` c `outcome.pipeline_mode`.
//!
//! ## Step-события [F3]
//!
//! Каждый шаг эмитится в `ctx.steps` (`recap:step`): classify → N×refine
//! (или один generate) → post_pass → narrative. UI рендерит thinking-блок.
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
use crate::pipeline::gbnf;
use crate::pipeline::recap;
use crate::pipeline::recap_steps::{preview_from_summary, step_event, RecapStepSink};
use crate::pipeline::refine_chain;
use crate::pipeline::summary_v2::CallType;
use crate::providers::llm::{LlmProvider, LlmRequest};
use crate::AppError;

/// [P8.2] Понижен с 4096 → 2560. На 3B Qwen ~16 tok/s, 4096 budget = ~5 мин
/// generation на main recap, причём grammar root=object разрешает arbitrary
/// nested filler. Для типичного 5-30 min звонка JSON recap ≤2560 tokens
/// (title + summary 200-500 слов + 3-7 key_points + 0-3 action_items).
/// Очень длинные звонки идут через refine-чейн с собственным бюджетом
/// (`refine_chain::REFINE_MAX_TOKENS`).
const MAIN_MAX_TOKENS: u32 = 2560;

/// Контекст для одного оркестратор-runs'а.
pub(crate) struct LocalOrchestratorCtx<'a> {
    pub transcript_md: &'a str,
    pub lang_detected: Option<&'a str>,
    pub known_speakers: Option<&'a str>,
    /// [Phase B] Active preset — определяет chunk size + trigger threshold.
    pub preset: LocalEnginePreset,
    /// [F3] Приёмник step-событий (thinking-блок). `NoopStepSink` в тестах.
    pub steps: &'a dyn RecapStepSink,
}

/// [F1] Итог local-генерации: JSON + режим для `calls.summary_pipeline_mode`.
#[derive(Debug)]
pub(crate) struct LocalRecapOutcome {
    pub summary_json: serde_json::Value,
    /// `one_shot` | `refine_chain`
    pub pipeline_mode: &'static str,
}

/// Запуск local v2 pipeline. Короткий transcript — single-pass; длинный —
/// [F1] refine-чейн. Возвращает финальный JSON + pipeline_mode; caller
/// делает persist. Никакой DB I/O здесь нет.
pub(crate) async fn run_v2_pipeline(
    provider: &dyn LlmProvider,
    ctx: LocalOrchestratorCtx<'_>,
) -> Result<LocalRecapOutcome, AppError> {
    // 1. Classifier (best-effort) — используется в обоих path'ях.
    // total_steps ещё неизвестен (нет chunk count) → 0 = «не знаю».
    ctx.steps.emit(step_event(0, 0, "classify", "started"));
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

    // 2. Dispatch: короткий → single-pass; длинный → [F1] refine-чейн.
    let chunk_cfg = chunker::ChunkConfig::for_preset(ctx.preset);
    let needs_chunking = chunker::needs_chunking(ctx.transcript_md, &chunk_cfg);
    // Layout шагов: classify(0) + [N×refine | generate](1..) + post_pass + narrative.
    let gen_steps: u32 = if needs_chunking {
        let refine_cfg = chunker::ChunkConfig::for_refine(ctx.preset);
        chunker::chunk_transcript(ctx.transcript_md, &refine_cfg).len() as u32
    } else {
        1
    };
    let total_steps = 1 + gen_steps + 2;
    ctx.steps.emit(step_event(
        0,
        total_steps,
        "classify",
        if cls_result.is_ok() { "done" } else { "failed" },
    ));

    let (mut summary_json, pipeline_mode) = if needs_chunking {
        let refine_cfg = chunker::ChunkConfig::for_refine(ctx.preset);
        let chunks = chunker::chunk_transcript(ctx.transcript_md, &refine_cfg);
        log::info!(
            "local refine-chain: {} chunks (transcript ~{} tokens, preset={:?})",
            chunks.len(),
            chunker::estimate_tokens(ctx.transcript_md),
            ctx.preset
        );
        let outcome = refine_chain::run_refine_chain(
            provider,
            &chunks,
            ctx.lang_detected,
            known_type,
            ctx.known_speakers,
            ctx.steps,
            1,
            total_steps,
        )
        .await?;
        if outcome.chunks_failed > 0 {
            log::warn!(
                "refine chain: {}/{} чанков пропущено (skip после ретрая)",
                outcome.chunks_failed,
                outcome.chunks_total
            );
        }
        (outcome.summary_json, "refine_chain")
    } else {
        // Single-pass main v2 generation.
        // [M14 T-07 Phase C] Expert prompt когда classifier дал call_type;
        // universal fallback на classifier failure (no regression).
        ctx.steps
            .emit(step_event(1, total_steps, "generate", "started"));
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
            grammar: None,
            json_schema: None,
        };
        let json = gbnf::generate_with_schema(
            provider,
            request,
            crate::pipeline::llm_schemas::SUMMARY_V2_JSON_SCHEMA,
        )
        .await
        .map_err(|e| {
            ctx.steps
                .emit(step_event(1, total_steps, "generate", "failed"));
            AppError::Other(format!("local llm: {e}"))
        })?;
        let mut done = step_event(1, total_steps, "generate", "done");
        done.preview = preview_from_summary(&json);
        ctx.steps.emit(done);
        (json, "one_shot")
    };

    // 3. [M14 T-08 Phase D] Action-item post-pass — best-effort refinement
    // действующих action_items (categories, owner_confidence, dedup, evidence
    // re-check). На LLM failure / garbage output → keep original без regression.
    // Skip когда action_items пустой массив.
    let post_pass_idx = 1 + gen_steps;
    ctx.steps.emit(step_event(
        post_pass_idx,
        total_steps,
        "post_pass",
        "started",
    ));
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
    ctx.steps
        .emit(step_event(post_pass_idx, total_steps, "post_pass", "done"));

    // 4. [recap-rich] Нарратив-минутки — отдельный write-проход из готовой
    // структуры + головы транскрипта. Best-effort: пусто → секция опускается.
    let narrative_idx = post_pass_idx + 1;
    ctx.steps.emit(step_event(
        narrative_idx,
        total_steps,
        "narrative",
        "started",
    ));
    let narrative = crate::pipeline::narrative::generate_narrative(
        provider,
        &summary_json,
        ctx.transcript_md,
        ctx.lang_detected,
    )
    .await;
    if !narrative.is_empty() {
        summary_json["narrative"] = serde_json::Value::String(narrative);
    }
    ctx.steps
        .emit(step_event(narrative_idx, total_steps, "narrative", "done"));

    Ok(LocalRecapOutcome {
        summary_json,
        pipeline_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    use crate::pipeline::recap_steps::test_support::VecSink;
    use crate::pipeline::recap_steps::NoopStepSink;
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
            steps: &NoopStepSink,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap();
        assert_eq!(result.pipeline_mode, "one_shot");
        let result = result.summary_json;
        assert_eq!(result["call_type"], "standup");

        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 3, "classifier + main + narrative");
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
        // [P8.3] gbnf wrapper больше не ретраит — single attempt с grammar.
        // Classifier failure = 1 Err, дальше main call продолжает без hint.
        let mock = MockProvider::new(vec![
            Err(LlmError::Provider("classifier crash".into())),
            Ok(minimal_v2_json()),
        ]);
        let ctx = LocalOrchestratorCtx {
            transcript_md: "stub transcript",
            lang_detected: Some("en"),
            known_speakers: None,
            preset: LocalEnginePreset::Light,
            steps: &NoopStepSink,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap().summary_json;
        assert_eq!(result["schema_version"], 2);

        let systems = mock.captured_systems();
        // classifier + main + narrative = 3.
        assert_eq!(systems.len(), 3);
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
            steps: &NoopStepSink,
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
            steps: &NoopStepSink,
        };
        run_v2_pipeline(&mock, ctx).await.unwrap();
        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 3);
        let main_prompt = &systems[1];
        // [MoM cleanup] Expert path определяется по slug + SPECIALIZED GUIDE,
        // без MoM-заголовков / type_specific_block.
        assert!(
            main_prompt.contains("`standup`"),
            "expert main prompt missing standup slug: {}",
            &main_prompt[..200.min(main_prompt.len())]
        );
        assert!(main_prompt.contains("SPECIALIZED GUIDE"));
        assert!(!main_prompt.contains("## Yesterday"));
        assert!(!main_prompt.contains("type_specific_block"));
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
            steps: &NoopStepSink,
        };
        let outcome = run_v2_pipeline(&mock, ctx).await.unwrap();
        assert_eq!(outcome.pipeline_mode, "one_shot");
        let systems = mock.captured_systems();
        assert_eq!(
            systems.len(),
            3,
            "short transcript: classifier + main + narrative, got {}",
            systems.len()
        );
        // Main call (index 1) — full v2 prompt, не refine.
        assert!(systems[1].contains("OUTPUT SCHEMA"));
        assert!(!systems[1].contains("CURRENT_RECAP"));
    }

    #[tokio::test]
    async fn orchestrator_long_transcript_uses_refine_chain() {
        // Light preset: длинный transcript → последовательный refine-чейн.
        let long = make_long_transcript(10, 3000);
        // Чейн режет refine-конфигом (чанки меньше map-эры).
        let chunk_count = crate::pipeline::chunker::chunk_transcript(
            &long,
            &crate::pipeline::chunker::ChunkConfig::for_refine(LocalEnginePreset::Light),
        )
        .len();
        assert!(chunk_count >= 2, "фикстура должна дать ≥2 чанков");
        // Stack: classifier + N refine-шагов (каждый отдаёт полный v2) + narrative.
        let mut responses: Vec<Result<serde_json::Value, LlmError>> = Vec::new();
        responses.push(Ok(serde_json::json!({
            "call_type": "standup",
            "confidence": 0.85,
        })));
        for i in 0..chunk_count {
            let mut v2 = minimal_v2_json();
            v2["title"] = serde_json::json!(format!("after part {i}"));
            responses.push(Ok(v2));
        }
        responses.push(Ok(minimal_v2_json()));
        let mock = MockProvider::new(responses);
        let sink = VecSink::new();
        let ctx = LocalOrchestratorCtx {
            transcript_md: &long,
            lang_detected: Some("ru"),
            known_speakers: None,
            preset: LocalEnginePreset::Light,
            steps: &sink,
        };
        let outcome = run_v2_pipeline(&mock, ctx).await.unwrap();
        assert_eq!(outcome.pipeline_mode, "refine_chain");
        assert_eq!(outcome.summary_json["schema_version"], 2);
        let systems = mock.captured_systems();
        // classifier + chunk_count генераций + narrative (post-pass скипается:
        // action_items пуст).
        assert_eq!(systems.len(), 1 + chunk_count + 1);
        // Refine-вызовы (2-й и дальше) содержат CURRENT_RECAP; MAP_OUTPUTS мёртв.
        assert!(systems[2].contains("CURRENT_RECAP"));
        assert!(systems.iter().all(|s| !s.contains("MAP_OUTPUTS")));
        // [F3] События: classify → N×refine → post_pass → narrative, по порядку.
        let events = sink.events();
        let kinds: Vec<&str> = events
            .iter()
            .filter(|e| e.status == "done")
            .map(|e| e.kind)
            .collect();
        assert_eq!(kinds[0], "classify");
        assert_eq!(
            kinds.iter().filter(|k| **k == "refine").count(),
            chunk_count
        );
        assert_eq!(kinds[kinds.len() - 2], "post_pass");
        assert_eq!(kinds[kinds.len() - 1], "narrative");
        // total_steps консистентен после classify.
        let expected_total = 1 + chunk_count as u32 + 2;
        assert!(events
            .iter()
            .skip(1)
            .all(|e| e.total_steps == expected_total));
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
            steps: &NoopStepSink,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap().summary_json;
        assert_eq!(
            mock.call_count(),
            4,
            "expected 4 calls (classifier + main + post-pass + narrative)"
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
        // [P8.3] gbnf wrapper больше не ретраит — single attempt per call.
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
            steps: &NoopStepSink,
        };
        let result = run_v2_pipeline(&mock, ctx).await.unwrap().summary_json;
        // Post-pass crashed → original kept.
        assert_eq!(result["action_items"], original_items);
        // classifier (1) + main (1) + post-pass (1) + narrative (1) = 4 calls.
        assert_eq!(mock.call_count(), 4);
    }
}
