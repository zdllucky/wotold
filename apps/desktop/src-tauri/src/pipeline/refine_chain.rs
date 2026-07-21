//! [F1] Refine-чейн для длинных транскриптов (заменил map-reduce).
//!
//! ## Flow
//!
//! 1. Chunk 0 → обычный one-shot v2/expert промпт (+ приписка «PART 1 of N»)
//!    → первичный CallSummaryV2 JSON.
//! 2. Chunk i (i≥1) → refine-промпт: CURRENT_RECAP (компактированный JSON
//!    текущего состояния) + новый фрагмент. Инструкция — «расширь и поправь,
//!    не регенерируй с нуля, не дропай, не дублируй». Ответ (полный
//!    CallSummaryV2) **заменяет** состояние.
//! 3. Финальный JSON → caller (post-pass, narrative, persist) — как раньше.
//!
//! ## Почему последовательно, а не map-reduce
//!
//! Map-шаги были контекстно-слепы: чанк N не знал, что чанк N-1 уже нашёл
//! то же решение с лучшей формулировкой; reduce получал плоские огрызки.
//! Refine-чейн несёт накопленный рекап через весь звонок — дедуп и
//! непрерывность контекста происходят прямо в модели.
//!
//! ## Token-бюджет (ctx 8192, все presets)
//!
//! chunk (≤2600 est) + injected recap (≤1400 est) + инструкция (~1000)
//! + вывод (2560) + служебные ≤ 8192. См. const assert ниже.
//!
//! ## Resilience
//!
//! Ретрай 1 раз (aggressive-компакция инъекции — меньше промпт, больше
//! шанс на полный JSON), затем skip чанка — состояние сохраняется, чейн
//! продолжает. Упал chunk 0 — следующий чанк становится initial. Все чанки
//! упали → Err. ≥50% потеряно → WARN DEGRADED (parity с map-reduce A2).

use crate::pipeline::chunker;
use crate::pipeline::expert_prompts;
use crate::pipeline::gbnf;
use crate::pipeline::llm_schemas;
use crate::pipeline::recap;
use crate::pipeline::recap_steps::{preview_from_summary, step_event, RecapStepSink};
use crate::pipeline::summary_v2::CallType;
use crate::providers::llm::{LlmError, LlmProvider, LlmRequest};
use crate::AppError;

/// Вывод одного refine-шага — полный CallSummaryV2 (parity со старым REDUCE).
const REFINE_MAX_TOKENS: u32 = 2560;
/// Бюджет инъекции текущего рекапа в промпт (est-токены, `estimate_tokens`).
const INJECTED_RECAP_MAX_TOKENS: usize = 1_400;
/// Aggressive-бюджет для ретрая — меньше инъекция, больше запас на вывод.
const INJECTED_RECAP_MAX_TOKENS_AGGRESSIVE: usize = 900;
/// Инструкция refine-промпта (rules + schema + guide), est-токены, с запасом.
const REFINE_INSTRUCTION_TOKENS: usize = 1_000;

const _: () = assert!(
    chunker::REFINE_MAX_TOKENS_PER_CHUNK
        + INJECTED_RECAP_MAX_TOKENS
        + REFINE_INSTRUCTION_TOKENS
        + REFINE_MAX_TOKENS as usize
        + 64
        <= 8_192
);

/// Результат чейна: финальный JSON + счётчики для телеметрии/warn'ов.
#[derive(Debug)]
pub(crate) struct RefineOutcome {
    pub summary_json: serde_json::Value,
    pub chunks_total: usize,
    pub chunks_failed: usize,
}

/// Компактирует CallSummaryV2 JSON для инъекции в refine-промпт.
/// Детерминированная лестница (останавливается как только влезло):
/// 1) `narrative`/`mom` выкидываются всегда (в чейне не нужны);
/// 2) evidence quotes → усечь до 100 chars;
/// 3) evidence выкинуть целиком;
/// 4) capы массивов (key_points ≤12, action_items ≤12, decisions ≤8,
///    open_questions ≤8, topics ≤5 × points ≤3);
/// 5) summary → ~600 chars.
///
/// `aggressive=true` — сразу все ступени + жёстче capы (≤8/≤8/≤6/≤6,
/// topics ≤3×2, summary ~400) — для ретрая после обрезанного ответа.
pub(crate) fn compact_recap_for_injection(
    summary: &serde_json::Value,
    max_tokens: usize,
    aggressive: bool,
) -> String {
    let mut s = summary.clone();
    if let Some(obj) = s.as_object_mut() {
        obj.remove("narrative");
        obj.remove("mom");
    }

    let fits = |v: &serde_json::Value| {
        serde_json::to_string(v)
            .map(|t| chunker::estimate_tokens(&t) <= max_tokens)
            .unwrap_or(false)
    };

    if !aggressive && fits(&s) {
        return serde_json::to_string(&s).unwrap_or_default();
    }

    // Ступень 2: усечь quotes.
    truncate_evidence_quotes(&mut s, 100);
    if !aggressive && fits(&s) {
        return serde_json::to_string(&s).unwrap_or_default();
    }

    // Ступень 3: выкинуть evidence.
    strip_evidence(&mut s);
    if !aggressive && fits(&s) {
        return serde_json::to_string(&s).unwrap_or_default();
    }

    // Ступень 4: capы массивов.
    let (kp, ai, dec, oq, topics, points) = if aggressive {
        (8, 8, 6, 6, 3, 2)
    } else {
        (12, 12, 8, 8, 5, 3)
    };
    cap_array(&mut s, "key_points", kp);
    cap_array(&mut s, "action_items", ai);
    cap_array(&mut s, "decisions", dec);
    cap_array(&mut s, "open_questions", oq);
    cap_topics(&mut s, topics, points);
    if !aggressive && fits(&s) {
        return serde_json::to_string(&s).unwrap_or_default();
    }

    // Ступень 5: усечь summary.
    let summary_cap = if aggressive { 400 } else { 600 };
    if let Some(text) = s.get("summary").and_then(|v| v.as_str()) {
        if text.chars().count() > summary_cap {
            let cut: String = text.chars().take(summary_cap).collect();
            s["summary"] = serde_json::Value::String(format!("{cut}…"));
        }
    }
    serde_json::to_string(&s).unwrap_or_default()
}

fn truncate_evidence_quotes(s: &mut serde_json::Value, max_chars: usize) {
    for key in ["action_items", "decisions", "open_questions"] {
        if let Some(items) = s.get_mut(key).and_then(|v| v.as_array_mut()) {
            for item in items {
                if let Some(quote) = item
                    .get_mut("evidence")
                    .and_then(|e| e.get_mut("quote"))
                    .filter(|q| q.is_string())
                {
                    let text = quote.as_str().unwrap_or_default();
                    if text.chars().count() > max_chars {
                        let cut: String = text.chars().take(max_chars).collect();
                        *quote = serde_json::Value::String(cut);
                    }
                }
            }
        }
    }
}

fn strip_evidence(s: &mut serde_json::Value) {
    for key in ["action_items", "decisions", "open_questions"] {
        if let Some(items) = s.get_mut(key).and_then(|v| v.as_array_mut()) {
            for item in items {
                if let Some(obj) = item.as_object_mut() {
                    obj.remove("evidence");
                }
            }
        }
    }
}

fn cap_array(s: &mut serde_json::Value, key: &str, max: usize) {
    if let Some(arr) = s.get_mut(key).and_then(|v| v.as_array_mut()) {
        arr.truncate(max);
    }
}

fn cap_topics(s: &mut serde_json::Value, max_topics: usize, max_points: usize) {
    if let Some(arr) = s.get_mut("topics").and_then(|v| v.as_array_mut()) {
        arr.truncate(max_topics);
        for topic in arr {
            if let Some(points) = topic.get_mut("points").and_then(|v| v.as_array_mut()) {
                points.truncate(max_points);
            }
        }
    }
}

/// Refine-промпт для chunk_no (1-based, ≥2): CURRENT_RECAP + новая часть.
pub(crate) fn build_refine_prompt(
    lang_detected: Option<&str>,
    known_call_type: Option<CallType>,
    known_speakers: Option<&str>,
    chunk_no: usize,
    chunk_total: usize,
    current_recap_json: &str,
) -> String {
    let lang = lang_detected.unwrap_or("ru");
    let known_block = known_speakers
        .map(|s| format!("\n\n## Known participants\n\n{s}"))
        .unwrap_or_default();
    let (role, guide, type_pin) = match known_call_type {
        Some(t) => (
            expert_prompts::type_role_hint(t),
            format!("\n\n{}", expert_prompts::type_guide_block(t)),
            format!(" Set `call_type` to `{}`.", t.as_str()),
        ),
        None => ("corporate calls", String::new(), String::new()),
    };
    format!(
        "OUTPUT LANGUAGE = {lang}. EVERY string value MUST be written in {lang} (only call_type/category enums stay English).\n\
\n\
You are a senior meeting analyst specialized in {role}. You maintain a RUNNING RECAP of one long call.\n\
You receive:\n\
(a) CURRENT_RECAP — full CallSummaryV2 JSON with everything known from parts 1..{prev} of the call;\n\
(b) TRANSCRIPT PART {chunk_no} of {chunk_total} (the input below) — the next fragment of the SAME call.\n\
\n\
## YOUR JOB\n\
\n\
EXTEND AND EDIT the current recap with NEW information from this part. Do NOT regenerate from scratch. Do NOT drop items already in CURRENT_RECAP unless this part explicitly contradicts or supersedes them. Do NOT duplicate items that are already captured — merge instead (prefer the more precise wording / owner_hint).\n\
\n\
## ABSOLUTE RULES\n\
\n\
1. NEVER invent facts, names, dates, numbers, or commitments not present in CURRENT_RECAP or THIS transcript part.\n\
2. For NEW decisions/action_items/open_questions from this part add `evidence.quote` = verbatim substring (10-200 chars) from THIS part when you can copy one; otherwise set quote to null but KEEP the item. Existing items keep their evidence as-is (null allowed).\n\
3. Update `summary`/`key_points`/`topics` so they cover the WHOLE call so far, not just this part.\n\
4. Known participants appear under their real names in transcript headers (`**Alice** [MM:SS]:`); identical names = the SAME person. NEVER use raw 'Speaker 0'/'owner' tags in output text.\n\
5. Output ONLY ONE JSON object — the full updated CallSummaryV2 (same schema as CURRENT_RECAP, schema_version 2). No prose, no markdown fences.{type_pin}{guide}{known_block}\n\
\n\
## CURRENT_RECAP\n\
\n\
{current_recap_json}",
        prev = chunk_no - 1,
    )
}

/// System-промпт для initial-шага чейна: обычный one-shot v2/expert +
/// приписка про «часть 1 из N».
fn build_initial_prompt(
    lang_detected: Option<&str>,
    known_call_type: Option<CallType>,
    known_speakers: Option<&str>,
    part_no: usize,
    chunk_total: usize,
) -> String {
    let base = match known_call_type {
        Some(t) => expert_prompts::build_expert_system_prompt(t, lang_detected, known_speakers),
        None => recap::build_v2_system_prompt(lang_detected, known_speakers, None),
    };
    format!(
        "{base}\n\nNOTE: The input is PART {part_no} of {chunk_total} of a longer call transcript. Summarize ONLY what is present here; later parts will EXTEND this recap."
    )
}

/// Валидность состояния чейна: объект с schema_version=2 и непустым title.
/// Схема-констрейн обычно гарантирует форму, но обрезанный/пустой ответ
/// не должен затирать накопленное состояние.
fn is_valid_state(json: &serde_json::Value) -> bool {
    json.get("schema_version").and_then(|v| v.as_u64()) == Some(2)
        && json
            .get("title")
            .and_then(|v| v.as_str())
            .is_some_and(|t| !t.trim().is_empty())
}

/// Последовательный refine-чейн по чанкам. `step_offset` — step_idx первого
/// чанка в общей нумерации шагов оркестратора; `total_steps` — общий план.
#[allow(clippy::too_many_arguments)] // internal chain runner; structured args не окупаются
pub(crate) async fn run_refine_chain(
    provider: &dyn LlmProvider,
    chunks: &[String],
    lang_detected: Option<&str>,
    known_call_type: Option<CallType>,
    known_speakers: Option<&str>,
    steps: &dyn RecapStepSink,
    step_offset: u32,
    total_steps: u32,
) -> Result<RefineOutcome, AppError> {
    let n = chunks.len();
    let mut state: Option<serde_json::Value> = None;
    let mut failed = 0usize;

    for (i, chunk) in chunks.iter().enumerate() {
        let step_idx = step_offset + i as u32;
        let mut ev = step_event(step_idx, total_steps, "refine", "started");
        ev.chunk_no = Some(i as u32 + 1);
        ev.chunk_total = Some(n as u32);
        steps.emit(ev);

        let mut result: Result<serde_json::Value, LlmError> =
            Err(LlmError::Provider("unattempted".into()));
        for attempt in 0..2 {
            let system = match &state {
                // Initial-шаг: первый чанк ИЛИ все предыдущие упали.
                None => {
                    build_initial_prompt(lang_detected, known_call_type, known_speakers, i + 1, n)
                }
                Some(current) => {
                    let injected = compact_recap_for_injection(
                        current,
                        if attempt == 0 {
                            INJECTED_RECAP_MAX_TOKENS
                        } else {
                            INJECTED_RECAP_MAX_TOKENS_AGGRESSIVE
                        },
                        attempt > 0,
                    );
                    build_refine_prompt(
                        lang_detected,
                        known_call_type,
                        known_speakers,
                        i + 1,
                        n,
                        &injected,
                    )
                }
            };
            let request = LlmRequest {
                model: None,
                system,
                input: chunk.clone(),
                max_tokens: Some(REFINE_MAX_TOKENS),
                grammar: None,
                json_schema: None,
            };
            match gbnf::generate_with_schema(provider, request, llm_schemas::SUMMARY_V2_JSON_SCHEMA)
                .await
            {
                Ok(json) if is_valid_state(&json) => {
                    result = Ok(json);
                    break;
                }
                Ok(json) => {
                    log::warn!(
                        "refine chunk {i} attempt {attempt}: invalid state (schema_version/title), retrying: {}",
                        &json.to_string()[..120.min(json.to_string().len())]
                    );
                    result = Err(LlmError::Provider("refine: invalid state JSON".into()));
                }
                Err(e) => result = Err(e),
            }
        }

        match result {
            Ok(json) => {
                let mut done = step_event(step_idx, total_steps, "refine", "done");
                done.chunk_no = Some(i as u32 + 1);
                done.chunk_total = Some(n as u32);
                done.preview = preview_from_summary(&json);
                steps.emit(done);
                state = Some(json);
            }
            Err(e) => {
                failed += 1;
                log::warn!("refine chunk {i} failed after retry (skipping): {e}");
                let mut fail_ev = step_event(step_idx, total_steps, "refine", "failed");
                fail_ev.chunk_no = Some(i as u32 + 1);
                fail_ev.chunk_total = Some(n as u32);
                steps.emit(fail_ev);
            }
        }
    }

    let Some(summary_json) = state else {
        return Err(AppError::Other(
            "refine_chain: all chunks failed, no recap state".into(),
        ));
    };
    if failed * 2 >= n {
        log::warn!("refine_chain: DEGRADED — {failed}/{n} чанков потеряно, recap будет неполным");
    }
    Ok(RefineOutcome {
        summary_json,
        chunks_total: n,
        chunks_failed: failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::recap_steps::test_support::VecSink;
    use async_trait::async_trait;
    use std::sync::Mutex;

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

    fn v2_json(title: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 2,
            "title": title,
            "summary": "stub summary",
            "key_points": ["p1", "p2"],
            "language": "ru",
            "call_type": "standup",
            "call_type_confidence": 0.8,
            "participants": [],
            "action_items": [],
            "decisions": [],
            "open_questions": [],
            "topics": [],
        })
    }

    fn fat_v2_json() -> serde_json::Value {
        let quote = "q".repeat(300);
        serde_json::json!({
            "schema_version": 2,
            "title": "fat",
            "summary": "s".repeat(3000),
            "key_points": (0..20).map(|i| format!("point {i}")).collect::<Vec<_>>(),
            "language": "ru",
            "call_type": "standup",
            "call_type_confidence": 0.8,
            "participants": [],
            "narrative": "n".repeat(2000),
            "mom": "legacy",
            "action_items": (0..20).map(|i| serde_json::json!({
                "id": format!("a{i}"), "text": format!("task {i}"),
                "owner_hint": null, "owner_confidence": 0.0,
                "due": null, "due_confidence": 0.0, "category": "idea",
                "evidence": { "quote": quote.clone(), "speaker": null }
            })).collect::<Vec<_>>(),
            "decisions": (0..12).map(|i| serde_json::json!({
                "id": format!("d{i}"), "text": format!("decision {i}"),
                "evidence": { "quote": quote.clone(), "speaker": null }, "confidence": 0.5
            })).collect::<Vec<_>>(),
            "open_questions": [],
            "topics": (0..8).map(|i| serde_json::json!({
                "title": format!("t{i}"), "points": ["a","b","c","d","e"]
            })).collect::<Vec<_>>(),
        })
    }

    // ── compact_recap_for_injection ──────────────────────────────────────

    #[test]
    fn compaction_always_drops_narrative_and_mom() {
        let out = compact_recap_for_injection(&fat_v2_json(), 100_000, false);
        assert!(!out.contains("narrative"));
        assert!(!out.contains("\"mom\""));
    }

    #[test]
    fn compaction_is_deterministic_and_idempotent_under_cap() {
        let fat = fat_v2_json();
        let a = compact_recap_for_injection(&fat, INJECTED_RECAP_MAX_TOKENS, false);
        let b = compact_recap_for_injection(&fat, INJECTED_RECAP_MAX_TOKENS, false);
        assert_eq!(a, b, "детерминизм");
        assert!(
            chunker::estimate_tokens(&a) <= INJECTED_RECAP_MAX_TOKENS,
            "должно влезть в бюджет: {} tokens",
            chunker::estimate_tokens(&a)
        );
        // Идемпотентность: компакция уже компактного не меняет размер класса.
        let reparsed: serde_json::Value = serde_json::from_str(&a).unwrap();
        let c = compact_recap_for_injection(&reparsed, INJECTED_RECAP_MAX_TOKENS, false);
        assert!(chunker::estimate_tokens(&c) <= INJECTED_RECAP_MAX_TOKENS);
    }

    #[test]
    fn aggressive_compaction_is_smaller() {
        let fat = fat_v2_json();
        let normal = compact_recap_for_injection(&fat, INJECTED_RECAP_MAX_TOKENS, false);
        let aggressive =
            compact_recap_for_injection(&fat, INJECTED_RECAP_MAX_TOKENS_AGGRESSIVE, true);
        assert!(aggressive.len() < normal.len());
        let parsed: serde_json::Value = serde_json::from_str(&aggressive).unwrap();
        assert!(parsed["key_points"].as_array().unwrap().len() <= 8);
        assert!(parsed["action_items"].as_array().unwrap().len() <= 8);
        assert!(parsed["decisions"].as_array().unwrap().len() <= 6);
    }

    #[test]
    fn small_recap_passes_through_untouched_fields() {
        let small = v2_json("small");
        let out = compact_recap_for_injection(&small, INJECTED_RECAP_MAX_TOKENS, false);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["title"], "small");
        assert_eq!(parsed["key_points"].as_array().unwrap().len(), 2);
    }

    // ── build_refine_prompt ──────────────────────────────────────────────

    #[test]
    fn refine_prompt_contains_current_recap_and_extend_rules() {
        let p = build_refine_prompt(Some("ru"), None, None, 2, 5, r#"{"title":"cur"}"#);
        assert!(p.contains("CURRENT_RECAP"));
        assert!(p.contains(r#"{"title":"cur"}"#));
        assert!(p.contains("EXTEND AND EDIT"));
        assert!(p.contains("PART 2 of 5"));
        assert!(!p.contains("MAP_OUTPUTS"));
    }

    #[test]
    fn refine_prompt_includes_expert_guide_and_type_pin_when_known() {
        let p = build_refine_prompt(
            Some("en"),
            Some(CallType::Standup),
            Some("- Alice"),
            3,
            4,
            "{}",
        );
        assert!(p.contains("SPECIALIZED GUIDE"));
        assert!(p.contains("`standup`"));
        assert!(p.contains("## Known participants"));
        assert!(p.contains("- Alice"));

        let p_universal = build_refine_prompt(Some("en"), None, None, 3, 4, "{}");
        assert!(!p_universal.contains("SPECIALIZED GUIDE"));
    }

    // ── run_refine_chain ─────────────────────────────────────────────────

    #[tokio::test]
    async fn happy_path_three_chunks_sequential_state_replacement() {
        let mock = MockProvider::new(vec![
            Ok(v2_json("after part 1")),
            Ok(v2_json("after part 2")),
            Ok(v2_json("after part 3")),
        ]);
        let sink = VecSink::new();
        let chunks = vec!["c0".to_string(), "c1".to_string(), "c2".to_string()];
        let outcome = run_refine_chain(&mock, &chunks, Some("ru"), None, None, &sink, 1, 6)
            .await
            .unwrap();
        assert_eq!(outcome.summary_json["title"], "after part 3");
        assert_eq!(outcome.chunks_failed, 0);
        assert_eq!(outcome.chunks_total, 3);

        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 3);
        // Первый — initial (PART 1 note, без CURRENT_RECAP).
        assert!(systems[0].contains("PART 1 of 3"));
        assert!(!systems[0].contains("CURRENT_RECAP"));
        // Второй/третий — refine с состоянием предыдущего шага.
        assert!(systems[1].contains("CURRENT_RECAP"));
        assert!(systems[1].contains("after part 1"));
        assert!(systems[2].contains("after part 2"));

        // События: 3× started + 3× done, у done есть preview.
        let events = sink.events();
        assert_eq!(events.len(), 6);
        let done: Vec<_> = events.iter().filter(|e| e.status == "done").collect();
        assert_eq!(done.len(), 3);
        assert!(done.iter().all(|e| e.preview.is_some()));
        assert_eq!(done[2].preview.as_ref().unwrap().title, "after part 3");
        // step_idx нумеруется от offset.
        assert_eq!(events[0].step_idx, 1);
        assert_eq!(events[5].step_idx, 3);
    }

    #[tokio::test]
    async fn failed_middle_chunk_retries_then_skips_and_chain_continues() {
        let mock = MockProvider::new(vec![
            Ok(v2_json("p1")),
            Err(LlmError::Provider("crash".into())),
            Err(LlmError::Provider("crash retry".into())),
            Ok(v2_json("p3")),
        ]);
        let sink = VecSink::new();
        let chunks = vec!["c0".into(), "c1".into(), "c2".into()];
        let outcome = run_refine_chain(&mock, &chunks, None, None, None, &sink, 0, 3)
            .await
            .unwrap();
        assert_eq!(outcome.summary_json["title"], "p3");
        assert_eq!(outcome.chunks_failed, 1);
        // 1 (init) + 2 (fail+retry) + 1 (refine c2) = 4 вызова.
        assert_eq!(mock.captured_systems().len(), 4);
        // c2-refine инъектит состояние p1 (не потеряно после падения c1).
        assert!(mock.captured_systems()[3].contains("\"p1\""));
        // failed-событие для чанка 2.
        let events = sink.events();
        assert!(events
            .iter()
            .any(|e| e.status == "failed" && e.chunk_no == Some(2)));
    }

    #[tokio::test]
    async fn failed_first_chunk_makes_next_chunk_initial() {
        let mock = MockProvider::new(vec![
            Err(LlmError::Provider("c0 crash".into())),
            Err(LlmError::Provider("c0 crash retry".into())),
            Ok(v2_json("from c1")),
        ]);
        let sink = VecSink::new();
        let chunks = vec!["c0".into(), "c1".into()];
        let outcome = run_refine_chain(&mock, &chunks, None, None, None, &sink, 0, 2)
            .await
            .unwrap();
        assert_eq!(outcome.summary_json["title"], "from c1");
        let systems = mock.captured_systems();
        assert_eq!(systems.len(), 3);
        // Третий вызов (chunk 1) — initial-стиль, не refine.
        assert!(systems[2].contains("PART 2 of 2"));
        assert!(!systems[2].contains("CURRENT_RECAP"));
    }

    #[tokio::test]
    async fn all_chunks_failed_returns_error() {
        let mock = MockProvider::new(vec![
            Err(LlmError::Provider("a".into())),
            Err(LlmError::Provider("b".into())),
            Err(LlmError::Provider("c".into())),
            Err(LlmError::Provider("d".into())),
        ]);
        let sink = VecSink::new();
        let chunks = vec!["c0".into(), "c1".into()];
        let err = run_refine_chain(&mock, &chunks, None, None, None, &sink, 0, 2)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("all chunks failed"));
    }

    #[tokio::test]
    async fn invalid_state_response_treated_as_failure() {
        // Ответ без schema_version → retry; второй такой же → skip чанка.
        let mock = MockProvider::new(vec![
            Ok(v2_json("good")),
            Ok(serde_json::json!({ "garbage": true })),
            Ok(serde_json::json!({ "schema_version": 2, "title": "  " })),
        ]);
        let sink = VecSink::new();
        let chunks = vec!["c0".into(), "c1".into()];
        let outcome = run_refine_chain(&mock, &chunks, None, None, None, &sink, 0, 2)
            .await
            .unwrap();
        // Чанк 1 дал мусор дважды → состояние осталось от чанка 0.
        assert_eq!(outcome.summary_json["title"], "good");
        assert_eq!(outcome.chunks_failed, 1);
    }
}
