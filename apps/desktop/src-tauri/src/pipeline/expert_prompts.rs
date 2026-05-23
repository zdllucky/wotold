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

/// Per-type config: focused MoM headers + type_specific_block schema +
/// optional privacy/extra rules.
struct TypeConfig {
    /// Slug идентификатор (snake_case, same as `CallType::as_str()`).
    slug: &'static str,
    /// Human-readable role hint в prompt'е.
    role_hint: &'static str,
    /// MoM section headers строго в этом порядке.
    mom_headers: &'static [&'static str],
    /// JSON schema text для type_specific_block ИЛИ "null" для `Other`.
    tsb_schema: &'static str,
    /// Privacy / extra rules block (one_on_one) — optional.
    extra_rules: Option<&'static str>,
}

fn type_config(call_type: CallType) -> TypeConfig {
    match call_type {
        CallType::SalesDiscovery => TypeConfig {
            slug: "sales_discovery",
            role_hint: "vendor rep exploring prospect's pain points, stakeholders, budget signals, and decision timeline",
            mom_headers: &[
                "## Customer pain",
                "## Stakeholders",
                "## Budget signals",
                "## Next steps",
            ],
            tsb_schema: "{ \"pain_points\": string[], \"current_solution\": string|null, \"budget_signal\": string|null, \"decision_makers\": [{\"name\":string,\"role\":string|null,\"stance\":\"champion\"|\"neutral\"|\"blocker\"|\"unknown\"}], \"timeline_hint\": string|null }",
            extra_rules: None,
        },
        CallType::SalesDemo => TypeConfig {
            slug: "sales_demo",
            role_hint: "vendor rep walking prospect through product capabilities while handling objections and capturing buying signals",
            mom_headers: &[
                "## Demo flow",
                "## Objections",
                "## Buying signals",
                "## Follow-up commitments",
            ],
            tsb_schema: "{ \"objections\": [{\"raised\":string,\"resolved\":bool}], \"buying_signals\": string[] }",
            extra_rules: None,
        },
        CallType::ProductSync => TypeConfig {
            slug: "product_sync",
            role_hint: "internal product team aligning on progress, blockers, decisions, and upcoming milestones",
            mom_headers: &[
                "## Progress",
                "## Blockers",
                "## Decisions",
                "## Next milestones",
            ],
            tsb_schema: "{ \"blockers\": string[], \"milestones\": [{\"name\":string,\"target\":string}] }",
            extra_rules: None,
        },
        CallType::Standup => TypeConfig {
            slug: "standup",
            role_hint: "short rotating team status updates — per-person yesterday/today/blockers, no deep discussion",
            mom_headers: &["## Yesterday", "## Today", "## Blockers"],
            tsb_schema: "{ \"per_person\": [{\"speaker\":string,\"yesterday\":string,\"today\":string,\"blockers\":string|null}] }",
            extra_rules: Some("Each per_person entry maps к одному participant. Если кто-то skip'нул свой update — omit, don't fabricate."),
        },
        CallType::CustomerInterview => TypeConfig {
            slug: "customer_interview",
            role_hint: "user research interview — extract jobs-to-be-done, current workflow, pain quotes verbatim, feature requests",
            mom_headers: &[
                "## Job to be done",
                "## Current workflow",
                "## Pain quotes",
                "## Feature requests",
            ],
            tsb_schema: "{ \"jtbd\": string|null, \"pain_quotes\": [{\"quote\":string,\"speaker\":string}] }",
            extra_rules: Some("`pain_quotes[i].quote` MUST be verbatim from transcript (same rule as evidence quotes)."),
        },
        CallType::OneOnOne => TypeConfig {
            slug: "one_on_one",
            role_hint: "manager↔report 1:1 — personal feedback, growth, challenges, career conversation",
            mom_headers: &[
                "## Wins",
                "## Challenges",
                "## Feedback",
                "## Career",
            ],
            tsb_schema: "{ \"topics_discussed\": string[] (≤5), \"follow_ups_committed\": string[] }",
            extra_rules: Some("**PRIVACY-SENSITIVE:** do NOT include verbatim personal feedback в `evidence.quote` — paraphrase + set `evidence.quote = null`. `action_items` SHOULD include ONLY work-related commitments, not personal growth promises."),
        },
        CallType::StrategyBrainstorm => TypeConfig {
            slug: "strategy_brainstorm",
            role_hint: "open ideation session — capture ideas with vote counts, surface top picks, log open questions and owners",
            mom_headers: &[
                "## Ideas",
                "## Top picks",
                "## Open questions",
                "## Owners",
            ],
            tsb_schema: "{ \"ideas\": [{\"text\":string,\"votes\":number|null}] }",
            extra_rules: None,
        },
        CallType::StatusUpdate => TypeConfig {
            slug: "status_update",
            role_hint: "formal workstream progress report — RAG status per stream, risks, asks for help",
            mom_headers: &["## Status by workstream", "## Risks", "## Asks"],
            tsb_schema: "{ \"workstreams\": [{\"name\":string,\"status\":\"green\"|\"yellow\"|\"red\",\"note\":string}] }",
            extra_rules: None,
        },
        CallType::Other => TypeConfig {
            slug: "other",
            role_hint: "generic meeting — call type doesn't fit specialized categories",
            mom_headers: &[
                "## Контекст",
                "## Обсудили",
                "## Решения",
                "## Дальнейшие шаги",
            ],
            tsb_schema: "null",
            extra_rules: None,
        },
    }
}

/// Shared ABSOLUTE RULES block — same content across universal + expert prompts.
fn absolute_rules_block() -> &'static str {
    "## ABSOLUTE RULES (violations are bugs)\n\
\n\
1. NEVER invent facts, names, dates, numbers, or commitments not present in the transcript.\n\
2. Every `action_items[i]`, `decisions[i]`, `open_questions[i]` SHOULD include `evidence.quote` — a verbatim substring (10-200 chars) copied from the transcript. If you cannot find a verbatim anchor, OMIT the item rather than fabricate.\n\
3. Owner attribution: only assign an owner if the transcript shows them explicitly accepting the task ('I'll do it', 'я возьму', 'I will take that'). Mere mention of a name is NOT enough. Set `owner_confidence`: 0.9+ only for explicit accept; 0.5 for inferred; 0.0 if no owner.\n\
4. Categorize each action_item:\n   - `commitment` — explicit accept ('я сделаю', 'I'll send it')\n   - `proposal` — suggested но не accepted\n   - `idea` — raised, no clear action\n\
5. Output ONLY ONE JSON object matching the schema. No prose, no markdown fences, no explanation.\n\
6. NEVER use raw 'Speaker 0', 'Speaker 1', 'owner' tags inside `summary`/`key_points`/`mom`/`action_items.text`. Resolve to names via:\n   (a) Known participants block — exact name.\n   (b) Self-introduction in transcript.\n   (c) Generic role: 'клиент', 'представитель вендора', 'коллега'. NEVER 'Спикер 1'."
}

/// Shared OUTPUT SCHEMA block (CallSummaryV2) — strict schema for both universal + expert.
fn output_schema_block(tsb_schema: &str) -> String {
    format!(
        "## OUTPUT SCHEMA (strict)\n\
\n\
{{\n\
  \"schema_version\": 2,\n\
  \"title\": string,                              // 3-7 слов, headline-style. Конкретика, без 'Звонок про'.\n\
  \"summary\": string,                            // 1-2 предложения TL;DR.\n\
  \"key_points\": string[],                       // 3-7 пунктов. Конкретные факты с цифрами/датами/решениями.\n\
  \"language\": \"ru\" | \"en\" | \"kk\" | \"mixed\",\n\
  \"call_type\": one of: sales_discovery, sales_demo, product_sync, standup, customer_interview, one_on_one, strategy_brainstorm, status_update, other,\n\
  \"call_type_confidence\": number (0..1),\n\
  \"participants\": [{{ \"speaker_tag\": string, \"display_name\": string|null, \"role_hint\": string|null }}],\n\
  \"action_items\": [{{\n\
    \"id\": string,\n\
    \"text\": string,\n\
    \"owner_hint\": string|null,\n\
    \"owner_confidence\": number (0..1),\n\
    \"due\": string|null,\n\
    \"due_confidence\": number (0..1),\n\
    \"category\": \"commitment\"|\"proposal\"|\"idea\",\n\
    \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }}\n\
  }}],\n\
  \"decisions\": [{{ \"id\": string, \"text\": string, \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }}, \"confidence\": number (0..1) }}],\n\
  \"open_questions\": [{{ \"id\": string, \"text\": string, \"raised_by\": string|null, \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }} }}],\n\
  \"mom\": string (Markdown — focused structure per call_type, см. SPECIALIZED GUIDE),\n\
  \"type_specific_block\": {tsb_schema}\n\
}}"
    )
}

/// Specialized guide block для одного call_type. Заменяет inline 9-type TYPE GUIDE.
fn specialized_guide_block(cfg: &TypeConfig) -> String {
    let headers = cfg.mom_headers.join(" / ");
    let extras = cfg
        .extra_rules
        .map(|s| format!("\n\n### Additional rules\n\n{s}"))
        .unwrap_or_default();
    format!(
        "## SPECIALIZED GUIDE — `{slug}`\n\
\n\
This call has been classified as `{slug}` — {role}.\n\
\n\
### MoM structure (use EXACTLY these headers, in this order)\n\
\n\
{headers}\n\
\n\
### type_specific_block schema\n\
\n\
{tsb}{extras}",
        slug = cfg.slug,
        role = cfg.role_hint,
        headers = headers,
        tsb = cfg.tsb_schema,
    )
}

fn language_formatting_block() -> &'static str {
    "## LANGUAGE & FORMATTING\n\
\n\
- Detect dominant language of transcript. Output ALL string fields (title, summary, key_points, mom, action_items.text, decisions.text, open_questions.text) в этом языке.\n\
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
        "You are a senior meeting analyst for Wotold specialized in {role}. Output language: {lang}.\n\
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
        schema = output_schema_block(cfg.tsb_schema),
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
        "You are a senior meeting analyst для REDUCE step of a long {role} call. You receive a JSON ARRAY of per-chunk MAP outputs. Your job: consolidate into ONE final `CallSummaryV2` JSON focused на call_type `{slug}`.\n\
\n\
Output language: {lang}.\n\
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
        schema = output_schema_block(cfg.tsb_schema),
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

    /// Sanity: каждый type имеет non-empty MoM headers + unique slug.
    /// Generic headers (## Blockers, ## Decisions) разрешено share между типами —
    /// focus в специализированных секциях, не в названиях MoM.
    #[test]
    fn type_config_has_non_empty_headers_and_unique_slugs() {
        let mut slugs: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
        for ct in all_call_types() {
            let cfg = type_config(ct);
            assert!(
                !cfg.mom_headers.is_empty(),
                "{:?} has empty mom_headers",
                ct
            );
            assert!(
                slugs.insert(cfg.slug),
                "duplicate slug across types: {}",
                cfg.slug
            );
        }
        assert_eq!(slugs.len(), 9, "expected 9 unique slugs");
    }

    /// Для каждого type expert prompt включает СВОИ headers и НЕ включает чужие.
    fn assert_focused_headers(call_type: CallType, prompt: &str) {
        let own = type_config(call_type);
        // Свои headers — present.
        for h in own.mom_headers {
            assert!(
                prompt.contains(h),
                "{:?} prompt missing own header: {h}",
                call_type
            );
        }
        // Чужие headers — absent.
        for other_ct in all_call_types() {
            if other_ct == call_type {
                continue;
            }
            let other_cfg = type_config(other_ct);
            for h in other_cfg.mom_headers {
                // Пропускаем если header случайно совпадает с моим (sanity test
                // выше это запретит, но safe guard).
                if own.mom_headers.contains(h) {
                    continue;
                }
                assert!(
                    !prompt.contains(h),
                    "{:?} prompt leaked header from {:?}: {h}",
                    call_type,
                    other_ct
                );
            }
        }
    }

    #[test]
    fn expert_prompt_for_sales_discovery_includes_focused_headers() {
        let p = build_expert_system_prompt(CallType::SalesDiscovery, Some("en"), None);
        assert_focused_headers(CallType::SalesDiscovery, &p);
        assert!(p.contains("pain_points"));
        assert!(p.contains("decision_makers"));
    }

    #[test]
    fn expert_prompt_for_sales_demo_includes_focused_headers() {
        let p = build_expert_system_prompt(CallType::SalesDemo, None, None);
        assert_focused_headers(CallType::SalesDemo, &p);
        assert!(p.contains("objections"));
        assert!(p.contains("buying_signals"));
    }

    #[test]
    fn expert_prompt_for_product_sync_includes_focused_headers() {
        let p = build_expert_system_prompt(CallType::ProductSync, None, None);
        assert_focused_headers(CallType::ProductSync, &p);
        assert!(p.contains("milestones"));
    }

    #[test]
    fn expert_prompt_for_standup_includes_focused_headers() {
        let p = build_expert_system_prompt(CallType::Standup, None, None);
        assert_focused_headers(CallType::Standup, &p);
        assert!(p.contains("per_person"));
    }

    #[test]
    fn expert_prompt_for_customer_interview_includes_focused_headers() {
        let p = build_expert_system_prompt(CallType::CustomerInterview, None, None);
        assert_focused_headers(CallType::CustomerInterview, &p);
        assert!(p.contains("jtbd"));
        assert!(p.contains("pain_quotes"));
    }

    #[test]
    fn expert_prompt_for_one_on_one_includes_privacy_note() {
        let p = build_expert_system_prompt(CallType::OneOnOne, None, None);
        assert_focused_headers(CallType::OneOnOne, &p);
        assert!(p.contains("PRIVACY-SENSITIVE"));
        assert!(p.contains("paraphrase"));
        assert!(p.contains("topics_discussed"));
    }

    #[test]
    fn expert_prompt_for_strategy_brainstorm_includes_focused_headers() {
        let p = build_expert_system_prompt(CallType::StrategyBrainstorm, None, None);
        assert_focused_headers(CallType::StrategyBrainstorm, &p);
        assert!(p.contains("\"ideas\""));
    }

    #[test]
    fn expert_prompt_for_status_update_includes_focused_headers() {
        let p = build_expert_system_prompt(CallType::StatusUpdate, None, None);
        assert_focused_headers(CallType::StatusUpdate, &p);
        assert!(p.contains("workstreams"));
        assert!(p.contains("\"green\""));
    }

    #[test]
    fn expert_prompt_for_other_uses_generic_headers_and_null_tsb() {
        let p = build_expert_system_prompt(CallType::Other, None, None);
        assert_focused_headers(CallType::Other, &p);
        assert!(p.contains("type_specific_block"));
        assert!(p.contains("null")); // tsb_schema = "null"
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
        // Focused headers stay.
        assert_focused_headers(CallType::Standup, &p);
    }
}
