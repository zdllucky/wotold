//! [M14 T-07 Phase C] Per-call-type focused prompts.
//!
//! ## Зачем
//!
//! Universal v2 prompt в `recap::build_v2_system_prompt` содержит inline
//! TYPE GUIDE — 9 type entries × ~9 строк = 80+ строк, которые LLM должен
//! прочитать, выбрать ОДИН тип, проигнорировать остальные 8. На локальных
//! 2-7B моделях это рассеивает внимание и LLM смешивает MoM headers.
//!
//! T-07 решает focused prompts: ОДИН prompt PER call_type содержит ТОЛЬКО
//! его MoM headers + type_specific_block schema + специфичные правила
//! (например privacy для one_on_one).
//!
//! ## Кто использует
//!
//! - `local_orchestrator::run_v2_pipeline` short path → когда classifier
//!   даёт `known_call_type=Some(t)` → `build_expert_system_prompt(t)`.
//! - `map_reduce::run_map_reduce` reduce step → когда есть known_call_type →
//!   `build_expert_reduce_prompt(t, map_outputs_json)`.
//! - Universal prompt остаётся для (a) cloud path (passes None), (b)
//!   fallback на classifier failure.
//!
//! ## Phase D/E (deferred)
//!
//! - T-08 action-item post-pass — отдельный validate-call.
//! - T-09 GBNF constrained decoding.

use crate::pipeline::summary_v2::CallType;

/// Per-type config: role hint + optional privacy/extra rules.
/// [MoM cleanup] mom_headers + type_specific_block schema убраны — слабая
/// локальная модель эхо-копировала их в `mom` (мусор в рекапе). Рекап теперь
/// без MoM-секций; остаётся role-фокус + privacy-правила.
struct TypeConfig {
    /// Slug идентификатор (snake_case, same as `CallType::as_str()`).
    slug: &'static str,
    /// Human-readable role hint в prompt'е.
    role_hint: &'static str,
    /// Privacy / extra rules block (one_on_one) — optional.
    extra_rules: Option<&'static str>,
}

fn type_config(call_type: CallType) -> TypeConfig {
    match call_type {
        CallType::SalesDiscovery => TypeConfig {
            slug: "sales_discovery",
            role_hint: "vendor rep exploring prospect's pain points, stakeholders, budget signals, and decision timeline",
            extra_rules: None,
        },
        CallType::SalesDemo => TypeConfig {
            slug: "sales_demo",
            role_hint: "vendor rep walking prospect through product capabilities while handling objections and capturing buying signals",
            extra_rules: None,
        },
        CallType::ProductSync => TypeConfig {
            slug: "product_sync",
            role_hint: "internal product team aligning on progress, blockers, decisions, and upcoming milestones",
            extra_rules: None,
        },
        CallType::Standup => TypeConfig {
            slug: "standup",
            role_hint: "short rotating team status updates — per-person yesterday/today/blockers, no deep discussion",
            extra_rules: None,
        },
        CallType::CustomerInterview => TypeConfig {
            slug: "customer_interview",
            role_hint: "user research interview — extract jobs-to-be-done, current workflow, pain quotes verbatim, feature requests",
            extra_rules: Some("Pain-quote action_items / decisions evidence MUST be verbatim from transcript (same rule as evidence quotes)."),
        },
        CallType::OneOnOne => TypeConfig {
            slug: "one_on_one",
            role_hint: "manager↔report 1:1 — personal feedback, growth, challenges, career conversation",
            extra_rules: Some("**PRIVACY-SENSITIVE:** do NOT include verbatim personal feedback в `evidence.quote` — paraphrase + set `evidence.quote = null`. `action_items` SHOULD include ONLY work-related commitments, not personal growth promises."),
        },
        CallType::StrategyBrainstorm => TypeConfig {
            slug: "strategy_brainstorm",
            role_hint: "open ideation session — capture ideas, surface top picks, log open questions and owners",
            extra_rules: None,
        },
        CallType::StatusUpdate => TypeConfig {
            slug: "status_update",
            role_hint: "formal workstream progress report — RAG status per stream, risks, asks for help",
            extra_rules: None,
        },
        CallType::Other => TypeConfig {
            slug: "other",
            role_hint: "generic meeting — call type doesn't fit specialized categories",
            extra_rules: None,
        },
    }
}

/// Shared ABSOLUTE RULES block — same content across universal + expert prompts.
fn absolute_rules_block() -> &'static str {
    "## ABSOLUTE RULES (violations are bugs)\n\
\n\
1. NEVER invent facts, names, dates, numbers, or commitments not present in the transcript.\n\
2. Capture EVERY real decision, action item, and open question raised in the call — aim for COMPLETENESS (typical business call has several of each). Leave an array empty ONLY if the call genuinely had none. For each item add `evidence.quote` = a verbatim substring (10-200 chars) from the transcript WHEN you can copy one; if you cannot, set `evidence.quote` to null but ALWAYS KEEP the item (never drop a real point just because you lack a verbatim quote).\n\
3. Owner attribution: only assign an owner if the transcript shows them explicitly accepting the task ('I'll do it', 'я возьму', 'I will take that'). Mere mention of a name is NOT enough. Set `owner_confidence`: 0.9+ only for explicit accept; 0.5 for inferred; 0.0 if no owner.\n\
4. Categorize each action_item:\n   - `commitment` — explicit accept ('я сделаю', 'I'll send it')\n   - `proposal` — suggested но не accepted\n   - `idea` — raised, no clear action\n\
5. Output ONLY ONE JSON object matching the schema. No prose, no markdown fences, no explanation.\n\
6. NEVER use raw 'Speaker 0', 'Speaker 1', 'owner' tags inside `summary`/`key_points`/`action_items.text`. Resolve to names via:\n   (a) Known participants block — exact name.\n   (b) Self-introduction in transcript.\n   (c) Generic role: 'клиент', 'представитель вендора', 'коллега'. NEVER 'Спикер 1'."
}

/// Shared OUTPUT SCHEMA block (CallSummaryV2) — strict schema for both universal + expert.
/// [MoM cleanup] mom + type_specific_block убраны из схемы.
fn output_schema_block() -> &'static str {
    "## OUTPUT SCHEMA (strict)\n\
\n\
{\n\
  \"schema_version\": 2,\n\
  \"title\": string,                              // 3-7 слов, headline-style. Конкретика, без 'Звонок про'.\n\
  \"summary\": string,                            // 3-5 предложений: о чём встреча, главные итоги, контекст. НЕ одна фраза.\n\
  \"key_points\": string[],                       // 5-10 конкретных пунктов с цифрами/датами/именами/решениями. Не общие фразы.\n\
  \"language\": \"ru\" | \"en\" | \"kk\" | \"mixed\",\n\
  \"call_type\": one of: sales_discovery, sales_demo, product_sync, standup, customer_interview, one_on_one, strategy_brainstorm, status_update, other,\n\
  \"call_type_confidence\": number (0..1),\n\
  \"participants\": [{ \"speaker_tag\": string, \"display_name\": string|null, \"role_hint\": string|null }],\n\
  \"action_items\": [{\n\
    \"id\": string,\n\
    \"text\": string,\n\
    \"owner_hint\": string|null,\n\
    \"owner_confidence\": number (0..1),\n\
    \"due\": string|null,\n\
    \"due_confidence\": number (0..1),\n\
    \"category\": \"commitment\"|\"proposal\"|\"idea\",\n\
    \"evidence\": { \"quote\": string|null, \"speaker\": string|null }\n\
  }],\n\
  \"decisions\": [{ \"id\": string, \"text\": string, \"evidence\": { \"quote\": string|null, \"speaker\": string|null }, \"confidence\": number (0..1) }],\n\
  \"open_questions\": [{ \"id\": string, \"text\": string, \"raised_by\": string|null, \"evidence\": { \"quote\": string|null, \"speaker\": string|null } }],\n\
  \"topics\": [{ \"title\": string, \"points\": string[] }]  // 2-5 обсуждённых тем, у каждой 1-4 конкретных под-пункта\n\
}"
}

/// Specialized guide block для одного call_type — только role-фокус + extra rules.
/// [MoM cleanup] MoM structure + type_specific_block schema убраны.
fn specialized_guide_block(cfg: &TypeConfig) -> String {
    let extras = cfg
        .extra_rules
        .map(|s| format!("\n\n### Additional rules\n\n{s}"))
        .unwrap_or_default();
    format!(
        "## SPECIALIZED GUIDE — `{slug}`\n\
\n\
This call has been classified as `{slug}` — {role}. Focus the summary, key_points, decisions, open_questions и action_items на этом контексте.{extras}",
        slug = cfg.slug,
        role = cfg.role_hint,
    )
}

fn language_formatting_block() -> &'static str {
    "## LANGUAGE & FORMATTING\n\
\n\
- Detect dominant language of transcript. Output ALL string fields (title, summary, key_points, action_items.text, decisions.text, open_questions.text) в этом языке.\n\
- `call_type` и `category` enum values остаются английскими (snake_case).\n\
- Mixed ru/en → respond в dominant + English tech terms as-is."
}

fn evidence_rules_block() -> &'static str {
    "## EVIDENCE QUOTE RULES\n\
\n\
- Verbatim substring of transcript. Preserve original language + casing + punctuation.\n\
- 10-200 characters length.\n\
- `evidence.speaker` отдельно (raw speaker_tag from transcript).\n\
- Если нет verifiable anchor → `evidence.quote = null` (backend drop'нет item)."
}

/// Expert prompt for full-transcript generation (short path в local_orchestrator).
pub(crate) fn build_expert_system_prompt(
    call_type: CallType,
    lang_detected: Option<&str>,
    known_speakers: Option<&str>,
) -> String {
    let lang = lang_detected.unwrap_or("ru");
    let cfg = type_config(call_type);
    let known_block = known_speakers
        .map(|s| format!("\n\n## Known participants\n\n{s}"))
        .unwrap_or_default();
    format!(
        "OUTPUT LANGUAGE = {lang}. EVERY string value (title, summary, key_points, decisions/action_items/open_questions text, topics) MUST be written in {lang}. Only enum values (call_type, category) stay English.\n\
\n\
You are a senior meeting analyst for Wotold specialized in {role}.\n\
\n\
{rules}\n\
\n\
{schema}\n\
\n\
{guide}\n\
\n\
{lang_block}\n\
\n\
{evidence}{known}",
        role = cfg.role_hint,
        rules = absolute_rules_block(),
        schema = output_schema_block(),
        guide = specialized_guide_block(&cfg),
        lang_block = language_formatting_block(),
        evidence = evidence_rules_block(),
        known = known_block,
    )
}

/// Expert reduce prompt — focused per-type aggregation of MAP_OUTPUTS.
pub(crate) fn build_expert_reduce_prompt(
    call_type: CallType,
    lang_detected: Option<&str>,
    known_speakers: Option<&str>,
    map_outputs_json: &str,
) -> String {
    let lang = lang_detected.unwrap_or("ru");
    let cfg = type_config(call_type);
    let known_block = known_speakers
        .map(|s| format!("\n\n## Known participants\n\n{s}"))
        .unwrap_or_default();
    format!(
        "OUTPUT LANGUAGE = {lang}. EVERY string value MUST be written in {lang} (only call_type/category enums stay English).\n\
\n\
You are a senior meeting analyst для REDUCE step of a long {role} call. You receive a JSON ARRAY of per-chunk MAP outputs. Your job: consolidate into ONE final `CallSummaryV2` JSON focused на call_type `{slug}`. Be COMPLETE — surface all decisions/action_items/open_questions/topics present in the MAP outputs.\n\
\n\
{rules}\n\
\n\
{schema}\n\
\n\
{guide}\n\
\n\
{evidence}{known}\n\
\n\
## MAP_OUTPUTS (input — array of per-chunk extractions)\n\
\n\
{map}\n\
\n\
Output ONLY the final CallSummaryV2 JSON object. No prose. Set `call_type` to `{slug}`.",
        role = cfg.role_hint,
        slug = cfg.slug,
        rules = absolute_rules_block(),
        schema = output_schema_block(),
        guide = specialized_guide_block(&cfg),
        evidence = evidence_rules_block(),
        known = known_block,
        map = map_outputs_json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_call_types() -> [CallType; 9] {
        [
            CallType::SalesDiscovery,
            CallType::SalesDemo,
            CallType::ProductSync,
            CallType::Standup,
            CallType::CustomerInterview,
            CallType::OneOnOne,
            CallType::StrategyBrainstorm,
            CallType::StatusUpdate,
            CallType::Other,
        ]
    }

    /// Sanity: каждый type имеет unique slug (9 типов).
    #[test]
    fn type_config_has_unique_slugs() {
        let mut slugs: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
        for ct in all_call_types() {
            assert!(
                slugs.insert(type_config(ct).slug),
                "duplicate slug across types"
            );
        }
        assert_eq!(slugs.len(), 9, "expected 9 unique slugs");
    }

    /// [MoM cleanup] Expert prompt для любого типа НЕ содержит MoM-заголовков
    /// и type_specific_block schema — слабая модель эхо-копировала их в рекап.
    #[test]
    fn expert_prompt_has_no_mom_or_type_specific_block() {
        for ct in all_call_types() {
            let p = build_expert_system_prompt(ct, None, None);
            for forbidden in [
                "## Status by workstream",
                "## Yesterday",
                "type_specific_block",
                "MoM structure",
                "workstreams",
                "\"mom\"",
            ] {
                assert!(
                    !p.contains(forbidden),
                    "{:?} prompt leaked MoM/tsb token: {forbidden}",
                    ct
                );
            }
        }
    }

    #[test]
    fn expert_prompt_includes_slug_and_role_focus() {
        let p = build_expert_system_prompt(CallType::StatusUpdate, None, None);
        assert!(p.contains("status_update"));
        assert!(p.contains("SPECIALIZED GUIDE"));
        assert!(p.contains("Focus the summary"));
    }

    #[test]
    fn expert_prompt_for_one_on_one_includes_privacy_note() {
        let p = build_expert_system_prompt(CallType::OneOnOne, None, None);
        assert!(p.contains("PRIVACY-SENSITIVE"));
        assert!(p.contains("paraphrase"));
    }

    #[test]
    fn expert_prompt_includes_known_speakers_when_present() {
        let p = build_expert_system_prompt(
            CallType::Standup,
            Some("ru"),
            Some("- owner = Damir Sagindyk"),
        );
        assert!(p.contains("Known participants"));
        assert!(p.contains("Damir Sagindyk"));
    }

    #[test]
    fn expert_reduce_prompt_embeds_map_outputs() {
        let map_outputs = serde_json::json!([
            { "chunk_idx": 0, "facts": ["alice committed to ship by Friday"] },
            { "chunk_idx": 1, "facts": ["bob agreed to review"] }
        ]);
        let json_str = serde_json::to_string(&map_outputs).unwrap();
        let p = build_expert_reduce_prompt(CallType::Standup, Some("en"), None, &json_str);
        assert!(
            p.contains(&json_str),
            "reduce prompt missing MAP_OUTPUTS body"
        );
        assert!(p.contains("MAP_OUTPUTS"));
        // Reduce should still set call_type explicitly.
        assert!(p.contains("`standup`"));
        // [MoM cleanup] no MoM headers / type_specific_block в reduce-промпте.
        assert!(!p.contains("## Yesterday"));
        assert!(!p.contains("type_specific_block"));
    }
}
