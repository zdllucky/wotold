//! [M15.3] Построение пассажей ассистента: transcript.md, recap.md и
//! structured-строки → куски текста для FTS и эмбеддингов.
//!
//! [TD-41] Выделено из `assistant/indexer.rs` (1087 строк при лимите 800,
//! правило 8) вместе с тестами. Граница естественная: здесь чистый разбор и
//! нарезка текста — ни базы, ни файловой системы, ни эмбеддера. В `indexer`
//! осталась оркестрация: чтение артефактов, запись пассажей, backfill.
//! Логика не менялась.

use crate::assistant::types::AssistantPassageKind;
use crate::db::assistant::PassageInput;
use crate::pipeline::chunker::estimate_tokens;

/// Целевой размер транскрипт-пассажа (окно speaker-turn'ов), в токенах.
/// ~350 ток ≈ 1.4KB кириллицы — 12-16 пассажей входят в retrieval-бюджет 5.5K.
const TRANSCRIPT_PASSAGE_TARGET_TOKENS: usize = 350;

/// Одна реплика транскрипта (заголовок `**{tag}** [{m}:{ss}]:` + текст).
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub speaker_tag: String,
    pub start_ms: i64,
    pub text: String,
}

/// Парс transcript.md → реплики. Формат из `merge.rs::render_transcript_md`:
/// строка-заголовок `**owner** [0:02]:` (закрывающие `**` ДО таймкода),
/// затем строки текста до следующего заголовка. Битые заголовки скипаются.
pub fn parse_transcript_turns(md: &str) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    let mut current: Option<Turn> = None;
    for line in md.lines() {
        if is_speaker_header_line(line) {
            if let Some(t) = current.take() {
                if !t.text.trim().is_empty() {
                    turns.push(t);
                }
            }
            current = parse_header_line(line);
            if current.is_none() {
                // Инвариант: системные теги (owner/Speaker N) не содержат `**`.
                // Битый заголовок роняет свой блок — оставляем след в логе.
                log::debug!("assistant indexer: unparsable header line skipped: {line:?}");
            }
            continue;
        }
        if let Some(t) = current.as_mut() {
            if !line.trim().is_empty() {
                if !t.text.is_empty() {
                    t.text.push(' ');
                }
                t.text.push_str(line.trim());
            }
        }
    }
    if let Some(t) = current.take() {
        if !t.text.trim().is_empty() {
            turns.push(t);
        }
    }
    turns
}

// Тот же критерий что chunker.rs::is_speaker_header_line — единый формат.
fn is_speaker_header_line(line: &str) -> bool {
    line.starts_with("**") && line.contains("]:")
}

/// `**{tag}** [{m}:{ss}]:` → (tag, ms). None если строка не парсится.
fn parse_header_line(line: &str) -> Option<Turn> {
    let rest = line.strip_prefix("**")?;
    let (tag, after_tag) = rest.split_once("**")?;
    let after_tag = after_tag.trim_start();
    let ts = after_tag.strip_prefix('[')?;
    let (clock, _) = ts.split_once("]:")?;
    let (min, sec) = clock.trim().split_once(':')?;
    let min: i64 = min.trim().parse().ok()?;
    let sec: i64 = sec.trim().parse().ok()?;
    if !(0..60).contains(&sec) || min < 0 {
        return None;
    }
    Some(Turn {
        speaker_tag: tag.trim().to_string(),
        start_ms: (min * 60 + sec) * 1000,
        text: String::new(),
    })
}

/// [M16.6] Резолв speaker-тега в имя: подтверждённая привязка → display_name,
/// иначе сырой тег. Имя попадает и в поле speaker, и в текст пассажа —
/// «что говорил Дамир» начинает матчить FTS, а не только устные упоминания.
fn resolve_speaker<'a>(
    names: &'a std::collections::HashMap<String, String>,
    tag: &'a str,
) -> &'a str {
    names.get(tag).map(String::as_str).unwrap_or(tag)
}

/// Окна последовательных реплик до ~350 ток, overlap = 1 реплика.
/// speaker/start_ms — от первой реплики окна; end_ms = start следующего окна.
/// [M16.6] `names`: speaker_tag → подтверждённое имя контакта.
pub fn build_transcript_passages(
    turns: &[Turn],
    names: &std::collections::HashMap<String, String>,
) -> Vec<PassageInput> {
    let mut windows: Vec<(usize, usize)> = Vec::new(); // [from, to) по turns
    let mut from = 0usize;
    while from < turns.len() {
        let mut to = from;
        let mut tokens = 0usize;
        while to < turns.len() {
            let t = estimate_tokens(&turns[to].text);
            if to > from && tokens + t > TRANSCRIPT_PASSAGE_TARGET_TOKENS {
                break;
            }
            tokens += t;
            to += 1;
        }
        windows.push((from, to));
        if to >= turns.len() {
            break;
        }
        // Overlap: следующее окно стартует с последней реплики текущего —
        // но только для окон из ≥2 реплик, иначе курсор не двигается
        // (одиночная oversized-реплика зацикливала бы нарезку).
        from = if to - from > 1 { to - 1 } else { to };
    }

    windows
        .iter()
        .map(|&(a, b)| {
            let text = turns[a..b]
                .iter()
                .map(|t| format!("{}: {}", resolve_speaker(names, &t.speaker_tag), t.text))
                .collect::<Vec<_>>()
                .join("\n");
            let end_ms = turns.get(b).map(|next| next.start_ms);
            PassageInput {
                kind: AssistantPassageKind::Transcript,
                speaker: Some(resolve_speaker(names, &turns[a].speaker_tag).to_string()),
                start_ms: Some(turns[a].start_ms),
                end_ms,
                token_est: estimate_tokens(&text) as i64,
                text,
            }
        })
        .collect()
}

/// [M16.6] Синтетическая «карточка звонка»: титул + дата + участники.
/// Якорь для «в каком звонке / кто был / о чём» — раньше титулы и даты
/// вообще не индексировались.
pub fn build_call_meta_passage(
    title: Option<&str>,
    started_at: &str,
    participants: &[String],
) -> Option<PassageInput> {
    let date = started_at.get(..10).map(|d| {
        let mut it = d.split('-');
        match (it.next(), it.next(), it.next()) {
            (Some(y), Some(m), Some(day)) => format!("{day}.{m}.{y}"),
            _ => d.to_string(),
        }
    })?;
    let mut text = match title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => format!("Звонок «{t}» — {date}."),
        None => format!("Звонок от {date}."),
    };
    if !participants.is_empty() {
        text.push_str(&format!(" Участники: {}.", participants.join(", ")));
    }
    Some(PassageInput {
        kind: AssistantPassageKind::CallMeta,
        speaker: None,
        start_ms: None,
        end_ms: None,
        token_est: estimate_tokens(&text) as i64,
        text,
    })
}

/// recap.md → пассажи-абзацы. Заголовки (`#…`) скипаются, буллет-группы
/// между пустыми строками идут одним пассажем. start_ms = None.
pub fn build_recap_passages(md: &str) -> Vec<PassageInput> {
    // CRLF-нормализация: иначе `\r\n\r\n` не матчит разделитель абзацев.
    let md = md.replace("\r\n", "\n");
    md.split("\n\n")
        .map(|block| {
            block
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.trim().is_empty())
        .map(|text| PassageInput {
            kind: AssistantPassageKind::Recap,
            speaker: None,
            start_ms: None,
            end_ms: None,
            token_est: estimate_tokens(&text) as i64,
            text,
        })
        .collect()
}

/// Общая форма structured-строки: текст + опциональная цитата-evidence.
fn structured_passage(
    kind: AssistantPassageKind,
    text: &str,
    quote: Option<&str>,
    speaker: Option<&str>,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Option<PassageInput> {
    let base = text.trim();
    if base.is_empty() {
        return None;
    }
    let full = match quote.map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => format!("{base} — цитата: {q}"),
        None => base.to_string(),
    };
    Some(PassageInput {
        kind,
        speaker: speaker.map(str::to_string),
        start_ms,
        end_ms,
        token_est: estimate_tokens(&full) as i64,
        text: full,
    })
}

/// decisions / action_items / open_questions → по одному пассажу на строку.
pub fn build_structured_passages(
    decisions: &[crate::db::decisions::DecisionRow],
    action_items: &[crate::db::ActionItem],
    open_questions: &[crate::db::open_questions::OpenQuestionRow],
    names: &std::collections::HashMap<String, String>,
) -> Vec<PassageInput> {
    let mut out = Vec::new();
    for d in decisions {
        out.extend(structured_passage(
            AssistantPassageKind::Decision,
            &d.text,
            d.evidence_quote.as_deref(),
            d.evidence_speaker
                .as_deref()
                .map(|t| resolve_speaker(names, t)),
            d.evidence_start_ms,
            d.evidence_end_ms,
        ));
    }
    for a in action_items {
        out.extend(structured_passage(
            AssistantPassageKind::ActionItem,
            &a.text,
            a.evidence_quote.as_deref(),
            a.evidence_speaker
                .as_deref()
                .map(|t| resolve_speaker(names, t)),
            a.evidence_start_ms,
            None,
        ));
    }
    for q in open_questions {
        out.extend(structured_passage(
            AssistantPassageKind::OpenQuestion,
            &q.text,
            q.evidence_quote.as_deref(),
            q.evidence_speaker
                .as_deref()
                .map(|t| resolve_speaker(names, t)),
            q.evidence_start_ms,
            None,
        ));
    }
    out
}

/// Фикстура transcript.md для тестов индексера и пассажей: два спикера,
/// таймкод больше часа (73:20) — на нём ловились ошибки парсинга минут.
#[cfg(test)]
pub(crate) const SAMPLE_MD_FOR_TESTS: &str = "# Transcript\n\n\
**owner** [0:00]:\nДавайте сверимся по срокам пилота.\n\n\
**Speaker 0** [0:11]:\nПо нашей части всё в графике. Отчёт будет к пятнице.\n\n\
**Speaker 1** [1:05]:\nУ меня вопрос по разделению голосов.\n\n\
**owner** [73:20]:\nИтого — фиксируем решения.\n";

#[cfg(test)]
mod tests {
    use super::*;

    use super::SAMPLE_MD_FOR_TESTS as SAMPLE_MD;

    // ── parse_transcript_turns ──

    #[test]
    fn parses_real_transcript_format() {
        let turns = parse_transcript_turns(SAMPLE_MD);
        assert_eq!(turns.len(), 4);
        assert_eq!(turns[0].speaker_tag, "owner");
        assert_eq!(turns[0].start_ms, 0);
        assert_eq!(turns[0].text, "Давайте сверимся по срокам пилота.");
        assert_eq!(turns[1].speaker_tag, "Speaker 0");
        assert_eq!(turns[1].start_ms, 11_000);
        assert_eq!(turns[2].start_ms, 65_000);
        // Минуты без часов: [73:20] = 73*60+20.
        assert_eq!(turns[3].start_ms, (73 * 60 + 20) * 1000);
    }

    #[test]
    fn multiline_turn_text_is_joined() {
        let md = "**owner** [0:05]:\nпервая строка\nвторая строка\n";
        let turns = parse_transcript_turns(md);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].text, "первая строка вторая строка");
    }

    #[test]
    fn broken_or_empty_md_yields_no_turns() {
        assert!(parse_transcript_turns("").is_empty());
        assert!(parse_transcript_turns("# Transcript\n\nпросто текст без заголовков").is_empty());
        // Битый заголовок (нет таймкода) — скип вместе с текстом под ним.
        assert!(parse_transcript_turns("**owner** без таймкода:\nтекст\n").is_empty());
        // Невалидные секунды.
        assert!(parse_transcript_turns("**owner** [0:99]:\nтекст\n").is_empty());
    }

    // ── build_transcript_passages ──

    fn turn(tag: &str, start_ms: i64, len_bytes: usize) -> Turn {
        Turn {
            speaker_tag: tag.into(),
            start_ms,
            text: "д".repeat(len_bytes / 2), // кириллица = 2 байта/символ
        }
    }

    #[test]
    fn windows_respect_target_and_overlap() {
        // Каждая реплика ~150 ток (600 байт) → окно вмещает 2 (300 ≤ 350,
        // третья давала бы 450) → окна с overlap 1: [0,2], [1,3], [2,4].
        let turns: Vec<Turn> = (0..4).map(|i| turn("Speaker 0", i * 10_000, 600)).collect();
        let ps = build_transcript_passages(&turns, &std::collections::HashMap::new());
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[0].start_ms, Some(0));
        assert_eq!(ps[0].end_ms, Some(20_000)); // старт turn[2] (первого вне окна)
        assert_eq!(ps[1].start_ms, Some(10_000)); // overlap: окно с turn[1]
        assert!(ps[2].end_ms.is_none()); // последнее окно
        for p in &ps {
            assert_eq!(p.kind, AssistantPassageKind::Transcript);
            assert!(p.token_est > 0);
        }
    }

    #[test]
    fn oversized_single_turn_is_own_passage() {
        // Реплика больше таргета не делится и не зацикливает алгоритм.
        let turns = vec![turn("owner", 0, 4_000), turn("Speaker 0", 5_000, 100)];
        let ps = build_transcript_passages(&turns, &std::collections::HashMap::new());
        assert_eq!(ps.len(), 2);
        assert!(ps[0].token_est as usize > TRANSCRIPT_PASSAGE_TARGET_TOKENS);
    }

    #[test]
    fn passage_text_carries_speaker_tags() {
        let turns = vec![turn("owner", 0, 40), turn("Speaker 0", 1_000, 40)];
        let ps = build_transcript_passages(&turns, &std::collections::HashMap::new());
        assert_eq!(ps.len(), 1);
        assert!(ps[0].text.starts_with("owner: "));
        assert!(ps[0].text.contains("\nSpeaker 0: "));
        assert_eq!(ps[0].speaker.as_deref(), Some("owner"));
    }

    #[test]
    fn empty_turns_yield_no_passages() {
        assert!(build_transcript_passages(&[], &std::collections::HashMap::new()).is_empty());
    }

    #[test]
    fn window_boundary_exactly_at_target_stays_open() {
        // 175 ток + 175 ток = ровно 350 (НЕ > TARGET) → обе в одном окне;
        // третья (350+175 > 350) — уже нет.
        let turns = vec![
            turn("owner", 0, 700),
            turn("Speaker 0", 1_000, 700),
            turn("Speaker 1", 2_000, 700),
        ];
        let ps = build_transcript_passages(&turns, &std::collections::HashMap::new());
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].end_ms, Some(2_000)); // окно [0,2), следующее начинается с turn[2]... с overlap [1,3)
    }

    // ── build_recap_passages ──

    #[test]
    fn recap_paragraphs_skip_headings() {
        let md = "# Рекап\n\nСинхрон по пилоту перед демо.\n\n## Решения\n\n- Локальный режим на демо.\n- Отчёт к пятнице.\n\n## Пустая секция\n\n";
        let ps = build_recap_passages(md);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].text, "Синхрон по пилоту перед демо.");
        assert!(ps[1].text.contains("Локальный режим"));
        assert!(ps[1].text.contains("\n- Отчёт"));
        assert!(ps.iter().all(|p| p.kind == AssistantPassageKind::Recap));
        assert!(ps.iter().all(|p| p.start_ms.is_none()));
    }

    // ── build_structured_passages ──

    #[test]
    fn structured_rows_map_with_and_without_evidence() {
        let decisions = vec![crate::db::decisions::DecisionRow {
            id: "d1".into(),
            call_id: "c1".into(),
            text: "Показываем локальный режим".into(),
            evidence_quote: Some("давайте зафиксируем".into()),
            evidence_speaker: Some("Speaker 0".into()),
            evidence_start_ms: Some(62_000),
            evidence_end_ms: Some(70_000),
            confidence: Some(0.9),
            order_idx: 0,
        }];
        let items = vec![crate::db::ActionItem {
            id: "a1".into(),
            call_id: "c1".into(),
            text: "Стенд для нагрузочных".into(),
            owner_contact_id: None,
            due: Some("четверг".into()),
            done: false,
            owner_confidence: None,
            due_confidence: None,
            category: Some("commitment".into()),
            evidence_quote: None,
            evidence_speaker: None,
            evidence_start_ms: None,
        }];
        let questions = vec![crate::db::open_questions::OpenQuestionRow {
            id: "q1".into(),
            call_id: "c1".into(),
            text: "   ".into(), // пустой текст — скип
            raised_by: None,
            evidence_quote: None,
            evidence_speaker: None,
            evidence_start_ms: None,
            order_idx: 0,
        }];
        let ps = build_structured_passages(
            &decisions,
            &items,
            &questions,
            &std::collections::HashMap::new(),
        );
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].kind, AssistantPassageKind::Decision);
        assert!(ps[0].text.contains("— цитата: давайте зафиксируем"));
        assert_eq!(ps[0].start_ms, Some(62_000));
        assert_eq!(ps[0].end_ms, Some(70_000));
        assert_eq!(ps[1].kind, AssistantPassageKind::ActionItem);
        assert_eq!(ps[1].text, "Стенд для нагрузочных");
        assert!(ps[1].end_ms.is_none());
    }
}
