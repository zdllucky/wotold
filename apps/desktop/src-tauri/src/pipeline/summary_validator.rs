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
//! **Fuzzy substring match** живёт в соседнем [`text_similarity`] — вынесен
//! туда при TD-18 вместе с Jaccard-дедупом, когда этот файл упёрся в лимит
//! когезии 800 строк.

pub mod text_similarity;

use std::collections::HashSet;

use crate::pipeline::summary_v2::{CallSummaryV2, ParticipantV2};
use text_similarity::{jaccard_token_overlap, substring_fuzzy_score};

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
    // [recap-rich] НЕ дропаем пункт при недостоверной цитате — текст решения/
    // задачи/вопроса реален, слабая local-модель лишь перефразирует цитату.
    // Обнуляем только цитату (`evidence = None`), пункт остаётся. Дропаем лишь
    // пункты с пустым `text` (мусор). Возвращаемое число = сколько цитат обнулено.
    let mut stripped = 0_usize;
    for ai in &mut summary.action_items {
        if ai.evidence.is_some() && !evidence_ok(&ai.evidence, transcript_text, fuzzy_threshold) {
            ai.evidence = None;
            stripped += 1;
        }
    }
    for d in &mut summary.decisions {
        if d.evidence.is_some() && !evidence_ok(&d.evidence, transcript_text, fuzzy_threshold) {
            d.evidence = None;
            stripped += 1;
        }
    }
    for q in &mut summary.open_questions {
        if q.evidence.is_some() && !evidence_ok(&q.evidence, transcript_text, fuzzy_threshold) {
            q.evidence = None;
            stripped += 1;
        }
    }
    summary.action_items.retain(|ai| !ai.text.trim().is_empty());
    summary.decisions.retain(|d| !d.text.trim().is_empty());
    summary.open_questions.retain(|q| !q.text.trim().is_empty());
    (summary, stripped)
}

/// [TD-18] Async-обёртка «сверить цитаты + схлопнуть дубли» для пайплайна.
///
/// Обе операции — Левенштейн и Jaccard по всему транскрипту, то есть чистый
/// CPU на десятки-сотни миллисекунд (часовой звонок × десяток пунктов). На
/// tokio-worker'ах крутятся Tauri-команды UI, поэтому считаем в blocking-пуле
/// (инженерное правило 5). Обёртка живёт здесь, а не на call-site: перенос в
/// пул — свойство самого алгоритма, и следующий вызывающий не должен об этом
/// вспоминать.
///
/// Синхронные `strip_unverified_evidence` / `dedup_items` остаются публичными
/// для golden-eval харнеса, который работает вне async-контекста.
pub async fn strip_and_dedup(
    summary: CallSummaryV2,
    transcript: &str,
    fuzzy_threshold: f32,
) -> Result<(CallSummaryV2, usize), crate::AppError> {
    let transcript = transcript.to_string();
    tokio::task::spawn_blocking(move || {
        let (mut summary, nulled) =
            strip_unverified_evidence(summary, &transcript, fuzzy_threshold);
        dedup_items(&mut summary);
        (summary, nulled)
    })
    .await
    .map_err(|e| crate::AppError::Other(format!("summary validator task join: {e}")))
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
    dedup_participants(&mut summary.participants);
}

/// [recap-fix A3] Dedup участников. Слабая local-модель повторяет одно имя на
/// нескольких speaker-тегах («Глеб Гусак» на speaker:0/1/unknown ×N) и дублит
/// один тег (speaker:unknown ×3). Чистим по двум ключам, сохраняя порядок:
///   1. уникальный `speaker_tag` (case-insensitive) — убирает дубли тега;
///   2. уникальный НЕпустой `display_name` — одно имя = один человек (пустое
///      имя / только-тег не коллапсим, иначе схлопнули бы всех безымянных).
fn dedup_participants(participants: &mut Vec<ParticipantV2>) {
    let mut seen_tags: HashSet<String> = HashSet::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let kept = std::mem::take(participants);
    *participants = kept
        .into_iter()
        .filter(|p| {
            if !seen_tags.insert(p.speaker_tag.trim().to_lowercase()) {
                return false;
            }
            if let Some(name) = p
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if !seen_names.insert(name.to_lowercase()) {
                    return false;
                }
            }
            true
        })
        .collect();
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
        OpenQuestion, ParticipantV2,
    };

    fn part(tag: &str, name: Option<&str>) -> ParticipantV2 {
        ParticipantV2 {
            speaker_tag: tag.into(),
            display_name: name.map(Into::into),
            role_hint: None,
        }
    }

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
            topics: Vec::new(),
            narrative: String::new(),
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

    #[test]
    fn dedup_participants_collapses_repeated_name_and_tag() {
        // Реальный кейс из лога: одно имя на 5 тегах + speaker:unknown ×3.
        let mut s = base_summary();
        s.participants = vec![
            part("speaker:1", Some("Глеб Гусак")),
            part("speaker:unknown", Some("Глеб Гусак")),
            part("owner", Some("Дамир")),
            part("speaker:0", Some("Глеб Гусак")),
            part("speaker:unknown", Some("Глеб Гусак")),
            part("speaker:unknown", Some("Глеб Гусак")),
        ];
        dedup_items(&mut s);
        // Остаётся первый «Глеб Гусак» + Дамир.
        assert_eq!(s.participants.len(), 2);
        assert_eq!(s.participants[0].speaker_tag, "speaker:1");
        assert_eq!(
            s.participants[0].display_name.as_deref(),
            Some("Глеб Гусак")
        );
        assert_eq!(s.participants[1].display_name.as_deref(), Some("Дамир"));
    }

    #[test]
    fn dedup_participants_keeps_distinct_unnamed_tags() {
        // Безымянные (только-тег) участники НЕ коллапсируются между собой.
        let mut s = base_summary();
        s.participants = vec![
            part("speaker:0", None),
            part("speaker:1", None),
            part("speaker:0", None), // дубль тега → уходит
        ];
        dedup_items(&mut s);
        assert_eq!(s.participants.len(), 2);
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
    fn strip_nulls_bad_quote_but_keeps_item() {
        // [recap-rich] Недостоверная цитата → пункт СОХРАНЯЕТСЯ, обнуляется
        // только evidence. Раньше пункт дропался целиком → пустые секции.
        let mut s = base_summary();
        s.action_items
            .push(ai("good", "x", Some("I'll do it tomorrow")));
        s.action_items
            .push(ai("bad", "y", Some("totally made up quote")));
        s.action_items
            .push(ai("good2", "z", Some("call back next week")));
        let transcript = "Alice: I'll do it tomorrow. Bob: call back next week.";
        let (stripped, nulled) = strip_unverified_evidence(s, transcript, DEFAULT_FUZZY_THRESHOLD);
        // Все 3 пункта на месте; у "bad" evidence обнулён.
        assert_eq!(stripped.action_items.len(), 3);
        assert_eq!(nulled, 1);
        let bad = stripped
            .action_items
            .iter()
            .find(|a| a.id == "bad")
            .unwrap();
        assert!(bad.evidence.is_none(), "недостоверная цитата обнулена");
        let good = stripped
            .action_items
            .iter()
            .find(|a| a.id == "good")
            .unwrap();
        assert!(good.evidence.is_some(), "достоверная цитата сохранена");
    }

    #[test]
    fn strip_keeps_items_without_evidence() {
        // Items с evidence=None (v1→v2 promotion) сохраняются как есть.
        let mut s = base_summary();
        s.action_items.push(ai("noev1", "x", None));
        s.action_items.push(ai("noev2", "y", None));
        s.action_items
            .push(ai("fabricated", "z", Some("quote nowhere in transcript")));
        let transcript = "Alice: unrelated chatter here.";
        let (stripped, nulled) = strip_unverified_evidence(s, transcript, DEFAULT_FUZZY_THRESHOLD);
        // Все 3 пункта сохранены; у "fabricated" цитата обнулена.
        assert_eq!(stripped.action_items.len(), 3);
        assert_eq!(nulled, 1, "только present-but-not-found цитата обнуляется");
        let ids: Vec<&str> = stripped
            .action_items
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert!(ids.contains(&"noev1"));
        assert!(ids.contains(&"noev2"));
        assert!(ids.contains(&"fabricated"));
    }

    #[test]
    fn strip_drops_empty_text_items() {
        let mut s = base_summary();
        s.action_items.push(ai("keep", "real task", None));
        s.action_items.push(ai("empty", "   ", None));
        let (stripped, _) = strip_unverified_evidence(s, "x", DEFAULT_FUZZY_THRESHOLD);
        assert_eq!(stripped.action_items.len(), 1);
        assert_eq!(stripped.action_items[0].id, "keep");
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
