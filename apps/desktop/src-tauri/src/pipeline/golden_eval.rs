//! [M14 T-12] Golden set + CI regression harness.
//!
//! ## Зачем
//!
//! Unit-тесты проверяют helpers (`parse_summary_v2_or_promote_legacy`,
//! `strip_unverified_evidence`, `dedup_items`) на синтетических входах.
//! Но system-level interplay (full pipeline parse → validate → strip →
//! dedup → expected output) не тестируется. Golden harness прогоняет
//! 10 reference cases (cloud v2 / legacy v1 promote / evidence stripping /
//! dedup / multilingual / empty arrays edge cases) и diff'ит против expected.
//!
//! Любая регрессия в parser/promote/validator/dedup ловится **deterministic'но**
//! на JSON-уровне (no LLM calls).
//!
//! ## Case format
//!
//! Каждый `case_NN_*.json` файл:
//! ```json
//! {
//!   "description": "Human-readable что тестируется",
//!   "transcript_md": "...",   // optional — для evidence verification
//!   "llm_input": { ... raw LLM JSON ... },
//!   "expected": { ... CallSummaryV2 после parse+strip+dedup ... }
//! }
//! ```
//!
//! ## Pipeline (matches recap.rs:persist_summary_v2)
//!
//! 1. `parse_summary_v2_or_promote_legacy(llm_input)` → `CallSummaryV2`
//! 2. Optional: `strip_unverified_evidence(summary, transcript_md, 0.90)` →
//!    drops items с garbage evidence
//! 3. `dedup_items(&mut summary)` → Jaccard ≥ 0.7 dedup
//! 4. Serialize to `serde_json::Value`
//! 5. Deep-diff vs expected
//!
//! ## Deferred (T-13 G-Eval)
//!
//! Этот harness — **structural diff**. T-13 добавит qualitative metrics
//! (coherence, faithfulness, relevance) через LLM-as-judge поверх тех же golden inputs.

#![cfg(test)]

use serde_json::Value;

use crate::pipeline::recap::parse_summary_v2_or_promote_legacy;
use crate::pipeline::summary_validator::{
    dedup_items, strip_unverified_evidence, DEFAULT_FUZZY_THRESHOLD,
};

#[derive(Debug, serde::Deserialize)]
struct GoldenCase {
    #[allow(dead_code)] // Used in panic messages.
    description: String,
    #[serde(default)]
    transcript_md: Option<String>,
    llm_input: Value,
    expected: Value,
}

fn run_golden_case(case_json: &str, case_name: &str) {
    let case: GoldenCase = serde_json::from_str(case_json)
        .unwrap_or_else(|e| panic!("[{case_name}] parse case JSON: {e}"));

    // 1. Parse input как CallSummaryV2 OR promote v1.
    let mut summary = parse_summary_v2_or_promote_legacy(case.llm_input.clone(), "golden-test")
        .unwrap_or_else(|e| panic!("[{case_name}] parse: {e} (desc: {})", case.description));

    // 2. Optional evidence stripping когда transcript_md given.
    if let Some(transcript) = &case.transcript_md {
        let (stripped, _dropped) =
            strip_unverified_evidence(summary, transcript, DEFAULT_FUZZY_THRESHOLD);
        summary = stripped;
    }

    // 3. Dedup.
    dedup_items(&mut summary);

    // 4. Serialize back to Value для structural diff.
    let actual =
        serde_json::to_value(&summary).unwrap_or_else(|e| panic!("[{case_name}] serialize: {e}"));

    // 5. Deep-diff.
    assert_json_eq(&actual, &case.expected, case_name, &case.description);
}

fn assert_json_eq(actual: &Value, expected: &Value, ctx: &str, desc: &str) {
    if actual != expected {
        panic!(
            "[{ctx}] golden mismatch ({desc}).\n--- actual ---\n{}\n--- expected ---\n{}",
            serde_json::to_string_pretty(actual).unwrap(),
            serde_json::to_string_pretty(expected).unwrap()
        );
    }
}

// ── Case fixtures (embedded at compile time) ─────────────────────────────

const CASE_01: &str = include_str!("golden_summaries/case_01_cloud_v2_sales_discovery.json");
const CASE_02: &str = include_str!("golden_summaries/case_02_cloud_v2_standup.json");
const CASE_03: &str = include_str!("golden_summaries/case_03_cloud_v2_one_on_one.json");
const CASE_04: &str = include_str!("golden_summaries/case_04_legacy_v1_promote.json");
const CASE_05: &str = include_str!("golden_summaries/case_05_legacy_v1_with_actions.json");
const CASE_06: &str = include_str!("golden_summaries/case_06_evidence_stripped.json");
const CASE_07: &str = include_str!("golden_summaries/case_07_dedup_action_items.json");
const CASE_08: &str = include_str!("golden_summaries/case_08_dedup_decisions.json");
const CASE_09: &str = include_str!("golden_summaries/case_09_multilingual_ru_en.json");
const CASE_10: &str = include_str!("golden_summaries/case_10_empty_arrays.json");

// ── Tests ────────────────────────────────────────────────────────────────

#[test]
fn golden_01_cloud_v2_sales_discovery() {
    run_golden_case(CASE_01, "case_01");
}

#[test]
fn golden_02_cloud_v2_standup() {
    run_golden_case(CASE_02, "case_02");
}

#[test]
fn golden_03_cloud_v2_one_on_one_privacy() {
    run_golden_case(CASE_03, "case_03");
}

#[test]
fn golden_04_legacy_v1_promote() {
    run_golden_case(CASE_04, "case_04");
}

#[test]
fn golden_05_legacy_v1_with_actions() {
    run_golden_case(CASE_05, "case_05");
}

#[test]
fn golden_06_evidence_stripped() {
    run_golden_case(CASE_06, "case_06");
}

#[test]
fn golden_07_dedup_action_items() {
    run_golden_case(CASE_07, "case_07");
}

#[test]
fn golden_08_dedup_decisions() {
    run_golden_case(CASE_08, "case_08");
}

#[test]
fn golden_09_multilingual_ru_en() {
    run_golden_case(CASE_09, "case_09");
}

#[test]
fn golden_10_empty_arrays() {
    run_golden_case(CASE_10, "case_10");
}
