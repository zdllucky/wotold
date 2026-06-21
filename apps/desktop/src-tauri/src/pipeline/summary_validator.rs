//! [M14 foundation] Validator для [`CallSummaryV2`] — substring fuzzy
//! evidence verification + schema range checks + dedup.

// [M14 foundation] Module-wide dead-code allow — все pub fn здесь это
// backbone для будущих фаз (T-08 action-item post-pass, T-10 orchestrator).
// Production callers подключатся когда orchestrator переедет на v2 schema.
// Без allow lints clippy.dead_code = deny (strict configuration в Cargo.toml).
#![allow(dead_code)]

//!
//! Зачем: PRD §5.1 ABSOLUTE RULE #2 — каждый action_item / decision /
//! open_question MUST include `evidence.quote` как verbatim substring
//! transcript'а. Cloud LLMs (особенно Grok 19.2% hallucination на Vectara
//! HHEM) любят выдумывать — без post-hoc verification фабрикации пробьются
//! в production. Validator drops items с failing evidence (NOT fail entire
//! summary), сохраняет partial output как degraded ok.
//!
//! **Fuzzy substring match** — своя naive sliding-window Levenshtein
//! similarity. Достаточно для 0.85-0.95 threshold (Vectara hallucination
//! tests показывают что 0.85-0.90 надёжно ловит fabrications). Не тянем
//! новых crates (rapidfuzz/strsim 50KB+ blob).
//!
//! Performance: literal `transcript.contains(quote)` check first — 95%
//! случаев hit, skip fuzzy. Fuzzy O(n·m) на 5K transcript × 100-char quote
//! ≈ 500K char ops ≈ < 1ms.

use crate::pipeline::summary_v2::CallSummaryV2;

/// Default cosine-like threshold для substring fuzzy match. PRD §6.2
/// validator step 3 указывает 0.90.
pub const DEFAULT_FUZZY_THRESHOLD: f32 = 0.90;

/// Default Jaccard token-overlap для dedup intent matching. ≥ 0.7 = same
/// intent → merge.
pub const DEFAULT_DEDUP_THRESHOLD: f32 = 0.7;

/// Кинд провалившегося item — нужно для error reporting + telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    ActionItem,
    Decision,
    OpenQuestion,
}

#[derive(Debug, Clone)]
pub enum ValidationError {
    EmptyTitle,
    EmptySummary,
    KeyPointsCountOutOfRange {
        count: usize,
    },
    /// `actual` outside [0, 1]; `field` для логирования (call_type_confidence,
    /// owner_confidence, etc).
    ConfidenceOutOfRange {
        field: String,
        actual: f32,
    },
    EvidenceQuoteMissing {
        kind: ItemKind,
        item_id: String,
    },
    EvidenceQuoteEmpty {
        kind: ItemKind,
        item_id: String,
    },
    EvidenceQuoteNotFound {
        kind: ItemKind,
        item_id: String,
        quote: String,
        fuzzy_score: f32,
    },
}

/// Pure-fn schema validation — confidence ranges, len bounds, no I/O.
pub fn validate_schema(summary: &CallSummaryV2) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if summary.title.trim().is_empty() {
        errors.push(ValidationError::EmptyTitle);
    }
    if summary.summary.trim().is_empty() {
        errors.push(ValidationError::EmptySummary);
    }
    // PRD §5.1: key_points 3..7.
    if !(3..=7).contains(&summary.key_points.len()) {
        errors.push(ValidationError::KeyPointsCountOutOfRange {
            count: summary.key_points.len(),
        });
    }
    if !(0.0..=1.0).contains(&summary.call_type_confidence) {
        errors.push(ValidationError::ConfidenceOutOfRange {
            field: "call_type_confidence".into(),
            actual: summary.call_type_confidence,
        });
    }
    for ai in &summary.action_items {
        if let Some(c) = ai.owner_confidence {
            if !(0.0..=1.0).contains(&c) {
                errors.push(ValidationError::ConfidenceOutOfRange {
                    field: format!("action_items[{}].owner_confidence", ai.id),
                    actual: c,
                });
            }
        }
        if let Some(c) = ai.due_confidence {
            if !(0.0..=1.0).contains(&c) {
                errors.push(ValidationError::ConfidenceOutOfRange {
                    field: format!("action_items[{}].due_confidence", ai.id),
                    actual: c,
                });
            }
        }
    }
    for d in &summary.decisions {
        if let Some(c) = d.confidence {
            if !(0.0..=1.0).contains(&c) {
                errors.push(ValidationError::ConfidenceOutOfRange {
                    field: format!("decisions[{}].confidence", d.id),
                    actual: c,
                });
            }
        }
    }
    errors
}

/// Substring fuzzy match — возвращает best similarity score (0..1) для
/// `needle` против `haystack`. 1.0 = literal match found; 0.0 = ничего
/// близкого.
///
/// Algorithm: sliding-window Levenshtein на normalized текстах. Lower +
/// collapse whitespace на обоих перед сравнением — устраняет false
/// negatives от форматирования.
pub fn substring_fuzzy_score(needle: &str, haystack: &str) -> f32 {
    let needle_norm = normalize(needle);
    let haystack_norm = normalize(haystack);
    if needle_norm.is_empty() {
        return 0.0;
    }
    // Fast path: literal match → 1.0.
    if haystack_norm.contains(&needle_norm) {
        return 1.0;
    }
    let n_chars: Vec<char> = needle_norm.chars().collect();
    let h_chars: Vec<char> = haystack_norm.chars().collect();
    let n_len = n_chars.len();
    let h_len = h_chars.len();
    if h_len < n_len {
        // Haystack короче needle — partial best vs full needle.
        return partial_similarity(&n_chars, &h_chars);
    }
    let mut best = 0.0_f32;
    // Slide window len=n_len по haystack; на каждом window считаем
    // similarity = 1 - edit_distance / n_len.
    let max_start = h_len.saturating_sub(n_len);
    for start in 0..=max_start {
        let window = &h_chars[start..start + n_len];
        let sim = partial_similarity(&n_chars, window);
        if sim > best {
            best = sim;
            if best >= 0.999 {
                // Достаточно близко к 1.0, ранний выход.
                return best;
            }
        }
    }
    best
}

/// Levenshtein-similarity для two slices одной длины (approximately).
/// Returns 1 - normalized_edit_distance в [0, 1].
fn partial_similarity(a: &[char], b: &[char]) -> f32 {
    let dist = levenshtein(a, b);
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    1.0 - (dist as f32) / (max_len as f32)
}

/// Naive O(n·m) Levenshtein distance. m, n ≤ 200 (quote length cap),
/// тривиально для production.
fn levenshtein(a: &[char], b: &[char]) -> usize {
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0_usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Normalize: lowercase + collapse whitespace + trim. Устраняет
/// false-negative от \r\n / leading spaces / capitalization.
fn normalize(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_was_space = false;
    for c in lower.chars() {
        if c.is_whitespace() {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Verify evidence quotes для всех action_items / decisions / open_questions.
/// Returns Vec<ValidationError> — пустой если всё ок.
pub fn verify_evidence_quotes(
    summary: &CallSummaryV2,
    transcript_text: &str,
    fuzzy_threshold: f32,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for ai in &summary.action_items {
        check_evidence(
            &ai.evidence,
            &ai.id,
            ItemKind::ActionItem,
            transcript_text,
            fuzzy_threshold,
            &mut errors,
            /*required=*/ true,
        );
    }
    for d in &summary.decisions {
        check_evidence(
            &d.evidence,
            &d.id,
            ItemKind::Decision,
            transcript_text,
            fuzzy_threshold,
            &mut errors,
            true,
        );
    }
    for q in &summary.open_questions {
        check_evidence(
            &q.evidence,
            &q.id,
            ItemKind::OpenQuestion,
            transcript_text,
            fuzzy_threshold,
            &mut errors,
            true,
        );
    }
    errors
}

fn check_evidence(
    evidence: &Option<crate::pipeline::summary_v2::EvidenceAnchor>,
    id: &str,
    kind: ItemKind,
    transcript: &str,
    threshold: f32,
    out: &mut Vec<ValidationError>,
    required: bool,
) {
    match evidence {
        None => {
            if required {
                out.push(ValidationError::EvidenceQuoteMissing {
                    kind,
                    item_id: id.to_string(),
                });
            }
        }
        Some(ev) => {
            if ev.quote.trim().is_empty() {
                out.push(ValidationError::EvidenceQuoteEmpty {
                    kind,
                    item_id: id.to_string(),
                });
                return;
            }
            let score = substring_fuzzy_score(&ev.quote, transcript);
            if score < threshold {
                out.push(ValidationError::EvidenceQuoteNotFound {
                    kind,
                    item_id: id.to_string(),
                    quote: ev.quote.clone(),
                    fuzzy_score: score,
                });
            }
        }
    }
}

/// Drop items с **фабрикованной** evidence-цитатой (present-but-not-found),
/// instead of fail entire summary. Returns (kept_summary, dropped_count).
/// Caller обычно log'ит count в telemetry.
///
/// **Items БЕЗ evidence (`None`) сохраняются.** Отсутствие цитаты ≠ галлюцинация:
/// у local-движка (Qwen) и любого v1→v2 promotion (`promote_legacy_to_v2`)
/// evidence отсутствует by design — раньше это удаляло ВСЕ задачи/решения у
/// каждого локального звонка. Missing evidence уже сигналится как non-fatal
/// `validate_schema` warning + UI показывает confidence ●●○; молча удалять
/// реальные items нельзя. Стрипаем только цитаты, которых нет в транскрипте.
pub fn strip_unverified_evidence(
    mut summary: CallSummaryV2,
    transcript_text: &str,
    fuzzy_threshold: f32,
) -> (CallSummaryV2, usize) {
    let mut dropped = 0_usize;
    summary.action_items.retain(|ai| {
        let keep =
            ai.evidence.is_none() || evidence_ok(&ai.evidence, transcript_text, fuzzy_threshold);
        if !keep {
            dropped += 1;
        }
        keep
    });
    summary.decisions.retain(|d| {
        let keep =
            d.evidence.is_none() || evidence_ok(&d.evidence, transcript_text, fuzzy_threshold);
        if !keep {
            dropped += 1;
        }
        keep
    });
    summary.open_questions.retain(|q| {
        let keep =
            q.evidence.is_none() || evidence_ok(&q.evidence, transcript_text, fuzzy_threshold);
        if !keep {
            dropped += 1;
        }
        keep
    });
    (summary, dropped)
}

fn evidence_ok(
    evidence: &Option<crate::pipeline::summary_v2::EvidenceAnchor>,
    transcript: &str,
    threshold: f32,
) -> bool {
    let Some(ev) = evidence else { return false };
    if ev.quote.trim().is_empty() {
        return false;
    }
    substring_fuzzy_score(&ev.quote, transcript) >= threshold
}

/// Token-overlap Jaccard для intent matching. Lowercases + whitespace-splits;
/// returns intersection_size / union_size.
#[allow(dead_code)] // [M14] Будет вызываться через `dedup_items` в T-08 + T-10.
fn jaccard_token_overlap(a: &str, b: &str) -> f32 {
    let a_tokens: std::collections::HashSet<String> = a
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    let b_tokens: std::collections::HashSet<String> = b
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    if a_tokens.is_empty() && b_tokens.is_empty() {
        return 1.0;
    }
    let inter = a_tokens.intersection(&b_tokens).count();
    let union = a_tokens.union(&b_tokens).count();
    if union == 0 {
        return 0.0;
    }
    inter as f32 / union as f32
}

/// Dedup items в [`CallSummaryV2`] по intent overlap. Для каждого
/// (action_items / decisions / open_questions) — если Jaccard ≥ threshold,
/// keep first (с лучшим evidence_quote). Mutates in-place.
#[allow(dead_code)] // [M14] Будет вызываться из orchestrator в T-10.
pub fn dedup_items(summary: &mut CallSummaryV2) {
    summary.action_items = dedup_vec(
        std::mem::take(&mut summary.action_items),
        DEFAULT_DEDUP_THRESHOLD,
        |a, b| jaccard_token_overlap(&a.text, &b.text),
    );
    summary.decisions = dedup_vec(
        std::mem::take(&mut summary.decisions),
        DEFAULT_DEDUP_THRESHOLD,
        |a, b| jaccard_token_overlap(&a.text, &b.text),
    );
    summary.open_questions = dedup_vec(
        std::mem::take(&mut summary.open_questions),
        DEFAULT_DEDUP_THRESHOLD,
        |a, b| jaccard_token_overlap(&a.text, &b.text),
    );
}

#[allow(dead_code)] // [M14] Используется через `dedup_items`.
fn dedup_vec<T, F: Fn(&T, &T) -> f32>(items: Vec<T>, threshold: f32, sim: F) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        let dup = out.iter().any(|kept| sim(kept, &item) >= threshold);
        if !dup {
            out.push(item);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::summary_v2::{
        ActionItemCategory, ActionItemV2, CallSummaryV2, CallType, Decision, EvidenceAnchor,
        OpenQuestion,
    };

    fn base_summary() -> CallSummaryV2 {
        CallSummaryV2 {
            schema_version: 2,
            title: "Q3 Planning".into(),
            summary: "Discussed Q3 roadmap.".into(),
            key_points: vec!["a".into(), "b".into(), "c".into()],
            mom: "## A\n- B".into(),
            language: "en".into(),
            call_type: CallType::ProductSync,
            call_type_confidence: 0.9,
            participants: vec![],
            action_items: vec![],
            decisions: vec![],
            open_questions: vec![],
            type_specific_block: None,
        }
    }

    fn ai(id: &str, text: &str, quote: Option<&str>) -> ActionItemV2 {
        ActionItemV2 {
            id: id.into(),
            text: text.into(),
            owner_hint: None,
            owner_confidence: None,
            due: None,
            due_confidence: None,
            category: ActionItemCategory::Commitment,
            evidence: quote.map(|q| EvidenceAnchor {
                quote: q.into(),
                ..Default::default()
            }),
        }
    }

    // ──────── substring_fuzzy_score ────────

    #[test]
    fn literal_substring_match_returns_1() {
        let score = substring_fuzzy_score(
            "I'll send the proposal",
            "Customer: yes please. I'll send the proposal by tomorrow.",
        );
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn whitespace_difference_still_near_1() {
        // Cloud LLM может вернуть quote с одинарным пробелом, transcript — с
        // двойным. Normalize должен collapse'нуть.
        let score = substring_fuzzy_score("hello world", "alice: hello  world  back");
        assert!(score >= 0.99, "got {score}");
    }

    #[test]
    fn case_difference_normalized() {
        let score = substring_fuzzy_score("Hello World", "alice: hello world back");
        assert!(score >= 0.99, "got {score}");
    }

    #[test]
    fn pure_hallucination_low_score() {
        let score = substring_fuzzy_score(
            "we agreed to ship enterprise tier",
            "Alice: just here to say hi and goodbye.",
        );
        // Common tokens: zero. Полностью разные тексты.
        assert!(score < 0.5, "got {score}");
    }

    #[test]
    fn fuzzy_threshold_boundary_typo_passes() {
        // 1 typo на 24 chars = ~96% similarity → passes 0.90 threshold.
        let needle = "I will send the report"; // 22 chars
        let transcript = "Bob: yeah, I willl send the report tonight."; // typo: willl vs will
        let score = substring_fuzzy_score(needle, transcript);
        assert!(score >= 0.90, "expected ≥ 0.90, got {score}");
    }

    // ──────── verify_evidence_quotes ────────

    #[test]
    fn verify_missing_evidence_flags_error() {
        let mut s = base_summary();
        s.action_items.push(ai("a1", "do thing", None));
        let errs = verify_evidence_quotes(&s, "transcript", DEFAULT_FUZZY_THRESHOLD);
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            errs[0],
            ValidationError::EvidenceQuoteMissing { .. }
        ));
    }

    #[test]
    fn verify_good_evidence_no_errors() {
        let mut s = base_summary();
        s.action_items.push(ai(
            "a1",
            "send report",
            Some("I'll send the report tomorrow"),
        ));
        let transcript = "Alice: I'll send the report tomorrow, sounds good.";
        let errs = verify_evidence_quotes(&s, transcript, DEFAULT_FUZZY_THRESHOLD);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn verify_hallucinated_evidence_flags_not_found() {
        let mut s = base_summary();
        s.action_items.push(ai(
            "a1",
            "deliver xyz",
            Some("absolutely fabricated quote not in transcript"),
        ));
        let transcript = "Alice: hi there. Bob: hello.";
        let errs = verify_evidence_quotes(&s, transcript, DEFAULT_FUZZY_THRESHOLD);
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            errs[0],
            ValidationError::EvidenceQuoteNotFound { .. }
        ));
    }

    // ──────── strip_unverified_evidence ────────

    #[test]
    fn strip_drops_only_failing_items_keeps_rest() {
        let mut s = base_summary();
        s.action_items
            .push(ai("good", "x", Some("I'll do it tomorrow")));
        s.action_items
            .push(ai("bad", "y", Some("totally made up quote")));
        s.action_items
            .push(ai("good2", "z", Some("call back next week")));
        let transcript = "Alice: I'll do it tomorrow. Bob: call back next week.";
        let (stripped, dropped) = strip_unverified_evidence(s, transcript, DEFAULT_FUZZY_THRESHOLD);
        assert_eq!(stripped.action_items.len(), 2);
        assert_eq!(dropped, 1);
        let ids: Vec<&str> = stripped
            .action_items
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert!(ids.contains(&"good"));
        assert!(ids.contains(&"good2"));
    }

    #[test]
    fn strip_keeps_items_without_evidence() {
        // [Fix A] v1→v2 promotion (Qwen local path) даёт items с evidence=None.
        // Раньше strip удалял их все → 0 задач у каждого локального звонка.
        // Теперь сохраняются; стрипается только present-but-not-found.
        let mut s = base_summary();
        s.action_items.push(ai("noev1", "x", None));
        s.action_items.push(ai("noev2", "y", None));
        s.action_items
            .push(ai("fabricated", "z", Some("quote nowhere in transcript")));
        let transcript = "Alice: unrelated chatter here.";
        let (stripped, dropped) = strip_unverified_evidence(s, transcript, DEFAULT_FUZZY_THRESHOLD);
        // Оба None-evidence сохранены; только фабрикованный удалён.
        assert_eq!(dropped, 1, "только present-but-not-found стрипается");
        let ids: Vec<&str> = stripped
            .action_items
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert!(ids.contains(&"noev1"));
        assert!(ids.contains(&"noev2"));
        assert!(!ids.contains(&"fabricated"));
    }

    // ──────── validate_schema ────────

    #[test]
    fn schema_empty_title_flagged() {
        let mut s = base_summary();
        s.title = "   ".into();
        let errs = validate_schema(&s);
        assert!(matches!(errs.first(), Some(ValidationError::EmptyTitle)));
    }

    #[test]
    fn schema_confidence_out_of_range_flagged() {
        let mut s = base_summary();
        s.call_type_confidence = 1.5;
        let errs = validate_schema(&s);
        assert!(matches!(
            errs.first(),
            Some(ValidationError::ConfidenceOutOfRange { .. })
        ));
    }

    #[test]
    fn schema_key_points_too_few_flagged() {
        let mut s = base_summary();
        s.key_points = vec!["only one".into()];
        let errs = validate_schema(&s);
        assert!(matches!(
            errs.first(),
            Some(ValidationError::KeyPointsCountOutOfRange { count: 1 })
        ));
    }

    // ──────── dedup_items ────────

    #[test]
    fn dedup_same_intent_action_items() {
        let mut s = base_summary();
        s.action_items
            .push(ai("a1", "follow up with Alice on Tuesday", None));
        s.action_items
            .push(ai("a2", "follow up with Alice on Tuesday", None));
        s.action_items
            .push(ai("a3", "ship release on Friday", None));
        dedup_items(&mut s);
        assert_eq!(s.action_items.len(), 2);
    }

    #[test]
    fn dedup_keeps_distinct_decisions() {
        let mut s = base_summary();
        s.decisions.push(Decision {
            id: "d1".into(),
            text: "Lock enterprise tier at $499".into(),
            evidence: None,
            confidence: None,
        });
        s.decisions.push(Decision {
            id: "d2".into(),
            text: "Launch beta next week".into(),
            evidence: None,
            confidence: None,
        });
        dedup_items(&mut s);
        assert_eq!(s.decisions.len(), 2);
    }

    #[test]
    fn dedup_open_questions_jaccard_threshold() {
        let mut s = base_summary();
        s.open_questions.push(OpenQuestion {
            id: "q1".into(),
            text: "should we offer a free trial".into(),
            raised_by: None,
            evidence: None,
        });
        s.open_questions.push(OpenQuestion {
            id: "q2".into(),
            // 5/6 tokens overlap with q1 → Jaccard ≈ 0.83 → merged.
            text: "should we offer a free demo".into(),
            raised_by: None,
            evidence: None,
        });
        dedup_items(&mut s);
        assert_eq!(s.open_questions.len(), 1);
    }
}
