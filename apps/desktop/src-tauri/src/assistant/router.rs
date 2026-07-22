//! [M16.4] Интент-раутер мета-вопросов — работает ДО retrieval (между
//! классификатором и фазой «retrieving» в `ask_core_with`).
//!
//! Мотивация (data-диагностика M16, живые фейлы Q1/Q6/Q9/Q15): вопросы
//! «сколько звонков», «когда был последний», «какие звонки были» отвечаются
//! метаданными `calls`/`assistant_index_state`, а не FTS-поиском — раньше
//! они уходили в retrieval и заканчивались «нет ответа». Детерминированные
//! интенты отвечают БЕЗ LLM: нулевая латентность, нулевые галлюцинации —
//! идеально при локальной 1.5-3B модели.
//!
//! Матчинг — слова целиком (стиль `classifier.rs`), узкие паттерны с якорем
//! на «звонок/встреча/созвон» + обязательные негативные тесты: контентные
//! вопросы («что решили по командам») роутер НЕ перехватывает.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::assistant::answer::{detect_prompt_mode, PromptMode};
use crate::assistant::embedder::TextEmbedder;
use crate::assistant::types::AssistantSource;
use crate::assistant::{embed_cache, retrieval};
use crate::AppError;

/// Результат роутинга.
pub enum RoutedAnswer {
    /// Готовый детерминированный ответ — мимо retrieval и LLM.
    Direct {
        text: String,
        sources: Vec<AssistantSource>,
    },
    /// «О чём [последний] звонок» — резолвнутый call_id, дальше
    /// call-summary путь (M16.5: рекап-пассажи напрямую).
    SummarizeCall { call_id: String },
}

/// Слова-якоря «звонок» (без них stats/last/list не срабатывают —
/// защита от перехвата контентных вопросов).
const CALL_WORDS: &[&str] = &[
    "звонок",
    "звонка",
    "звонке",
    "звонков",
    "звонки",
    "созвон",
    "созвона",
    "созвоне",
    "созвонов",
    "созвоны",
    "встреча",
    "встречи",
    "встрече",
    "встреч",
    "встречу",
    "запись",
    "записи",
    "записей",
];

/// Глаголы «обсуждали» для WhenDiscussed.
const DISCUSS_VERBS: &[&str] = &[
    "обсуждали",
    "обсуждался",
    "обсуждалась",
    "обсуждалось",
    "говорили",
    "поднимали",
    "упоминали",
    "разговаривали",
    "затрагивали",
];

/// Служебные слова, отрезаемые от темы WhenDiscussed.
const WHEN_STRIP: &[&str] = &[
    "когда",
    "мы",
    "вы",
    "я",
    "раз",
    "последний",
    "про",
    "об",
    "обо",
    "тему",
    "тема",
];

fn words(question: &str) -> Vec<String> {
    question
        .to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn has(ws: &[String], w: &str) -> bool {
    ws.contains(&w.to_string())
}

fn has_any(ws: &[String], set: &[&str]) -> bool {
    ws.iter().any(|x| set.contains(&x.as_str()))
}

/// Попытка роутинга. `None` — обычный конвейер (retrieval → LLM).
pub async fn try_route(
    pool: &SqlitePool,
    question: &str,
    scope_call_id: Option<&str>,
    embedder: Option<Arc<dyn TextEmbedder>>,
) -> Result<Option<RoutedAnswer>, AppError> {
    let ws = words(question);

    // [M16.5] Обобщающий вопрос в call-scope («о чём звонок», «суть»,
    // «итоги») — сразу рекап-путь: FTS-матч по таким вопросам случаен.
    if let Some(call_id) = scope_call_id {
        if detect_prompt_mode(question) == PromptMode::Summarize {
            return Ok(Some(RoutedAnswer::SummarizeCall {
                call_id: call_id.to_string(),
            }));
        }
    }

    // Stats: «сколько звонков [записано/было]».
    if has(&ws, "сколько") && has_any(&ws, CALL_WORDS) {
        return Ok(Some(stats_answer(pool).await?));
    }

    // LastCall: «когда был последний звонок» / «о чём был последний звонок».
    if has_any(&ws, &["последний", "последнего", "последнем", "крайний"])
        && has_any(&ws, CALL_WORDS)
    {
        let Some((id, title, date)) = last_ready_call(pool).await? else {
            return Ok(Some(RoutedAnswer::Direct {
                text: "Записанных звонков пока нет.".to_string(),
                sources: vec![],
            }));
        };
        if detect_prompt_mode(question) == PromptMode::Summarize {
            return Ok(Some(RoutedAnswer::SummarizeCall { call_id: id }));
        }
        return Ok(Some(RoutedAnswer::Direct {
            text: format!("Последний звонок — «{title}», {date}."),
            sources: vec![AssistantSource {
                call_id: id,
                call_title: title,
                start_ms: None,
            }],
        }));
    }

    // WhenDiscussed: «когда обсуждали ТЕМУ» → поиск темы + даты звонков.
    if has(&ws, "когда") && has_any(&ws, DISCUSS_VERBS) {
        return when_discussed(pool, question, &ws, embedder).await;
    }

    // ListCalls: «какие звонки были [за неделю]» / «покажи звонки».
    if has_any(&ws, &["какие", "покажи", "список", "перечисли"]) && has_any(&ws, CALL_WORDS)
    {
        return Ok(Some(list_calls(pool, &ws).await?));
    }

    Ok(None)
}

/// «Записано N звонков (M в поиске), суммарно X ч Y мин» — из index_stats.
async fn stats_answer(pool: &SqlitePool) -> Result<RoutedAnswer, AppError> {
    let stats = crate::db::assistant::index_stats(pool).await?;
    let total_min = stats.total_duration_sec / 60;
    let dur = if total_min >= 60 {
        format!("{} ч {} мин", total_min / 60, total_min % 60)
    } else {
        format!("{total_min} мин")
    };
    let text = format!(
        "Записано {} звонков, {} из них в поиске ассистента. Суммарная длительность — {dur}.",
        stats.total_calls, stats.indexed_calls
    );
    Ok(RoutedAnswer::Direct {
        text,
        sources: vec![],
    })
}

/// Последний ready-звонок: (id, титул, «ДД.ММ.ГГГГ»).
async fn last_ready_call(pool: &SqlitePool) -> Result<Option<(String, String, String)>, AppError> {
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, title, started_at FROM calls WHERE status = 'ready'
         ORDER BY started_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, title, started_at)| {
        let date = fmt_date(&started_at).unwrap_or_else(|| started_at.clone());
        let title = title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Без названия".to_string());
        (id, title, date)
    }))
}

/// «2026-07-21T…» → «21.07.2026».
fn fmt_date(started_at: &str) -> Option<String> {
    let d = started_at.get(..10)?;
    let mut it = d.split('-');
    let (y, m, day) = (it.next()?, it.next()?, it.next()?);
    if y.len() != 4 || m.len() != 2 || day.len() != 2 {
        return None;
    }
    Some(format!("{day}.{m}.{y}"))
}

/// Тема после среза служебных слов WhenDiscussed. None — темы не осталось.
fn when_topic(question: &str, ws: &[String]) -> Option<String> {
    let _ = question;
    let topic: Vec<&str> = ws
        .iter()
        .map(String::as_str)
        .filter(|w| !WHEN_STRIP.contains(w) && !DISCUSS_VERBS.contains(w))
        .collect();
    if topic.is_empty() {
        return None;
    }
    Some(topic.join(" "))
}

/// «Когда обсуждали X» → retrieval темы → группировка по звонкам с датами.
async fn when_discussed(
    pool: &SqlitePool,
    question: &str,
    ws: &[String],
    embedder: Option<Arc<dyn TextEmbedder>>,
) -> Result<Option<RoutedAnswer>, AppError> {
    let Some(topic) = when_topic(question, ws) else {
        return Ok(None); // «когда обсуждали?» без темы — обычный конвейер
    };
    let hits = retrieval::search_hybrid(
        pool,
        &topic,
        retrieval::Scope::Global,
        embedder,
        embed_cache::global(),
    )
    .await?;
    if hits.is_empty() {
        return Ok(Some(RoutedAnswer::Direct {
            text: crate::assistant::EMPTY_GLOBAL_TEXT.to_string(),
            sources: vec![],
        }));
    }
    // Первое вхождение per звонок (hits идут best-first), максимум 3 звонка.
    let mut seen: Vec<(String, Option<i64>)> = Vec::new();
    for h in &hits {
        if seen.iter().any(|(c, _)| c == &h.call_id) {
            continue;
        }
        seen.push((h.call_id.clone(), h.start_ms));
        if seen.len() >= 3 {
            break;
        }
    }
    let mut lines = Vec::new();
    let mut sources = Vec::new();
    for (call_id, start_ms) in &seen {
        let meta: Option<(Option<String>, String)> =
            sqlx::query_as("SELECT title, started_at FROM calls WHERE id = ?1")
                .bind(call_id)
                .fetch_optional(pool)
                .await?;
        let (title, date) = match meta {
            Some((t, s)) => (
                t.filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| "Без названия".into()),
                fmt_date(&s).unwrap_or(s),
            ),
            None => continue,
        };
        let clock = start_ms
            .map(|ms| {
                let sec = (ms / 1000).max(0);
                format!(", {}:{:02}", sec / 60, sec % 60)
            })
            .unwrap_or_default();
        lines.push(format!("— «{title}» · {date}{clock}"));
        sources.push(AssistantSource {
            call_id: call_id.clone(),
            call_title: title,
            start_ms: *start_ms,
        });
    }
    if lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(RoutedAnswer::Direct {
        text: format!("Тема поднималась:\n{}", lines.join("\n")),
        sources,
    }))
}

/// Относительный период из слов вопроса → дней назад. None — без фильтра.
fn period_days(ws: &[String]) -> Option<i64> {
    if has(ws, "сегодня") {
        return Some(1);
    }
    if has(ws, "вчера") {
        return Some(2);
    }
    if has_any(ws, &["неделю", "неделе", "недели", "неделя"]) {
        return Some(7);
    }
    if has_any(ws, &["месяц", "месяца", "месяце"]) {
        return Some(31);
    }
    if has_any(ws, &["квартал", "квартала", "квартале"]) {
        return Some(92);
    }
    None
}

/// Список звонков (опц. за период), свежие сверху, максимум 10.
async fn list_calls(pool: &SqlitePool, ws: &[String]) -> Result<RoutedAnswer, AppError> {
    let since = period_days(ws).map(|days| {
        (chrono::Utc::now() - chrono::Duration::days(days))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    });
    let rows: Vec<(String, Option<String>, String)> = match &since {
        Some(s) => {
            sqlx::query_as(
                "SELECT id, title, started_at FROM calls
                 WHERE status = 'ready' AND started_at >= ?1
                 ORDER BY started_at DESC LIMIT 10",
            )
            .bind(s)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT id, title, started_at FROM calls
                 WHERE status = 'ready' ORDER BY started_at DESC LIMIT 10",
            )
            .fetch_all(pool)
            .await?
        }
    };
    if rows.is_empty() {
        return Ok(RoutedAnswer::Direct {
            text: if since.is_some() {
                "За этот период записанных звонков нет.".to_string()
            } else {
                "Записанных звонков пока нет.".to_string()
            },
            sources: vec![],
        });
    }
    let mut lines = Vec::new();
    let mut sources = Vec::new();
    for (id, title, started_at) in rows {
        let title = title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Без названия".into());
        let date = fmt_date(&started_at).unwrap_or(started_at);
        lines.push(format!("— «{title}» · {date}"));
        sources.push(AssistantSource {
            call_id: id,
            call_title: title,
            start_ms: None,
        });
    }
    Ok(RoutedAnswer::Direct {
        text: lines.join("\n"),
        sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    async fn seed_call(pool: &SqlitePool, id: &str, title: &str, started_at: &str) {
        sqlx::query(
            "INSERT INTO calls (id, title, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, ?2, ?3, 600, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .bind(title)
        .bind(started_at)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn stats_intent_answers_with_counts_without_llm() {
        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Планёрка", "2026-07-01T09:00:00+00:00").await;
        let routed = try_route(&db.pool, "сколько звонков записано", None, None)
            .await
            .unwrap();
        let Some(RoutedAnswer::Direct { text, .. }) = routed else {
            panic!("stats должен роутиться в Direct");
        };
        assert!(text.contains("Записано 1 звонков"), "{text}");
    }

    #[tokio::test]
    async fn last_call_when_answers_with_date_and_source() {
        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Старый", "2026-06-01T09:00:00+00:00").await;
        seed_call(&db.pool, "c2", "Свежий", "2026-07-21T05:57:08+00:00").await;
        let routed = try_route(&db.pool, "Когда был последний звонок?", None, None)
            .await
            .unwrap();
        let Some(RoutedAnswer::Direct { text, sources }) = routed else {
            panic!("last-call должен роутиться в Direct");
        };
        assert!(text.contains("«Свежий»"), "{text}");
        assert!(text.contains("21.07.2026"), "{text}");
        assert_eq!(sources[0].call_id, "c2");
    }

    #[tokio::test]
    async fn last_call_about_delegates_to_summarize() {
        let db = fresh_db().await;
        seed_call(&db.pool, "c2", "Свежий", "2026-07-21T05:57:08+00:00").await;
        let routed = try_route(&db.pool, "О чем был последний звонок", None, None)
            .await
            .unwrap();
        assert!(
            matches!(routed, Some(RoutedAnswer::SummarizeCall { ref call_id }) if call_id == "c2"),
            "«о чём последний» → SummarizeCall"
        );
    }

    #[tokio::test]
    async fn call_scope_summarize_question_routes_to_recap_path() {
        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Планёрка", "2026-07-01T09:00:00+00:00").await;
        let routed = try_route(&db.pool, "о чем звонок", Some("c1"), None)
            .await
            .unwrap();
        assert!(
            matches!(routed, Some(RoutedAnswer::SummarizeCall { ref call_id }) if call_id == "c1")
        );
    }

    #[tokio::test]
    async fn list_calls_with_and_without_period() {
        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Древний", "2020-01-01T09:00:00+00:00").await;
        seed_call(&db.pool, "c2", "Свежий", &chrono::Utc::now().to_rfc3339()).await;

        let all = try_route(&db.pool, "какие звонки были", None, None)
            .await
            .unwrap();
        let Some(RoutedAnswer::Direct { text, sources }) = all else {
            panic!("list должен роутиться");
        };
        assert!(
            text.contains("«Древний»") && text.contains("«Свежий»"),
            "{text}"
        );
        assert_eq!(sources.len(), 2);

        let week = try_route(&db.pool, "какие звонки были за неделю", None, None)
            .await
            .unwrap();
        let Some(RoutedAnswer::Direct { text, .. }) = week else {
            panic!("list-week должен роутиться");
        };
        assert!(
            text.contains("«Свежий»") && !text.contains("«Древний»"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn when_discussed_finds_call_and_date() {
        use crate::assistant::types::AssistantPassageKind;
        use crate::db::assistant::{replace_call_passages, PassageInput};

        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Синхрон", "2026-07-01T09:00:00+00:00").await;
        replace_call_passages(
            &db.pool,
            "c1",
            &[PassageInput {
                kind: AssistantPassageKind::Transcript,
                speaker: None,
                start_ms: Some(200_000),
                end_ms: Some(210_000),
                text: "обсуждали приватность данных и локальное хранение".into(),
                token_est: 10,
            }],
        )
        .await
        .unwrap();

        let routed = try_route(&db.pool, "Когда обсуждали приватность?", None, None)
            .await
            .unwrap();
        let Some(RoutedAnswer::Direct { text, sources }) = routed else {
            panic!("when-discussed должен роутиться");
        };
        assert!(text.contains("«Синхрон»"), "{text}");
        assert!(text.contains("01.07.2026"), "{text}");
        assert!(text.contains("3:20"), "таймкод первого вхождения: {text}");
        assert_eq!(sources[0].call_id, "c1");

        // Темы нет в корпусе → честное «не найдено», не мусорный ответ.
        let miss = try_route(&db.pool, "Когда обсуждали марсоходы?", None, None)
            .await
            .unwrap();
        let Some(RoutedAnswer::Direct { text, sources }) = miss else {
            panic!("miss должен дать Direct-empty");
        };
        assert_eq!(text, crate::assistant::EMPTY_GLOBAL_TEXT);
        assert!(sources.is_empty());
    }

    #[test]
    fn negatives_content_questions_are_not_routed_words() {
        // Контентные вопросы (живые SUCCESS-кейсы Q4/Q7/Q12/Q13 + фейлы,
        // которые чинят другие слои) не должны матчить ни один интент.
        for q in [
            "Решения планёрки продукта",
            "Что должен сделать дамир",
            "Что по переводам",
            "что по проекту",
            "Что решили на звонке по командам",
            "Кто такой Александр",
        ] {
            let ws = words(q);
            let stats = has(&ws, "сколько") && has_any(&ws, CALL_WORDS);
            let last = has_any(&ws, &["последний", "последнего", "последнем", "крайний"])
                && has_any(&ws, CALL_WORDS);
            let when = has(&ws, "когда") && has_any(&ws, DISCUSS_VERBS);
            let list = has_any(&ws, &["какие", "покажи", "список", "перечисли"])
                && has_any(&ws, CALL_WORDS);
            assert!(!stats && !last && !when && !list, "перехвачен: {q}");
        }
    }
}
