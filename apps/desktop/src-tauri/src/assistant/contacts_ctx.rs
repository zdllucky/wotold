//! [B26.5] Контакты в контексте ассистента (выбор юзера: «и то и другое»):
//! (a) router-интент «кто такой X» → детерминированная карточка контакта;
//! (b) инжект-канал — контакт, чьё имя встретилось в ЛЮБОМ вопросе,
//! добавляется фрагментом-источником (kind `contact`, sentinel
//! `call_id = "contact:<id>"`) в контекст LLM и в «Контекст поиска».
//!
//! Почему НЕ FTS-индексация: контактов десятки-сотни — in-memory скан при
//! ask (<1мс) даёт тот же результат без миграций и sync-хуков; кириллица
//! в SQLite NOCASE не фолдится, матчим в Rust.

use sqlx::SqlitePool;

use crate::db::assistant::PassageHit;
use crate::AppError;

/// Префикс sentinel-«call_id» контакт-фрагмента (фронт распознаёт и ведёт
/// в раздел «Контакты», а не в звонок).
pub const CONTACT_CALL_PREFIX: &str = "contact:";

/// Максимум контакт-фрагментов на вопрос (не заспамить контекст).
const MAX_INJECTED: usize = 2;
/// Минимальная длина слова имени для матча (короткие инициалы — шум).
const MIN_NAME_WORD: usize = 3;
/// Хвост падежа: «Дамира»/«Дамиром» матчат имя «Дамир».
const MAX_SUFFIX_DIFF: usize = 3;
/// Усечение заметок в карточке.
const NOTES_MAX_CHARS: usize = 200;

/// Лёгкая карточка контакта (без identifiers — они не нужны LLM).
#[derive(Debug, Clone)]
pub struct ContactBrief {
    pub id: String,
    pub display_name: String,
    pub org: Option<String>,
    pub role: Option<String>,
    pub notes: Option<String>,
}

/// Статистика совместных звонков (подтверждённые привязки).
#[derive(Debug, Clone, Default)]
pub struct ContactCallStats {
    pub call_count: i64,
    /// (id, титул, «ДД.ММ.ГГГГ») последнего звонка.
    pub last_call: Option<(String, String, String)>,
}

fn normalize(s: &str) -> String {
    s.to_lowercase().replace('ё', "е")
}

/// Слово вопроса матчит слово имени: точно или как падежная форма
/// (общий префикс = имя, хвост ≤3 симв).
fn word_matches(question_word: &str, name_word: &str) -> bool {
    if name_word.chars().count() < MIN_NAME_WORD {
        return false;
    }
    question_word == name_word
        || (question_word.starts_with(name_word)
            && question_word.chars().count() - name_word.chars().count() <= MAX_SUFFIX_DIFF)
}

/// Контакты, чьё имя (любое слово display_name) встречается в вопросе.
pub fn match_contacts<'a>(contacts: &'a [ContactBrief], question: &str) -> Vec<&'a ContactBrief> {
    let q_words: Vec<String> = normalize(question)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    contacts
        .iter()
        .filter(|c| {
            normalize(&c.display_name)
                .split(|ch: char| !ch.is_alphanumeric())
                .any(|nw| q_words.iter().any(|qw| word_matches(qw, nw)))
        })
        .collect()
}

/// Строка выборки контакта: (id, display_name, org, role, notes).
type ContactRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Все контакты одним лёгким SELECT (identifiers не тянем).
pub async fn list_contact_briefs(pool: &SqlitePool) -> Result<Vec<ContactBrief>, AppError> {
    let rows: Vec<ContactRow> =
        sqlx::query_as("SELECT id, display_name, org, role, notes FROM contacts")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, display_name, org, role, notes)| ContactBrief {
            id,
            display_name,
            org,
            role,
            notes,
        })
        .collect())
}

/// Совместные звонки контакта (подтверждённые привязки call_speakers).
pub async fn contact_call_stats(
    pool: &SqlitePool,
    contact_id: &str,
) -> Result<ContactCallStats, AppError> {
    let (call_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT cs.call_id) FROM call_speakers cs
         JOIN calls c ON c.id = cs.call_id
         WHERE cs.contact_id = ?1 AND cs.confirmed = 1 AND c.status = 'ready'",
    )
    .bind(contact_id)
    .fetch_one(pool)
    .await?;
    let last_call: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT c.id, c.title, c.started_at FROM call_speakers cs
         JOIN calls c ON c.id = cs.call_id
         WHERE cs.contact_id = ?1 AND cs.confirmed = 1 AND c.status = 'ready'
         ORDER BY c.started_at DESC LIMIT 1",
    )
    .bind(contact_id)
    .fetch_optional(pool)
    .await?;
    Ok(ContactCallStats {
        call_count,
        last_call: last_call.map(|(id, title, started_at)| {
            let title = title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| "Без названия".to_string());
            let date = fmt_date(&started_at).unwrap_or(started_at);
            (id, title, date)
        }),
    })
}

fn fmt_date(started_at: &str) -> Option<String> {
    let d = started_at.get(..10)?;
    let mut it = d.split('-');
    let (y, m, day) = (it.next()?, it.next()?, it.next()?);
    if y.len() != 4 || m.len() != 2 || day.len() != 2 {
        return None;
    }
    Some(format!("{day}.{m}.{y}"))
}

/// Текст карточки контакта — для фрагмента и Direct-ответа роутера.
pub fn contact_card_text(c: &ContactBrief, stats: &ContactCallStats) -> String {
    let mut text = format!("{} — контакт", c.display_name);
    if let Some(org) = c.org.as_deref().filter(|s| !s.trim().is_empty()) {
        text.push_str(&format!(", {org}"));
    }
    if let Some(role) = c.role.as_deref().filter(|s| !s.trim().is_empty()) {
        text.push_str(&format!(", {role}"));
    }
    text.push('.');
    if let Some(notes) = c.notes.as_deref().filter(|s| !s.trim().is_empty()) {
        let short: String = notes.chars().take(NOTES_MAX_CHARS).collect();
        text.push_str(&format!(" Заметки: {short}."));
    }
    if stats.call_count > 0 {
        text.push_str(&format!(" Звонков вместе: {}", stats.call_count));
        if let Some((_, title, date)) = &stats.last_call {
            text.push_str(&format!(", последний — «{title}», {date}"));
        }
        text.push('.');
    } else {
        text.push_str(" Совместных звонков не записано.");
    }
    text
}

/// [B26.5b] Инжект-канал: контакты из вопроса → синтетические PassageHit
/// (для ctx.fragments) + мапа sentinel→имя (титулы для промпта/чипов).
/// Ошибки — пустой результат (канал best-effort, основной конвейер важнее).
pub async fn contact_hits_for_question(
    pool: &SqlitePool,
    question: &str,
) -> (Vec<PassageHit>, std::collections::HashMap<String, String>) {
    let contacts = match list_contact_briefs(pool).await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("assistant contacts inject: {e}");
            return (Vec::new(), std::collections::HashMap::new());
        }
    };
    let matched = match_contacts(&contacts, question);
    let mut hits = Vec::new();
    let mut titles = std::collections::HashMap::new();
    for c in matched.into_iter().take(MAX_INJECTED) {
        let stats = contact_call_stats(pool, &c.id).await.unwrap_or_default();
        let sentinel = format!("{CONTACT_CALL_PREFIX}{}", c.id);
        let text = contact_card_text(c, &stats);
        titles.insert(sentinel.clone(), c.display_name.clone());
        hits.push(PassageHit {
            id: -1, // синтетический, в БД не существует
            call_id: sentinel,
            kind: "contact".to_string(),
            speaker: None,
            start_ms: None,
            end_ms: None,
            token_est: (text.len() / 4).max(1) as i64,
            text,
            rank: 0.0,
        });
    }
    (hits, titles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    fn brief(name: &str) -> ContactBrief {
        ContactBrief {
            id: format!("id-{name}"),
            display_name: name.into(),
            org: None,
            role: None,
            notes: None,
        }
    }

    #[test]
    fn match_contacts_handles_cases_and_cyrillic() {
        let contacts = vec![brief("Дамир Нуртазин"), brief("Иван Петров"), brief("Ли")];
        // Падежная форма «Дамира» матчит «Дамир»; регистр/ё нормализуются.
        let m = match_contacts(&contacts, "Что за проект Дамира?");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].display_name, "Дамир Нуртазин");
        // Фамилия тоже матчит.
        let m = match_contacts(&contacts, "что говорил петров вчера");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].display_name, "Иван Петров");
        // Короткое имя (<3) не матчится (шум).
        assert!(match_contacts(&contacts, "что по ли").is_empty());
        // Нет имени — пусто.
        assert!(match_contacts(&contacts, "что решили по бюджету").is_empty());
        // Слишком длинный хвост («Дамирский» ≠ падеж «Дамир») — не матч.
        assert!(match_contacts(&contacts, "дамирский проект").is_empty());
    }

    #[tokio::test]
    async fn stats_and_card_text() {
        let db = fresh_db().await;
        sqlx::query(
            "INSERT INTO contacts (id, display_name, org, notes, created_at, updated_at)
             VALUES ('ct1', 'Иван Петров', 'Acme', 'Партнёр по инвойсу', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO calls (id, title, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES ('c1', 'Синхрон', '2026-07-01T09:00:00+00:00', 60, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO call_speakers (id, call_id, speaker_tag, contact_id, confirmed)
             VALUES ('cs1', 'c1', 'speaker:1', 'ct1', 1)",
        )
        .execute(&db.pool)
        .await
        .unwrap();

        let stats = contact_call_stats(&db.pool, "ct1").await.unwrap();
        assert_eq!(stats.call_count, 1);
        let (_, title, date) = stats.last_call.as_ref().unwrap();
        assert_eq!(title, "Синхрон");
        assert_eq!(date, "01.07.2026");

        let card = contact_card_text(
            &ContactBrief {
                id: "ct1".into(),
                display_name: "Иван Петров".into(),
                org: Some("Acme".into()),
                role: None,
                notes: Some("Партнёр по инвойсу".into()),
            },
            &stats,
        );
        assert_eq!(
            card,
            "Иван Петров — контакт, Acme. Заметки: Партнёр по инвойсу. \
             Звонков вместе: 1, последний — «Синхрон», 01.07.2026."
        );
    }

    #[tokio::test]
    async fn inject_builds_sentinel_hits() {
        let db = fresh_db().await;
        sqlx::query(
            "INSERT INTO contacts (id, display_name, created_at, updated_at)
             VALUES ('ct1', 'Ренат Буланов', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&db.pool)
        .await
        .unwrap();

        let (hits, titles) = contact_hits_for_question(&db.pool, "Кто такой Буланов Ренат").await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "contact");
        assert_eq!(hits[0].call_id, "contact:ct1");
        assert!(hits[0].text.contains("Ренат Буланов — контакт"));
        assert_eq!(titles.get("contact:ct1").unwrap(), "Ренат Буланов");

        let (none, _) = contact_hits_for_question(&db.pool, "что решили по бюджету").await;
        assert!(none.is_empty());
    }
}
