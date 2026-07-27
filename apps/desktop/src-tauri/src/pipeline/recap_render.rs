//! [M14] Рендер `recap.md` из `CallSummaryV2` и локализованные подписи секций.
//!
//! [TD-41] Выделено из `pipeline/recap.rs` (1135 строк при лимите 800,
//! правило 8) вместе с тестами. Граница естественная: здесь только сборка
//! markdown из уже готовой структуры — ни LLM, ни базы. Соседний
//! `recap_md.rs` занимается пост-обработкой готового markdown (выделение
//! имён). Логика не менялась.

use crate::db::{self, ActionItemInput};
use crate::pipeline::{recap_md, summary_v2::CallSummaryV2};

/// [M14 T-02] Расширенный render для CallSummaryV2 — добавляет
/// ## Решения / Decisions + ## Открытые вопросы / Open questions секции
/// + category badges + evidence quotes как blockquotes.
///
/// Localization: labels подбираются по `summary.language` (ru/en/kk → ru/en/kk
/// localized; иначе ru fallback).
/// Семантически-пустой recap.md: только heading-строки (`# …`) и пробелы, без
/// тела. v2 render всегда даёт «# Рекап\n\n», поэтому до-фиксный пустой рекап =
/// `"# Рекап\n\n"` (строка непустая). Используется bulk-регеном чтобы найти
/// звонки требующие пересоздания. Mirror TS `isMarkdownBlank`.
pub(crate) fn recap_md_is_blank(md: &str) -> bool {
    !md.lines().any(|line| {
        let t = line.trim();
        !t.is_empty() && !is_md_heading(t)
    })
}

/// [recap-rich] Плейсхолдер вместо имени от слабой модели — не рендерим.
fn is_placeholder_name(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "unknown" | "null" | "none" | "n/a" | "не указано" | "неизвестно" | "белгісіз"
    )
}

fn is_md_heading(trimmed: &str) -> bool {
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes)
        && trimmed[hashes..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
}

pub(crate) fn render_recap_md_v2(
    summary: &CallSummaryV2,
    contacts: &[db::Contact],
    action_inputs: &[ActionItemInput],
) -> String {
    let labels = RecapLabels::for_lang(&summary.language);
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", labels.title));

    // [B20.3] Известные имена (участники + контакты) для render-side bold.
    // Плейсхолдеры («unknown»/«не указано») не выделяем.
    let known_names: Vec<String> = summary
        .participants
        .iter()
        .filter_map(|p| p.display_name.clone())
        .chain(contacts.iter().map(|c| c.display_name.clone()))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty() && !is_placeholder_name(n))
        .collect();

    // [recap-rich] Вверху — нарратив-минутки (prose) если есть; иначе короткий
    // summary. Оба сразу не рендерим (нарратив уже включает суть).
    // Bold применяем только к summary-фоллбеку: нарратив выделяет имена сам
    // (правило в narrative-промпте), двойная разметка не нужна.
    let lead = if !summary.narrative.trim().is_empty() {
        summary.narrative.trim().to_string()
    } else {
        recap_md::bold_known_names(summary.summary.trim(), &known_names)
    };
    if !lead.is_empty() {
        out.push_str(&lead);
        out.push_str("\n\n");
    }

    if !summary.key_points.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.key_points));
        for kp in &summary.key_points {
            out.push_str(&format!(
                "- {}\n",
                recap_md::bold_known_names(kp.trim(), &known_names)
            ));
        }
        out.push('\n');
    }

    // [recap-rich] Темы — обсуждённые темы с под-пунктами.
    if !summary.topics.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.topics));
        for t in &summary.topics {
            if t.title.trim().is_empty() {
                continue;
            }
            out.push_str(&format!("### {}\n", t.title.trim()));
            for p in &t.points {
                if !p.trim().is_empty() {
                    out.push_str(&format!("- {}\n", p.trim()));
                }
            }
            out.push('\n');
        }
    }

    // [M14 T-02] Decisions section.
    if !summary.decisions.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.decisions));
        for d in &summary.decisions {
            out.push_str(&format!("- {}\n", d.text.trim()));
            if let Some(ev) = d.evidence.as_ref() {
                if !ev.quote.trim().is_empty() {
                    out.push_str(&format!("  > {}\n", ev.quote.trim()));
                }
            }
        }
        out.push('\n');
    }

    // [M14 T-02] Open questions section.
    if !summary.open_questions.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.open_questions));
        for q in &summary.open_questions {
            // [recap-rich] Слабая модель кладёт плейсхолдеры в raised_by
            // («unknown» / «не указано») — не печатаем такой суффикс.
            let by_suffix = q
                .raised_by
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty() && !is_placeholder_name(b))
                .map(|b| format!(" ({b})"))
                .unwrap_or_default();
            out.push_str(&format!("- {}{}\n", q.text.trim(), by_suffix));
            if let Some(ev) = q.evidence.as_ref() {
                if !ev.quote.trim().is_empty() {
                    out.push_str(&format!("  > {}\n", ev.quote.trim()));
                }
            }
        }
        out.push('\n');
    }

    // [MoM cleanup] `mom` НЕ рендерим: слабая локальная модель эхо-копировала
    // сюда инструкции промпта (## Status by workstream + type_specific_block
    // schema + raw JSON) → мусор в рекапе. Промпты больше не просят mom/tsb.

    if !action_inputs.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.tasks));
        for (i, ai) in action_inputs.iter().enumerate() {
            let owner_label = ai
                .owner_contact_id
                .as_deref()
                .and_then(|id| contacts.iter().find(|c| c.id == id))
                .map(|c| c.display_name.clone())
                .or_else(|| {
                    summary
                        .action_items
                        .get(i)
                        .and_then(|r| r.owner_hint.clone())
                });
            let due_suffix = ai
                .due
                .as_deref()
                .map(|d| format!(" — {} {d}", labels.until))
                .unwrap_or_default();
            // [B20.2] Категория — локализованный inline-code лейбл (рендерится
            // как `.md-code`-чип в v2 UI). Emoji выпилены по design-gate (канон
            // wotold-v2 — line-иконки/чипы, без emoji); старые сохранённые
            // recap.md с emoji остаются валидными — рендерер их не переписывает.
            let category_prefix = ai
                .category
                .as_deref()
                .map(|c| match c {
                    "commitment" => format!("`{}` ", labels.cat_commitment),
                    "proposal" => format!("`{}` ", labels.cat_proposal),
                    "idea" => format!("`{}` ", labels.cat_idea),
                    _ => String::new(),
                })
                .unwrap_or_default();
            match owner_label {
                Some(label) if !label.trim().is_empty() => {
                    out.push_str(&format!(
                        "- [ ] {}**{}** — {}{}\n",
                        category_prefix,
                        label.trim(),
                        ai.text.trim(),
                        due_suffix
                    ));
                }
                _ => {
                    out.push_str(&format!(
                        "- [ ] {}{}{}\n",
                        category_prefix,
                        ai.text.trim(),
                        due_suffix
                    ));
                }
            }
            if let Some(ev) = ai.evidence_quote.as_deref() {
                if !ev.trim().is_empty() {
                    out.push_str(&format!("  > {}\n", ev.trim()));
                }
            }
        }
        out.push('\n');
    }

    if !summary.participants.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.participants));
        for p in &summary.participants {
            let name = p
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            match name {
                Some(n) if n != p.speaker_tag => {
                    out.push_str(&format!("- {} (`{}`)\n", n, p.speaker_tag));
                }
                _ => {
                    out.push_str(&format!("- `{}`\n", p.speaker_tag));
                }
            }
        }
        out.push('\n');
    }

    out
}

/// Локализованные labels для секций recap.md. Lang detection из summary.language.
struct RecapLabels {
    title: &'static str,
    key_points: &'static str,
    topics: &'static str,
    decisions: &'static str,
    open_questions: &'static str,
    tasks: &'static str,
    participants: &'static str,
    until: &'static str,
    cat_commitment: &'static str,
    cat_proposal: &'static str,
    cat_idea: &'static str,
}

impl RecapLabels {
    fn for_lang(lang: &str) -> Self {
        match lang {
            "en" => Self {
                title: "Recap",
                key_points: "Key points",
                topics: "Topics",
                decisions: "Decisions",
                open_questions: "Open questions",
                tasks: "Tasks",
                participants: "Participants",
                until: "by",
                cat_commitment: "Commitment",
                cat_proposal: "Proposal",
                cat_idea: "Idea",
            },
            "kk" => Self {
                title: "Қорытынды",
                key_points: "Негізгі тармақтар",
                topics: "Тақырыптар",
                decisions: "Шешімдер",
                open_questions: "Ашық сұрақтар",
                tasks: "Тапсырмалар",
                participants: "Қатысушылар",
                until: "мерзім:",
                cat_commitment: "Уәде",
                cat_proposal: "Ұсыныс",
                cat_idea: "Идея",
            },
            _ => Self {
                title: "Рекап",
                key_points: "Ключевое",
                topics: "Темы",
                decisions: "Решения",
                open_questions: "Открытые вопросы",
                tasks: "Задачи",
                participants: "Участники",
                until: "до",
                cat_commitment: "Договорённость",
                cat_proposal: "Предложение",
                cat_idea: "Идея",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::summary_v2::{ActionItemCategory, CallType};

    fn contact(id: &str, name: &str) -> db::Contact {
        db::Contact {
            id: id.to_string(),
            display_name: name.to_string(),
            is_owner: false,
            org: None,
            role: None,
            attributes: serde_json::Value::Object(serde_json::Map::new()),
            notes: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            source: "local".into(),
            external_id: None,
            external_etag: None,
            identifiers: vec![],
        }
    }

    /// [M14 T-02] Helper для построения minimal CallSummaryV2 в tests.
    fn empty_summary_v2(lang: &str) -> CallSummaryV2 {
        CallSummaryV2 {
            schema_version: 2,
            title: String::new(),
            summary: String::new(),
            key_points: vec![],
            mom: String::new(),
            language: lang.into(),
            call_type: CallType::Other,
            call_type_confidence: 0.0,
            participants: vec![],
            action_items: vec![],
            decisions: vec![],
            open_questions: vec![],
            topics: Vec::new(),
            narrative: String::new(),
            type_specific_block: None,
        }
    }

    #[test]
    fn render_recap_md_v2_skips_empty_sections() {
        let mut s = empty_summary_v2("ru");
        s.summary = "Brief".into();
        let md = render_recap_md_v2(&s, &[], &[]);
        assert!(md.contains("# Рекап"));
        assert!(md.contains("Brief"));
        assert!(!md.contains("## Ключевое"));
        assert!(!md.contains("## Решения"));
        assert!(!md.contains("## Открытые вопросы"));
        assert!(!md.contains("## Задачи"));
    }

    #[test]
    fn recap_md_is_blank_detects_header_only() {
        // Старый до-фиксный пустой рекап = «# Рекап\n\n».
        assert!(recap_md_is_blank("# Рекап\n\n"));
        assert!(recap_md_is_blank("# Рекап"));
        assert!(recap_md_is_blank("  \n## A\n\n"));
        assert!(recap_md_is_blank(""));
        // С телом — не blank.
        assert!(!recap_md_is_blank("# Рекап\n\nКоманда обсудила релиз."));
        assert!(!recap_md_is_blank("# Рекап\n\n## Ключевое\n- пункт"));
        // render пустого summary с одним полем → не blank.
        let mut s = empty_summary_v2("ru");
        s.summary = "Brief".into();
        assert!(!recap_md_is_blank(&render_recap_md_v2(&s, &[], &[])));
        // render полностью пустого summary → blank (как старые звонки).
        let empty = empty_summary_v2("ru");
        assert!(recap_md_is_blank(&render_recap_md_v2(&empty, &[], &[])));
    }

    #[test]
    fn render_recap_md_v2_does_not_render_mom() {
        // [MoM cleanup] Даже если модель положила мусор в mom (эхо схемы) —
        // он НЕ попадает в recap.md.
        let mut s = empty_summary_v2("ru");
        s.summary = "Краткое содержание.".into();
        s.mom =
            "## Status by workstream / ## Risks / ## Asks\n\n### type_specific_block schema\n{\"workstreams\":[]}"
                .into();
        let md = render_recap_md_v2(&s, &[], &[]);
        assert!(md.contains("Краткое содержание."));
        assert!(!md.contains("Status by workstream"));
        assert!(!md.contains("type_specific_block"));
        assert!(!md.contains("workstreams"));
    }

    #[test]
    fn render_recap_md_v2_renders_action_items_with_owner_label() {
        let contacts = vec![contact("a", "Alice")];
        let mut s = empty_summary_v2("ru");
        s.title = "Q3 plan review".into();
        s.summary = "Discussed Q3.".into();
        s.key_points = vec!["plan reviewed".into()];
        s.action_items = vec![crate::pipeline::summary_v2::ActionItemV2 {
            id: "ai-1".into(),
            text: "send draft".into(),
            owner_hint: Some("Alice".into()),
            owner_confidence: Some(0.95),
            due: Some("2026-06-01".into()),
            due_confidence: Some(0.8),
            category: ActionItemCategory::Commitment,
            evidence: None,
        }];
        s.participants = vec![crate::pipeline::summary_v2::ParticipantV2 {
            speaker_tag: "Speaker 0".into(),
            display_name: Some("Alice".into()),
            role_hint: None,
        }];
        let action_inputs = vec![ActionItemInput {
            text: "send draft".into(),
            owner_contact_id: Some("a".into()),
            due: Some("2026-06-01".into()),
            category: Some("commitment".into()),
            ..Default::default()
        }];
        let md = render_recap_md_v2(&s, &contacts, &action_inputs);
        assert!(md.contains("## Задачи"));
        // [M14 T-02, B20.2] category — локализованный `код-лейбл` перед owner.
        assert!(md.contains("`Договорённость` **Alice** — send draft — до 2026-06-01"));
        assert!(md.contains("## Участники"));
        assert!(md.contains("Alice (`Speaker 0`)"));
    }

    #[test]
    fn render_recap_md_v2_includes_decisions_and_open_questions_sections() {
        let mut s = empty_summary_v2("ru");
        s.summary = "Brief".into();
        s.decisions = vec![crate::pipeline::summary_v2::Decision {
            id: "d1".into(),
            text: "Lock enterprise tier at $499".into(),
            evidence: Some(crate::pipeline::summary_v2::EvidenceAnchor {
                quote: "we agreed on 499 dollars".into(),
                speaker: Some("Alice".into()),
                start_ms: None,
                end_ms: None,
            }),
            confidence: Some(0.9),
        }];
        s.open_questions = vec![crate::pipeline::summary_v2::OpenQuestion {
            id: "q1".into(),
            text: "Should we offer a trial?".into(),
            raised_by: Some("Bob".into()),
            evidence: None,
        }];
        let md = render_recap_md_v2(&s, &[], &[]);
        assert!(md.contains("## Решения"));
        assert!(md.contains("Lock enterprise tier at $499"));
        assert!(md.contains("> we agreed on 499 dollars"));
        assert!(md.contains("## Открытые вопросы"));
        assert!(md.contains("Should we offer a trial? (Bob)"));
    }

    #[test]
    fn render_recap_md_v2_language_en_uses_english_labels() {
        let mut s = empty_summary_v2("en");
        s.summary = "Brief".into();
        s.key_points = vec!["a".into(), "b".into(), "c".into()];
        let md = render_recap_md_v2(&s, &[], &[]);
        assert!(md.contains("# Recap"));
        assert!(md.contains("## Key points"));
    }
}
