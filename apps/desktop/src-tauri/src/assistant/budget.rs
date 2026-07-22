//! [M15.6] Budget assembly: ранжированные пассажи → контекст ≤5.5K токенов.
//!
//! Бюджет из SPEC §1: окно 8192 = system ~0.6K + фрагменты ≤5.5K + история +
//! резерв ответа (PRD §3). Greedy по порядку retrieval (он уже отранжировал:
//! call-scope — свои раньше чужих, внутри — bm25). Дедуп душит overlap-окна
//! транскрипта (M15.3 нарезает с перекрытием в 1 реплику).

// [M15.6] Production caller — assistant::ask (M15.7).
#![allow(dead_code)]

use std::collections::HashMap;

use crate::assistant::retrieval::Scope;
use crate::assistant::types::AssistantPassageKind;
use crate::db::assistant::PassageHit;

/// Бюджет фрагментов (SPEC §1: «top-k фрагментов до бюджета ~5.5K токенов»).
pub const FRAGMENT_BUDGET_TOKENS: i64 = 5_500;

/// Cap пассажей на один звонок в global-scope (разнообразие источников).
const MAX_PASSAGES_PER_CALL_GLOBAL: usize = 3;

/// Собранный контекст. Порядок fragments стабилен — нумерация [1..N]
/// для промпта (M15.7) = индекс+1. token_total — для mono-строки UI.
/// [M16.1] skipped_* — внутренняя диагностика отбора (debug-лог ask_core),
/// в S2-контракт НЕ уходит.
#[derive(Debug, Clone)]
pub struct BudgetedContext {
    pub fragments: Vec<PassageHit>,
    pub token_total: i64,
    pub skipped_dedup: usize,
    pub skipped_cap: usize,
    pub skipped_budget: usize,
}

/// Greedy-сборка: дедуп (текст + transcript-overlap) → cap/звонок (global) →
/// бюджет с skip-and-continue (после большого пассажа мелкий ещё может влезть).
/// Принимает hits по значению (retrieval отдаёт owned Vec) — без клонов текста.
pub fn assemble(hits: Vec<PassageHit>, scope: Scope<'_>) -> BudgetedContext {
    let is_global = matches!(scope, Scope::Global);
    let mut fragments: Vec<PassageHit> = Vec::new();
    let mut token_total: i64 = 0;
    let mut per_call: HashMap<String, usize> = HashMap::new();
    // Занятые интервалы транскрипта per call — душим overlap-окна.
    let mut taken_ranges: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    let (mut skipped_dedup, mut skipped_cap, mut skipped_budget) = (0usize, 0usize, 0usize);

    for hit in hits {
        if fragments.iter().any(|f| f.text == hit.text) {
            skipped_dedup += 1;
            continue;
        }
        if is_transcript_overlap(&taken_ranges, &hit) {
            skipped_dedup += 1;
            continue;
        }
        if is_global
            && per_call.get(hit.call_id.as_str()).copied().unwrap_or(0)
                >= MAX_PASSAGES_PER_CALL_GLOBAL
        {
            skipped_cap += 1;
            continue;
        }
        if token_total + hit.token_est > FRAGMENT_BUDGET_TOKENS {
            skipped_budget += 1;
            continue; // skip-and-continue: следующий может быть меньше
        }

        token_total += hit.token_est;
        *per_call.entry(hit.call_id.clone()).or_insert(0) += 1;
        if let (Some(start), end) = (hit.start_ms, hit.end_ms) {
            taken_ranges
                .entry(hit.call_id.clone())
                .or_default()
                .push((start, end.unwrap_or(i64::MAX)));
        }
        fragments.push(hit);
    }

    BudgetedContext {
        fragments,
        token_total,
        skipped_dedup,
        skipped_cap,
        skipped_budget,
    }
}

/// Transcript-пассаж, чей start_ms попадает в уже взятый интервал того же
/// звонка, — overlap-сосед взятого окна (нарезка M15.3 с перекрытием).
fn is_transcript_overlap(taken: &HashMap<String, Vec<(i64, i64)>>, hit: &PassageHit) -> bool {
    let Some(start) = hit.start_ms else {
        return false; // recap/structured без таймкода — не окна
    };
    if hit.kind != AssistantPassageKind::Transcript.as_str() {
        return false;
    }
    taken
        .get(hit.call_id.as_str())
        .is_some_and(|ranges| ranges.iter().any(|&(a, b)| start >= a && start < b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::retrieval::Scope;

    fn hit(
        call: &str,
        kind: &str,
        start_ms: Option<i64>,
        end_ms: Option<i64>,
        tokens: i64,
        text: &str,
    ) -> PassageHit {
        PassageHit {
            id: 0,
            call_id: call.into(),
            kind: kind.into(),
            speaker: None,
            start_ms,
            end_ms,
            text: text.into(),
            token_est: tokens,
            rank: -1.0,
        }
    }

    #[test]
    fn budget_boundary_exact_fit_and_skip_and_continue() {
        let hits = vec![
            hit("c1", "transcript", Some(0), Some(10_000), 5_400, "большой"),
            hit("c2", "transcript", Some(0), Some(10_000), 200, "не влезает"), // 5600 > 5500
            hit("c3", "decision", None, None, 100, "мелкий влезает"),          // ровно 5500
            hit("c4", "decision", None, None, 1, "уже нет"),
        ];
        let ctx = assemble(hits, Scope::Global);
        let texts: Vec<&str> = ctx.fragments.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(texts, vec!["большой", "мелкий влезает"]);
        assert_eq!(ctx.token_total, 5_500);
    }

    // [M16.1] Счётчики скипов — диагностика debug-лога ask_core.
    #[test]
    fn skip_counters_track_dedup_cap_and_budget() {
        let hits = vec![
            hit("c1", "transcript", Some(0), Some(10_000), 100, "а"),
            hit("c1", "recap", None, None, 100, "а"), // текст-дубль → dedup
            hit("c1", "recap", None, None, 100, "б"),
            hit("c1", "decision", None, None, 100, "в"),
            hit("c1", "decision", None, None, 100, "г"), // 4-й в c1 → cap
            hit(
                "c2",
                "transcript",
                Some(0),
                Some(1_000),
                6_000,
                "не влезает",
            ), // budget
        ];
        let ctx = assemble(hits, Scope::Global);
        assert_eq!(ctx.fragments.len(), 3);
        assert_eq!(ctx.skipped_dedup, 1);
        assert_eq!(ctx.skipped_cap, 1);
        assert_eq!(ctx.skipped_budget, 1);
    }

    #[test]
    fn per_call_cap_only_in_global() {
        let hits: Vec<PassageHit> = (0..5)
            .map(|i| hit("c1", "decision", None, None, 10, &format!("решение {i}")))
            .collect();
        assert_eq!(assemble(hits.clone(), Scope::Global).fragments.len(), 3); // global cap
        assert_eq!(assemble(hits, Scope::Call("c1")).fragments.len(), 5); // call-scope без cap
    }

    #[test]
    fn transcript_overlap_windows_are_deduped() {
        // Окна M15.3 с перекрытием: [0..20s), [10s..30s) — второе начинается
        // внутри первого → скип. Третье [30s..) — берём.
        let hits = vec![
            hit("c1", "transcript", Some(0), Some(20_000), 100, "окно А"),
            hit(
                "c1",
                "transcript",
                Some(10_000),
                Some(30_000),
                100,
                "окно Б (overlap)",
            ),
            hit("c1", "transcript", Some(30_000), None, 100, "окно В"),
        ];
        let ctx = assemble(hits, Scope::Call("c1"));
        let texts: Vec<&str> = ctx.fragments.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(texts, vec!["окно А", "окно В"]);
    }

    #[test]
    fn overlap_dedup_is_per_call_and_transcript_only() {
        let hits = vec![
            hit("c1", "transcript", Some(0), Some(20_000), 100, "звонок 1"),
            hit(
                "c2",
                "transcript",
                Some(5_000),
                Some(15_000),
                100,
                "звонок 2 — другой звонок",
            ),
            hit(
                "c1",
                "decision",
                Some(5_000),
                None,
                50,
                "decision с таймкодом внутри окна",
            ),
        ];
        let ctx = assemble(hits, Scope::Call("c1"));
        assert_eq!(
            ctx.fragments.len(),
            3,
            "чужие звонки и structured не дедупятся по интервалам"
        );
    }

    #[test]
    fn exact_text_duplicates_collapse() {
        let hits = vec![
            hit("c1", "recap", None, None, 50, "одинаковый текст"),
            hit("c2", "recap", None, None, 50, "одинаковый текст"),
        ];
        assert_eq!(assemble(hits, Scope::Global).fragments.len(), 1);
    }

    #[test]
    fn order_is_stable_and_empty_input_ok() {
        let hits = vec![
            hit("c1", "decision", None, None, 10, "первый"),
            hit("c2", "decision", None, None, 10, "второй"),
            hit("c3", "decision", None, None, 10, "третий"),
        ];
        let ctx = assemble(hits, Scope::Global);
        let texts: Vec<&str> = ctx.fragments.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(texts, vec!["первый", "второй", "третий"]);
        assert_eq!(ctx.token_total, 30);

        let empty = assemble(Vec::new(), Scope::Global);
        assert!(empty.fragments.is_empty());
        assert_eq!(empty.token_total, 0);
    }

    #[test]
    fn open_ended_last_window_blocks_following_starts() {
        // end_ms = None (последнее окно звонка) → интервал до бесконечности:
        // любые более поздние старты того же звонка внутри.
        let hits = vec![
            hit("c1", "transcript", Some(100_000), None, 100, "хвост"),
            hit(
                "c1",
                "transcript",
                Some(200_000),
                Some(210_000),
                100,
                "позже хвоста",
            ),
        ];
        let ctx = assemble(hits, Scope::Call("c1"));
        assert_eq!(ctx.fragments.len(), 1);
    }
}
