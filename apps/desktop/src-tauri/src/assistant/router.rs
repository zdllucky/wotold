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

pub(crate) fn words(question: &str) -> Vec<String> {
    question
        .to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn has(ws: &[String], w: &str) -> bool {
    ws.contains(&w.to_string())
}

pub(crate) fn has_any(ws: &[String], set: &[&str]) -> bool {
    ws.iter().any(|x| set.contains(&x.as_str()))
}

/// [TD-22] Служебные слова, не несущие темы. Класс замкнутый — предлоги,
/// местоимения, связки, вопросительные частицы. Именно поэтому список
/// допустим: в отличие от содержательных слов, новых предлогов в языке не
/// появляется.
const FUNCTION_WORDS: &[&str] = &[
    // предлоги
    "в",
    "во",
    "на",
    "за",
    "по",
    "с",
    "со",
    "к",
    "ко",
    "у",
    "о",
    "об",
    "обо",
    "из",
    "от",
    "до",
    "при",
    "про", // местоимения и указатели
    "я",
    "мы",
    "ты",
    "вы",
    "он",
    "она",
    "они",
    "мне",
    "нам",
    "мой",
    "моя",
    "мои",
    "моих",
    "наш",
    "наши",
    "это",
    "этот",
    "эта",
    "эти",
    "том",
    "тот",
    "та",
    "те",
    "там",
    "тут",
    "всех",
    "все",
    "всего", // связки и частицы
    "и",
    "а",
    "но",
    "же",
    "ли",
    "бы",
    "не",
    "ещё",
    "еще",
    "уже",
    "быть",
    "был",
    "была",
    "было",
    "были",
    "есть",
];

/// [TD-22] Слова, которыми описывают сам факт записи. Для вопроса «сколько
/// звонков записано» это не тема, а хвост триггера.
const RECORDING_WORDS: &[&str] = &[
    "записано",
    "записан",
    "записана",
    "записаны",
    "записал",
    "записали",
    "прошло",
    "прошли",
    "состоялось",
    "состоялись",
];

/// [TD-22] Слова относительного периода. Дублируют вокабуляр
/// [`super::period::period_range`]; расхождение ловится тестом
/// `period_words_are_recognized_by_parser`.
const PERIOD_WORDS: &[&str] = &[
    "сегодня",
    "вчера",
    "позавчера",
    "неделю",
    "неделе",
    "недели",
    "неделя",
    "месяц",
    "месяца",
    "месяце",
    "год",
    "года",
    "году",
    // квалификаторы периода — без них period_range не понимает год и
    // переключение на календарный интервал
    "прошлой",
    "прошлую",
    "прошлом",
    "прошлый",
    "прошлого",
    "этом",
    "этот",
    "текущем",
    "назад",
];

/// [TD-22] Есть ли в вопросе тема помимо самого триггера.
///
/// Прямой ответ роутера безальтернативен: fallthrough при низкой уверенности
/// не предусмотрен, поэтому ложный перехват = гарантированно неверный ответ.
/// «Какие решения приняли на встрече?» отдавал **список звонков**, не вызывая
/// ни retrieval, ни LLM.
///
/// Правило: если после вычёркивания триггеров, слов о звонке, периода и
/// служебных слов что-то осталось — вопрос про содержание, а не про список.
/// Приём тот же, что в `when_topic`, который так работал с самого начала.
fn has_topic_beyond(ws: &[String], triggers: &[&str]) -> bool {
    ws.iter().any(|w| {
        let w = w.as_str();
        !triggers.contains(&w)
            && !CALL_WORDS.contains(&w)
            && !FUNCTION_WORDS.contains(&w)
            && !RECORDING_WORDS.contains(&w)
            && !PERIOD_WORDS.contains(&w)
    })
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
    // [TD-22] «Сколько задач раздали на встрече?» — вопрос про содержание,
    // а не про счётчик записей; тема есть → обычный конвейер.
    if has(&ws, "сколько") && has_any(&ws, CALL_WORDS) && !has_topic_beyond(&ws, &["сколько"])
    {
        return Ok(Some(super::direct::stats_answer(pool).await?));
    }

    // LastCall: «когда был последний звонок» / «о чём был последний звонок».
    // [TD-22] Женские формы добавлены: «встреча» женского рода, и «когда была
    // последняя встреча» уходило мимо роутера целиком.
    if has_any(
        &ws,
        &[
            "последний",
            "последнего",
            "последнем",
            "последняя",
            "последнюю",
            "последней",
            "крайний",
            "крайняя",
            "крайнюю",
            "крайней",
        ],
    ) && has_any(&ws, CALL_WORDS)
    {
        let Some((id, title, date)) = super::direct::last_ready_call(pool).await? else {
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
    // [TD-22] «Какие решения приняли на встрече?» отдавал список звонков,
    // не вызывая ни retrieval, ни LLM — тема в вопросе снимает перехват.
    const LIST_TRIGGERS: &[&str] = &["какие", "покажи", "список", "перечисли"];
    if has_any(&ws, LIST_TRIGGERS)
        && has_any(&ws, CALL_WORDS)
        && !has_topic_beyond(&ws, LIST_TRIGGERS)
    {
        return Ok(Some(super::direct::list_calls(pool, &ws).await?));
    }

    // [B26.5a] «Кто такой/такая X» → карточка контакта. Контакт не найден →
    // None: обычный конвейер (упоминания в звонках).
    if has(&ws, "кто") && has_any(&ws, &["такой", "такая", "такое", "это"]) {
        if let Some(routed) = super::direct::who_is(pool, &ws).await? {
            return Ok(Some(routed));
        }
    }

    Ok(None)
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
        None,
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
                super::direct::fmt_date(&s).unwrap_or(s),
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

    // [B26.5a] «Кто такой X» → карточка контакта; нет контакта → None.
    #[tokio::test]
    async fn who_is_returns_contact_card_or_falls_through() {
        let db = fresh_db().await;
        sqlx::query(
            "INSERT INTO contacts (id, display_name, org, created_at, updated_at)
             VALUES ('ct1', 'Ренат Буланов', 'Acme', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&db.pool)
        .await
        .unwrap();

        let routed = try_route(&db.pool, "Кто такой Буланов Ренат", None, None)
            .await
            .unwrap();
        let Some(RoutedAnswer::Direct { text, .. }) = routed else {
            panic!("контакт найден — должен быть Direct");
        };
        assert!(text.contains("Ренат Буланов — контакт, Acme"), "{text}");
        assert!(text.contains("Совместных звонков не записано"), "{text}");

        // Контакта нет → None (уходит в retrieval за упоминаниями).
        assert!(try_route(&db.pool, "Кто такой Александр", None, None)
            .await
            .unwrap()
            .is_none());
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

    // ── [TD-22] ложные перехваты ────────────────────────────────────────

    const LIST_TRIGGERS_FOR_TEST: &[&str] = &["какие", "покажи", "список", "перечисли"];

    #[tokio::test]
    async fn content_questions_with_call_words_are_not_intercepted() {
        // Регрессия TD-22. Старый список негативов содержал только
        // «что…»-формулировки и эту дыру не ловил: вопрос со словом о звонке
        // И триггером перехватывался целиком. Прямой ответ безальтернативен
        // (fallthrough нет), поэтому ложный перехват = гарантированно
        // неверный ответ, причём без вызова retrieval и LLM.
        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Планёрка", "2026-07-01T09:00:00+00:00").await;

        for q in [
            "Какие решения приняли на встрече",
            "Сколько задач раздали на встрече",
            "Какие риски обсуждали на созвоне",
            "Перечисли договорённости со звонка",
            "Покажи задачи из встречи",
        ] {
            let routed = try_route(&db.pool, q, None, None).await.unwrap();
            assert!(
                routed.is_none(),
                "{q:?} — вопрос про содержание, обязан идти в обычный конвейер"
            );
        }
    }

    #[tokio::test]
    async fn listing_questions_are_still_intercepted() {
        // Вторая половина: guard не должен убить сам интент. Без этого теста
        // «починка» могла бы просто отключить роутер и выглядеть зелёной.
        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Планёрка", &chrono::Utc::now().to_rfc3339()).await;

        for q in [
            "какие звонки были",
            "покажи звонки",
            "какие созвоны были за неделю",
            "перечисли встречи",
            "сколько звонков записано",
            "сколько было звонков",
        ] {
            let routed = try_route(&db.pool, q, None, None).await.unwrap();
            assert!(routed.is_some(), "{q:?} — это запрос списка или счётчика");
        }
    }

    #[tokio::test]
    async fn last_call_matches_feminine_forms() {
        // «встреча» женского рода, и «когда была последняя встреча» уходило
        // мимо роутера: список форм покрывал только мужской.
        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Синхрон", "2026-07-01T09:00:00+00:00").await;

        for q in [
            "когда была последняя встреча",
            "когда была крайняя встреча",
            "о чём была последняя запись",
        ] {
            let routed = try_route(&db.pool, q, None, None).await.unwrap();
            assert!(routed.is_some(), "{q:?} — вопрос про последний звонок");
        }
    }

    #[tokio::test]
    async fn period_phrases_do_not_look_like_a_topic() {
        // PERIOD_WORDS дублирует вокабуляр period_range, и разойтись им
        // нельзя: слово, которое парсер считает периодом, а guard — темой,
        // сломает «какие звонки были за <период>».
        //
        // Проверяем именно это свойство, а не «каждое слово парсится в
        // одиночку»: год парсер намеренно требует с квалификатором
        // («в прошлом году»), голое «год» периодом не считается.
        let db = fresh_db().await;
        seed_call(&db.pool, "c1", "Планёрка", &chrono::Utc::now().to_rfc3339()).await;

        for q in [
            "какие звонки были сегодня",
            "какие звонки были вчера",
            "какие звонки были позавчера",
            "какие звонки были за неделю",
            "какие звонки были на прошлой неделе",
            "какие звонки были за месяц",
            "какие звонки были в прошлом месяце",
            "какие звонки были в этом году",
            "какие звонки были в прошлом году",
        ] {
            let ws = words(q);
            assert!(
                !has_topic_beyond(&ws, LIST_TRIGGERS_FOR_TEST),
                "{q:?} — период, а не тема"
            );
            assert!(
                try_route(&db.pool, q, None, None).await.unwrap().is_some(),
                "{q:?} обязан перехватываться как список"
            );
        }
    }
}
