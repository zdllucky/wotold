//! [M14 T-06 Phase B] Map-reduce orchestration для длинных transcripts.
//!
//! ## Flow
//!
//! 1. Caller (`local_orchestrator`) предварительно нарезал transcript
//!    в Vec<String> chunks (по speaker boundaries, см. `chunker.rs`).
//! 2. `run_map_reduce` для каждого chunk дёргает LLM с `build_map_prompt`
//!    → JSON `{ chunk_idx, facts, decisions_candidates, action_candidates,
//!    open_questions_candidates, topic_tags, participants_mentioned }`.
//! 3. Все map outputs собираются в JSON array → reduce LLM-call с
//!    `build_reduce_prompt` → финальный `CallSummaryV2` JSON.
//! 4. Caller передаёт JSON в `recap::persist_recap_from_json` — без изменений.
//!
//! ## Resilience
//!
//! Если один map call возвращает Err или garbage JSON — пропускаем его
//! (log warn), reduce работает с остальными. Если ВСЕ map calls fail —
//! `run_map_reduce` возвращает Err: нет данных для reduce.
//!
//! ## Phase C/D/E (deferred)
//!
//! - T-07 8 expert prompts: заменить universal reduce prompt на 8 specialized.
//! - T-08 action-item post-pass: после reduce — отдельный validate-call.
//! - T-09 GBNF constrained decoding.

use crate::pipeline::expert_prompts;
use crate::pipeline::gbnf;
use crate::pipeline::summary_v2::CallType;
use crate::providers::llm::{LlmProvider, LlmRequest};
use crate::AppError;

/// Per-chunk map output: small max_tokens (~1024) — JSON компактнее full v2.
const MAP_MAX_TOKENS: u32 = 1024;
/// Final reduce: full CallSummaryV2 — 4096 tokens достаточно.
const REDUCE_MAX_TOKENS: u32 = 4096;
/// [M14 T-18 P2] Hierarchical pipeline trigger — когда `chunks.len()`
/// больше этого порога, переключаемся на 3-level (map → mid-reduce per
/// group → final reduce). Меньше — flat 2-level path (run_map_reduce).
const HIERARCHICAL_THRESHOLD: usize = 8;
/// [M14 T-18 P2] Group size для mid-reduce — каждая mid-reduce LLM call
/// консолидирует до этого числа map outputs.
const MID_REDUCE_GROUP_SIZE: usize = 4;
/// [M14 T-18 P2] Mid-reduce output — aggregate same shape как map output,
/// 2K tokens достаточно для 4 chunks worth of facts/candidates.
const MID_REDUCE_MAX_TOKENS: u32 = 2048;

/// Map step: classifier + extractor для одного chunk'а transcript'а.
/// Output schema из PRD §5.3.
pub(crate) fn build_map_prompt(lang_detected: Option<&str>, chunk_idx: usize) -> String {
    let lang = lang_detected.unwrap_or("ru");
    format!(
        "You are a meeting analyst processing CHUNK {chunk_idx} of a corporate call transcript (output language: {lang}). This chunk is PART OF a longer call — extract facts + candidates only; final summarization happens later in REDUCE step.\n\
\n\
## RULES\n\
\n\
1. NEVER invent facts, names, dates, numbers, or commitments not present in THIS chunk.\n\
2. `evidence_quote` MUST be a verbatim substring (10-200 chars) copied from THIS chunk text.\n\
3. Action items: only flag explicit accepts ('я возьму', 'I'll take it') as `commitment`; suggested as `proposal`; raised без accept as `idea`.\n\
4. Output ONLY ONE JSON object matching schema below. No prose, no markdown fences.\n\
5. NEVER resolve speaker tags into names в `facts`/topic_tags — REDUCE step делает it. Keep `Speaker 0` / `**name**` references as-is в evidence quotes.\n\
\n\
## OUTPUT SCHEMA\n\
\n\
{{\n\
  \"chunk_idx\": {chunk_idx},\n\
  \"facts\": [string],                              // ≤25 words each, ≤10 items per chunk\n\
  \"decisions_candidates\": [{{\n\
    \"text\": string,\n\
    \"evidence_quote\": string,                     // verbatim 10-200 chars\n\
    \"speaker\": string|null\n\
  }}],\n\
  \"action_candidates\": [{{\n\
    \"text\": string,\n\
    \"owner_hint\": string|null,\n\
    \"due\": string|null,\n\
    \"category\": \"commitment\"|\"proposal\"|\"idea\",\n\
    \"evidence_quote\": string,                     // verbatim\n\
    \"speaker\": string|null\n\
  }}],\n\
  \"open_questions_candidates\": [{{\n\
    \"text\": string,\n\
    \"raised_by\": string|null,\n\
    \"evidence_quote\": string                      // verbatim\n\
  }}],\n\
  \"topic_tags\": [string],                         // 1-3 word tags, ≤5 per chunk\n\
  \"participants_mentioned\": [string]              // distinct human names referenced\n\
}}\n\
\n\
Output ONLY the JSON object. No prose. No markdown fences."
    )
}

/// Reduce step: собирает все map outputs + классификация + known speakers
/// → финальный `CallSummaryV2` JSON (same shape как `recap::build_v2_system_prompt`).
pub(crate) fn build_reduce_prompt(
    lang_detected: Option<&str>,
    known_call_type: Option<CallType>,
    known_speakers: Option<&str>,
    map_outputs_json: &str,
) -> String {
    let lang = lang_detected.unwrap_or("ru");
    let known_block = known_speakers
        .map(|s| format!("\n\n## Known participants\n{s}"))
        .unwrap_or_default();
    let type_hint = known_call_type
        .map(|t| {
            format!(
                "\n\n## Classification hint (pre-determined)\nCall type already classified as `{}`. Set `call_type` to this value, use the matching TYPE GUIDE section, и populate `type_specific_block` соответственно.",
                t.as_str()
            )
        })
        .unwrap_or_default();
    format!(
        "You are a senior meeting analyst для REDUCE step of a long call. You receive a JSON ARRAY of per-chunk MAP outputs (facts, decisions_candidates, action_candidates, open_questions_candidates, topic_tags, participants_mentioned). Your job: consolidate into ONE final `CallSummaryV2` JSON.\n\
\n\
Output language: {lang}.\n\
\n\
## ABSOLUTE RULES\n\
\n\
1. NEVER invent facts not present в MAP_OUTPUTS — only consolidate, dedupe, resolve speakers.\n\
2. `decisions` / `open_questions` / `action_items` SHOULD keep `evidence.quote` verbatim from corresponding chunk MAP output's `evidence_quote`.\n\
3. Resolve speakers via Known participants block если присутствует; иначе оставь `**name**` или generic role.\n\
4. Output ONLY ONE JSON object matching CallSummaryV2 schema. No prose, no markdown fences.\n\
5. Dedupe: identical action_items от двух разных chunks (overlap'нутый turn) — keep one, prefer the one с более точным owner_hint.\n\
\n\
## OUTPUT SCHEMA (CallSummaryV2 strict)\n\
\n\
{{\n\
  \"schema_version\": 2,\n\
  \"title\": string,\n\
  \"summary\": string,\n\
  \"key_points\": string[],\n\
  \"language\": \"ru\" | \"en\" | \"kk\" | \"mixed\",\n\
  \"call_type\": one of: sales_discovery, sales_demo, product_sync, standup, customer_interview, one_on_one, strategy_brainstorm, status_update, other,\n\
  \"call_type_confidence\": number 0..1,\n\
  \"participants\": [{{ \"speaker_tag\": string, \"display_name\": string|null, \"role_hint\": string|null }}],\n\
  \"action_items\": [{{\n\
    \"id\": string,\n\
    \"text\": string,\n\
    \"owner_hint\": string|null,\n\
    \"owner_confidence\": number 0..1,\n\
    \"due\": string|null,\n\
    \"due_confidence\": number 0..1,\n\
    \"category\": \"commitment\"|\"proposal\"|\"idea\",\n\
    \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }}\n\
  }}],\n\
  \"decisions\": [{{ \"id\": string, \"text\": string, \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }}, \"confidence\": number 0..1 }}],\n\
  \"open_questions\": [{{ \"id\": string, \"text\": string, \"raised_by\": string|null, \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }} }}],\n\
  \"mom\": string (Markdown),\n\
  \"type_specific_block\": object|null\n\
}}{known_block}{type_hint}\n\
\n\
## MAP_OUTPUTS (input — array of per-chunk extractions)\n\
\n\
{map_outputs_json}\n\
\n\
Output ONLY the final CallSummaryV2 JSON object. No prose."
    )
}

/// Run map → reduce pipeline. Каждый map call best-effort: на ошибку пропускаем.
/// Если все map calls fail — возвращаем AppError.
pub(crate) async fn run_map_reduce(
    provider: &dyn LlmProvider,
    chunks: &[String],
    lang_detected: Option<&str>,
    known_call_type: Option<CallType>,
    known_speakers: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    // 1. Map step: per-chunk LLM call.
    let mut map_outputs: Vec<serde_json::Value> = Vec::with_capacity(chunks.len());
    for (idx, chunk) in chunks.iter().enumerate() {
        let request = LlmRequest {
            model: None,
            system: build_map_prompt(lang_detected, idx),
            input: chunk.clone(),
            max_tokens: Some(MAP_MAX_TOKENS),
            grammar: None,
        };
        match gbnf::generate_with_grammar_fallback(provider, request).await {
            Ok(json_value) => map_outputs.push(json_value),
            Err(e) => {
                log::warn!("map step chunk {idx} failed (skipping): {e}");
            }
        }
    }
    if map_outputs.is_empty() {
        return Err(AppError::Other(
            "map-reduce: all map calls failed, nothing to reduce".into(),
        ));
    }

    // 2. Reduce step: consolidate map outputs.
    let map_outputs_json = serde_json::to_string(&map_outputs)
        .map_err(|e| AppError::Other(format!("map-reduce: serialize map outputs: {e}")))?;
    // [M14 T-07 Phase C] Expert reduce prompt когда classifier дал call_type;
    // universal fallback на classifier failure.
    let reduce_system = match known_call_type {
        Some(t) => expert_prompts::build_expert_reduce_prompt(
            t,
            lang_detected,
            known_speakers,
            &map_outputs_json,
        ),
        None => build_reduce_prompt(lang_detected, None, known_speakers, &map_outputs_json),
    };
    let request = LlmRequest {
        model: None,
        system: reduce_system,
        // Reduce использует консолидированные MAP_OUTPUTS из system prompt'а —
        // input не нужен. Пустая строка сохраняет LlmRequest invariant.
        input: String::new(),
        max_tokens: Some(REDUCE_MAX_TOKENS),
        grammar: None,
    };
    gbnf::generate_with_grammar_fallback(provider, request)
        .await
        .map_err(|e| AppError::Other(format!("reduce llm: {e}")))
}

// ── [M14 T-18 P2] Hierarchical 3-level pipeline ──────────────────────────

/// Mid-reduce prompt: aggregate K map outputs в single intermediate JSON
/// matching map output schema. Используется когда `chunks.len()` превышает
/// `HIERARCHICAL_THRESHOLD` — level 2 между per-chunk map и final reduce.
pub(crate) fn build_mid_reduce_prompt(lang_detected: Option<&str>, group_idx: usize) -> String {
    let lang = lang_detected.unwrap_or("ru");
    format!(
        "You are a meeting analyst consolidating GROUP {group_idx} of partial chunk-level extractions from a long corporate call. Output language: {lang}.\n\
\n\
## YOUR JOB\n\
\n\
You receive an ARRAY of per-chunk MAP outputs (facts, decisions_candidates, action_candidates, open_questions_candidates, topic_tags, participants_mentioned). Consolidate into ONE aggregate JSON of the SAME shape — dedup overlapping items, prefer items с stronger evidence_quote, keep all distinct findings.\n\
\n\
## RULES\n\
\n\
1. NEVER invent facts not present в input MAP_OUTPUTS — only merge / dedup / pick best.\n\
2. evidence_quote remains verbatim from source chunks.\n\
3. Output schema is INTERMEDIATE — final CallSummaryV2 will be produced by LEVEL 3 reduce. Do NOT generate title / summary / mom / call_type.\n\
4. Output ONLY ONE JSON object matching schema below. No prose, no markdown fences.\n\
\n\
## OUTPUT SCHEMA\n\
\n\
{{\n\
  \"group_idx\": {group_idx},\n\
  \"facts\": [string],\n\
  \"decisions_candidates\": [{{ \"text\": string, \"evidence_quote\": string, \"speaker\": string|null }}],\n\
  \"action_candidates\": [{{\n\
    \"text\": string,\n\
    \"owner_hint\": string|null,\n\
    \"due\": string|null,\n\
    \"category\": \"commitment\"|\"proposal\"|\"idea\",\n\
    \"evidence_quote\": string,\n\
    \"speaker\": string|null\n\
  }}],\n\
  \"open_questions_candidates\": [{{ \"text\": string, \"raised_by\": string|null, \"evidence_quote\": string }}],\n\
  \"topic_tags\": [string],\n\
  \"participants_mentioned\": [string]\n\
}}\n\
\n\
Output ONLY the JSON object."
    )
}

/// Single mid-reduce LLM call для одной группы map outputs (level 2).
async fn run_single_mid_reduce(
    provider: &dyn LlmProvider,
    group_outputs: &[serde_json::Value],
    lang_detected: Option<&str>,
    group_idx: usize,
) -> Result<serde_json::Value, AppError> {
    let group_json = serde_json::to_string(group_outputs)
        .map_err(|e| AppError::Other(format!("mid-reduce serialize: {e}")))?;
    let request = LlmRequest {
        model: None,
        system: build_mid_reduce_prompt(lang_detected, group_idx),
        input: group_json,
        max_tokens: Some(MID_REDUCE_MAX_TOKENS),
        grammar: None,
    };
    gbnf::generate_with_grammar_fallback(provider, request)
        .await
        .map_err(|e| AppError::Other(format!("mid-reduce llm: {e}")))
}

/// [M14 T-18 P2] Hierarchical entry point. Dispatch'ит:
/// - `chunks.len() <= HIERARCHICAL_THRESHOLD` → flat 2-level (`run_map_reduce`)
/// - `> HIERARCHICAL_THRESHOLD` → 3-level (map → mid-reduce per group → final reduce)
///
/// Best-effort: failed map / mid-reduce calls пропускаются. Если ВСЕ map
/// или ВСЕ mid-reduce calls fail → AppError.
pub(crate) async fn run_pipeline(
    provider: &dyn LlmProvider,
    chunks: &[String],
    lang_detected: Option<&str>,
    known_call_type: Option<CallType>,
    known_speakers: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    if chunks.len() <= HIERARCHICAL_THRESHOLD {
        return run_map_reduce(
            provider,
            chunks,
            lang_detected,
            known_call_type,
            known_speakers,
        )
        .await;
    }

    log::info!(
        "hierarchical pipeline: {} chunks → {} groups (group_size={})",
        chunks.len(),
        chunks.len().div_ceil(MID_REDUCE_GROUP_SIZE),
        MID_REDUCE_GROUP_SIZE
    );

    // 1. Map step (level 1) — per-chunk extraction.
    let mut map_outputs: Vec<serde_json::Value> = Vec::with_capacity(chunks.len());
    for (idx, chunk) in chunks.iter().enumerate() {
        let request = LlmRequest {
            model: None,
            system: build_map_prompt(lang_detected, idx),
            input: chunk.clone(),
            max_tokens: Some(MAP_MAX_TOKENS),
            grammar: None,
        };
        match gbnf::generate_with_grammar_fallback(provider, request).await {
            Ok(json_value) => map_outputs.push(json_value),
            Err(e) => log::warn!("hierarchical map chunk {idx} failed (skipping): {e}"),
        }
    }
    if map_outputs.is_empty() {
        return Err(AppError::Other("hierarchical: all map calls failed".into()));
    }

    // 2. Mid-reduce step (level 2) — group map outputs.
    let mut mid_aggregates: Vec<serde_json::Value> = Vec::new();
    for (group_idx, group) in map_outputs.chunks(MID_REDUCE_GROUP_SIZE).enumerate() {
        match run_single_mid_reduce(provider, group, lang_detected, group_idx).await {
            Ok(agg) => mid_aggregates.push(agg),
            Err(e) => log::warn!("mid-reduce group {group_idx} failed (skipping): {e}"),
        }
    }
    if mid_aggregates.is_empty() {
        return Err(AppError::Other(
            "hierarchical: all mid-reduce calls failed".into(),
        ));
    }

    // 3. Final reduce step (level 3) — consolidate mid-aggregates → CallSummaryV2.
    let mid_aggregates_json = serde_json::to_string(&mid_aggregates)
        .map_err(|e| AppError::Other(format!("hierarchical: serialize mid-aggregates: {e}")))?;
    let reduce_system = match known_call_type {
        Some(t) => expert_prompts::build_expert_reduce_prompt(
            t,
            lang_detected,
            known_speakers,
            &mid_aggregates_json,
        ),
        None => build_reduce_prompt(lang_detected, None, known_speakers, &mid_aggregates_json),
    };
    let request = LlmRequest {
        model: None,
        system: reduce_system,
        input: String::new(),
        max_tokens: Some(REDUCE_MAX_TOKENS),
        grammar: None,
    };
    gbnf::generate_with_grammar_fallback(provider, request)
        .await
        .map_err(|e| AppError::Other(format!("hierarchical final reduce llm: {e}")))
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
            "title": "reduced",
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

    fn minimal_map_json(idx: usize) -> serde_json::Value {
        serde_json::json!({
            "chunk_idx": idx,
            "facts": [format!("fact from chunk {idx}")],
            "decisions_candidates": [],
            "action_candidates": [],
            "open_questions_candidates": [],
            "topic_tags": [],
            "participants_mentioned": [],
        })
    }

    #[test]
    fn build_map_prompt_includes_required_fields() {
        let p = build_map_prompt(Some("ru"), 3);
        assert!(p.contains("\"chunk_idx\": 3"));
        assert!(p.contains("facts"));
        assert!(p.contains("decisions_candidates"));
        assert!(p.contains("action_candidates"));
        assert!(p.contains("open_questions_candidates"));
        assert!(p.contains("topic_tags"));
        assert!(p.contains("participants_mentioned"));
        assert!(p.contains("Output ONLY the JSON object"));
    }

    #[test]
    fn build_reduce_prompt_embeds_map_outputs() {
        let map_outputs = serde_json::json!([
            { "chunk_idx": 0, "facts": ["a"] },
            { "chunk_idx": 1, "facts": ["b"] }
        ]);
        let json_str = serde_json::to_string(&map_outputs).unwrap();
        let p = build_reduce_prompt(Some("ru"), None, None, &json_str);
        assert!(p.contains(&json_str));
        assert!(p.contains("CallSummaryV2"));
        assert!(p.contains("schema_version"));
    }

    #[test]
    fn build_reduce_prompt_includes_call_type_hint_when_present() {
        let p_with = build_reduce_prompt(Some("ru"), Some(CallType::Standup), None, "[]");
        assert!(p_with.contains("Classification hint"));
        assert!(p_with.contains("`standup`"));

        let p_without = build_reduce_prompt(Some("ru"), None, None, "[]");
        assert!(!p_without.contains("Classification hint"));
    }

    #[tokio::test]
    async fn run_map_reduce_uses_expert_reduce_when_known_type() {
        // known_call_type=Some(Standup) → reduce prompt = expert (SPECIALIZED GUIDE).
        let mock = MockProvider::new(vec![Ok(minimal_map_json(0)), Ok(minimal_v2_json())]);
        let chunks = vec!["**A** [0:00]:\nhi".to_string()];
        run_map_reduce(&mock, &chunks, Some("en"), Some(CallType::Standup), None)
            .await
            .unwrap();
        let systems = mock.captured_systems();
        // [0]=map, [1]=reduce.
        assert_eq!(systems.len(), 2);
        let reduce = &systems[1];
        assert!(
            reduce.contains("SPECIALIZED GUIDE"),
            "expert reduce missing focused guide marker"
        );
        assert!(reduce.contains("`standup`"));
        // Other types' specialized headers absent.
        assert!(!reduce.contains("## Customer pain"));
        assert!(!reduce.contains("## Demo flow"));
    }

    #[tokio::test]
    async fn run_map_reduce_uses_universal_reduce_when_no_type() {
        // known_call_type=None → universal reduce (Classification hint absent
        // because hint только при Some, тоже).
        let mock = MockProvider::new(vec![Ok(minimal_map_json(0)), Ok(minimal_v2_json())]);
        let chunks = vec!["**A** [0:00]:\nhi".to_string()];
        run_map_reduce(&mock, &chunks, None, None, None)
            .await
            .unwrap();
        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 2);
        let reduce = &systems[1];
        // Universal reduce НЕ имеет SPECIALIZED GUIDE marker.
        assert!(
            !reduce.contains("SPECIALIZED GUIDE"),
            "universal reduce should NOT contain expert marker"
        );
        // Должно быть упоминание CallSummaryV2 (universal mom-агрегация).
        assert!(reduce.contains("CallSummaryV2"));
    }

    #[tokio::test]
    async fn run_map_reduce_two_chunks_then_reduce_success() {
        let mock = MockProvider::new(vec![
            Ok(minimal_map_json(0)),
            Ok(minimal_map_json(1)),
            Ok(minimal_v2_json()),
        ]);
        let chunks = vec![
            "**A** [0:00]:\nhi".to_string(),
            "**B** [1:00]:\nbye".to_string(),
        ];
        let result = run_map_reduce(&mock, &chunks, Some("ru"), Some(CallType::Standup), None)
            .await
            .unwrap();
        assert_eq!(result["title"], "reduced");

        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 3, "expected 2 map + 1 reduce");
        // 3rd call = reduce, must include map outputs JSON.
        assert!(systems[2].contains("MAP_OUTPUTS"));
        assert!(systems[2].contains("fact from chunk 0"));
        assert!(systems[2].contains("fact from chunk 1"));
    }

    #[tokio::test]
    async fn run_map_reduce_skips_failed_map_continues_reduce() {
        // [M14 T-09 Phase E] gbnf wrapper retries failing map call — need
        // 2 Err для chunk 1 (initial + grammar retry).
        let mock = MockProvider::new(vec![
            Ok(minimal_map_json(0)),
            Err(LlmError::Provider("simulated map crash 1".into())),
            Err(LlmError::Provider(
                "simulated map crash 2 (grammar retry)".into(),
            )),
            Ok(minimal_map_json(2)),
            Ok(minimal_v2_json()),
        ]);
        let chunks = vec!["c0".to_string(), "c1".to_string(), "c2".to_string()];
        let result = run_map_reduce(&mock, &chunks, None, None, None)
            .await
            .unwrap();
        assert_eq!(result["schema_version"], 2);

        let systems = mock.captured_systems();
        // chunk0 (1) + chunk1 attempts (2) + chunk2 (1) + reduce (1) = 5.
        assert_eq!(systems.len(), 5, "expected 5 calls");
        // Reduce — последний, должен видеть только 2 map outputs (idx 0 и 2).
        let reduce_prompt = systems.last().unwrap();
        assert!(reduce_prompt.contains("fact from chunk 0"));
        assert!(reduce_prompt.contains("fact from chunk 2"));
        assert!(!reduce_prompt.contains("fact from chunk 1"));
    }

    #[tokio::test]
    async fn run_map_reduce_all_maps_fail_returns_error() {
        // [M14 T-09 Phase E] gbnf wrapper retries each map — need 4 Err total
        // для 2 chunks × 2 attempts.
        let mock = MockProvider::new(vec![
            Err(LlmError::Provider("c0 attempt 1".into())),
            Err(LlmError::Provider("c0 attempt 2 (grammar)".into())),
            Err(LlmError::Provider("c1 attempt 1".into())),
            Err(LlmError::Provider("c1 attempt 2 (grammar)".into())),
        ]);
        let chunks = vec!["c0".to_string(), "c1".to_string()];
        let err = run_map_reduce(&mock, &chunks, None, None, None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("all map calls failed"),
            "expected all-fail error, got: {err}"
        );
    }

    // ── [M14 T-18 P2] Hierarchical pipeline tests ────────────────────────

    #[test]
    fn build_mid_reduce_prompt_includes_required_schema_fields() {
        let p = build_mid_reduce_prompt(Some("en"), 2);
        assert!(p.contains("group_idx"));
        assert!(p.contains("facts"));
        assert!(p.contains("decisions_candidates"));
        assert!(p.contains("action_candidates"));
        assert!(p.contains("open_questions_candidates"));
        assert!(p.contains("topic_tags"));
        assert!(p.contains("participants_mentioned"));
        // Mid-reduce does NOT generate final v2 fields — make sure prompt says so.
        assert!(p.contains("INTERMEDIATE"));
        assert!(p.contains("LEVEL 3"));
    }

    #[tokio::test]
    async fn run_pipeline_short_input_delegates_to_flat_map_reduce() {
        // 3 chunks ≤ HIERARCHICAL_THRESHOLD (8) → flat path: 3 map + 1 reduce = 4 calls.
        let mock = MockProvider::new(vec![
            Ok(minimal_map_json(0)),
            Ok(minimal_map_json(1)),
            Ok(minimal_map_json(2)),
            Ok(minimal_v2_json()),
        ]);
        let chunks = vec!["c0".into(), "c1".into(), "c2".into()];
        let result = run_pipeline(&mock, &chunks, None, None, None)
            .await
            .unwrap();
        assert_eq!(result["schema_version"], 2);
        assert_eq!(mock.captured_systems().len(), 4);
        // Last call — final reduce, MAP_OUTPUTS marker.
        let systems = mock.captured_systems();
        assert!(systems.last().unwrap().contains("MAP_OUTPUTS"));
    }

    #[tokio::test]
    async fn run_pipeline_long_input_uses_hierarchical() {
        // 9 chunks > HIERARCHICAL_THRESHOLD (8) → hierarchical:
        // 9 maps + ceil(9/4)=3 mid-reduces + 1 final = 13 calls.
        let mut responses: Vec<Result<serde_json::Value, LlmError>> = Vec::new();
        for i in 0..9 {
            responses.push(Ok(minimal_map_json(i)));
        }
        // 3 mid-reduce outputs (same shape as map output).
        for i in 0..3 {
            responses.push(Ok(serde_json::json!({
                "group_idx": i,
                "facts": [format!("agg fact group {i}")],
                "decisions_candidates": [],
                "action_candidates": [],
                "open_questions_candidates": [],
                "topic_tags": [],
                "participants_mentioned": [],
            })));
        }
        // Final reduce → v2.
        responses.push(Ok(minimal_v2_json()));
        let mock = MockProvider::new(responses);
        let chunks: Vec<String> = (0..9).map(|i| format!("chunk-{i}")).collect();
        let result = run_pipeline(&mock, &chunks, None, None, None)
            .await
            .unwrap();
        assert_eq!(result["schema_version"], 2);

        let systems = mock.captured_systems();
        assert_eq!(
            systems.len(),
            13,
            "expected 13 LLM calls, got {}",
            systems.len()
        );
        // Sanity: middle calls (idx 9, 10, 11) — mid-reduce (SPECIALIZED GUIDE absent, intermediate marker).
        assert!(systems[9].contains("INTERMEDIATE"));
        // Last call — final reduce с MAP_OUTPUTS (universal reduce path since known_call_type=None).
        assert!(systems[12].contains("MAP_OUTPUTS"));
        // Final reduce sees mid-aggregate facts, не raw chunk facts.
        assert!(systems[12].contains("agg fact group"));
    }

    #[tokio::test]
    async fn run_pipeline_mid_reduce_failure_skips_group() {
        // 9 chunks → 3 mid-reduce groups. Middle group fails → final reduce
        // works with 2 mid-aggregates.
        let mut responses: Vec<Result<serde_json::Value, LlmError>> = Vec::new();
        for i in 0..9 {
            responses.push(Ok(minimal_map_json(i)));
        }
        responses.push(Ok(serde_json::json!({
            "group_idx": 0, "facts": ["g0"],
            "decisions_candidates": [], "action_candidates": [],
            "open_questions_candidates": [], "topic_tags": [], "participants_mentioned": []
        })));
        responses.push(Err(LlmError::Provider("mid-reduce crash".into())));
        responses.push(Err(LlmError::Provider(
            "mid-reduce crash grammar retry".into(),
        )));
        responses.push(Ok(serde_json::json!({
            "group_idx": 2, "facts": ["g2"],
            "decisions_candidates": [], "action_candidates": [],
            "open_questions_candidates": [], "topic_tags": [], "participants_mentioned": []
        })));
        responses.push(Ok(minimal_v2_json()));
        let mock = MockProvider::new(responses);
        let chunks: Vec<String> = (0..9).map(|i| format!("chunk-{i}")).collect();
        let result = run_pipeline(&mock, &chunks, None, None, None)
            .await
            .unwrap();
        assert_eq!(result["schema_version"], 2);

        let systems = mock.captured_systems();
        // Final reduce должно содержать только g0 + g2 (g1 dropped).
        let final_reduce = systems.last().unwrap();
        assert!(final_reduce.contains("\"facts\":[\"g0\"]") || final_reduce.contains("g0"));
        assert!(final_reduce.contains("g2"));
    }

    #[tokio::test]
    async fn run_pipeline_all_maps_fail_returns_error() {
        // 9 chunks, все map calls fail (с grammar retry — 2 attempts each = 18 errs).
        let mut responses: Vec<Result<serde_json::Value, LlmError>> = Vec::new();
        for i in 0..18 {
            responses.push(Err(LlmError::Provider(format!("crash {i}"))));
        }
        let mock = MockProvider::new(responses);
        let chunks: Vec<String> = (0..9).map(|i| format!("c{i}")).collect();
        let err = run_pipeline(&mock, &chunks, None, None, None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("all map calls failed"),
            "expected all-maps-fail error, got: {err}"
        );
    }

    #[tokio::test]
    async fn run_pipeline_all_mid_reduces_fail_returns_error() {
        // 9 maps succeed, все mid-reduce fail → AppError.
        let mut responses: Vec<Result<serde_json::Value, LlmError>> = Vec::new();
        for i in 0..9 {
            responses.push(Ok(minimal_map_json(i)));
        }
        // 3 groups × 2 attempts (retry) = 6 errors.
        for i in 0..6 {
            responses.push(Err(LlmError::Provider(format!("mid-reduce crash {i}"))));
        }
        let mock = MockProvider::new(responses);
        let chunks: Vec<String> = (0..9).map(|i| format!("c{i}")).collect();
        let err = run_pipeline(&mock, &chunks, None, None, None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("all mid-reduce calls failed"),
            "expected all-mid-fail error, got: {err}"
        );
    }
}
