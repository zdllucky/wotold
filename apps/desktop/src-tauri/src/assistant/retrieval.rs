//! [M15.5] Retrieval: вопрос → безопасный FTS5 MATCH → ранжированные пассажи.
//!
//! Санитизация — токенизация по не-алфавитно-цифровым + обёртка каждого
//! токена в кавычки: сырой MATCH-синтаксис (OR/NEAR/скобки/кавычки/минусы)
//! до `assistant_fts` не доходит (W5, PRD §9.2). Русская морфология без
//! стемминга unicode61 компенсируется префикс-экспансией: `приватность →
//! "приватн"*`. Recall > precision: токены соединяются OR — bm25 ранжирует,
//! budget (M15.6) режет.

// [M15.5] Production caller — assistant::ask (M15.7).
#![allow(dead_code)]

use sqlx::SqlitePool;

use crate::db::assistant::{search_fts, PassageHit};
use crate::AppError;

/// Область поиска: весь архив или конкретный звонок (с добором из других).
#[derive(Debug, Clone, Copy)]
pub enum Scope<'a> {
    Global,
    Call(&'a str),
}

/// Лимиты SPEC/PRD §4.2: global top-12; call-scope 8 своих + 4 глобальных.
const GLOBAL_LIMIT: i64 = 12;
const CALL_OWN_LIMIT: i64 = 8;
const CALL_OTHER_LIMIT: i64 = 4;

/// Минимальная длина токена (односимвольные предлоги — шум).
const MIN_TOKEN_CHARS: usize = 2;
/// Максимум токенов в MATCH (защита от вопросов-простыней).
const MAX_TOKENS: usize = 12;
/// Слова длиннее этого получают префикс-экспансию.
const PREFIX_EXPANSION_MIN_CHARS: usize = 6;
/// Минимальная длина основы при экспансии.
const PREFIX_STEM_MIN_CHARS: usize = 4;

/// Вопрос → FTS5 MATCH-выражение. `None` — искать нечего (нет валидных
/// токенов), вызывающий возвращает пустой результат без похода в БД.
pub(crate) fn build_match_expr(question: &str) -> Option<String> {
    let terms: Vec<String> = question
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && w.chars().count() >= MIN_TOKEN_CHARS)
        .take(MAX_TOKENS)
        .map(term_for_token)
        .collect();
    if terms.is_empty() {
        return None;
    }
    Some(terms.join(" OR "))
}

/// Токен → `"токен"` или `"основа"*` (морфология-lite).
fn term_for_token(token: &str) -> String {
    let len = token.chars().count();
    if len >= PREFIX_EXPANSION_MIN_CHARS {
        let stem_len = (len - 2).max(PREFIX_STEM_MIN_CHARS);
        let stem: String = token.chars().take(stem_len).collect();
        format!("\"{stem}\"*")
    } else {
        format!("\"{token}\"")
    }
}

/// Поиск по индексу. Call-scope: сначала ВСЕ свои пассажи (top-8), затем
/// добор из других звонков (top-4) — свои безусловно раньше чужих (порядок
/// проходов, не merge по bm25 между ними; внутри прохода — bm25).
///
/// Длина `question` здесь не ограничивается (build_match_expr лениво берёт
/// первые 12 токенов, O(n) один проход) — command-слой (M15.8) должен
/// провалидировать/капнуть длину на границе.
pub async fn search(
    pool: &SqlitePool,
    question: &str,
    scope: Scope<'_>,
) -> Result<Vec<PassageHit>, AppError> {
    let Some(expr) = build_match_expr(question) else {
        return Ok(Vec::new());
    };
    match scope {
        Scope::Global => search_fts(pool, &expr, GLOBAL_LIMIT, None, None).await,
        Scope::Call(call_id) => {
            let mut own = search_fts(pool, &expr, CALL_OWN_LIMIT, Some(call_id), None).await?;
            let other = search_fts(pool, &expr, CALL_OTHER_LIMIT, None, Some(call_id)).await?;
            own.extend(other);
            Ok(own)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::types::AssistantPassageKind;
    use crate::db::assistant::{replace_call_passages, PassageInput};
    use crate::db::test_support::fresh_db;

    // ── build_match_expr (pure) ──

    #[test]
    fn short_words_exact_long_words_prefixed() {
        // «пилот» (5) — точная форма; «приватность» (11) — основа len-2=9.
        assert_eq!(
            build_match_expr("пилот приватность").as_deref(),
            Some("\"пилот\" OR \"приватнос\"*")
        );
    }

    #[test]
    fn six_char_word_gets_min_stem() {
        // «отчёты» (6 симв) → основа max(4, 6-2)=4 симв «отчё»*.
        assert_eq!(build_match_expr("отчёты").as_deref(), Some("\"отчё\"*"));
    }

    #[test]
    fn single_char_and_digits_only_filtered() {
        assert_eq!(build_match_expr("а в я"), None);
        assert_eq!(build_match_expr(""), None);
        assert_eq!(build_match_expr("?!…"), None);
        // Цифры — валидные токены (даты, номера версий).
        assert_eq!(build_match_expr("2026").as_deref(), Some("\"2026\""));
    }

    #[test]
    fn match_syntax_is_neutralized() {
        // Операторы/кавычки/скобки режутся сплитом — остаются голые слова.
        let expr = build_match_expr("сроки\" OR kind: NEAR(пилот) -минус при*вет").unwrap();
        assert!(!expr.contains("NEAR("));
        assert!(!expr.contains("kind:"));
        assert!(!expr.contains('-'));
        // Каждый терм в кавычках; допустимы только наши OR-соединители.
        for part in expr.split(" OR ") {
            assert!(part.starts_with('"'), "unquoted term: {part}");
        }
    }

    #[test]
    fn token_cap_limits_wall_of_text() {
        let long = (0..40)
            .map(|i| format!("слово{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let expr = build_match_expr(&long).unwrap();
        assert_eq!(expr.split(" OR ").count(), MAX_TOKENS);
    }

    // ── search (fresh_db) ──

    fn passage(text: &str, start_ms: i64) -> PassageInput {
        PassageInput {
            kind: AssistantPassageKind::Transcript,
            speaker: Some("owner".into()),
            start_ms: Some(start_ms),
            end_ms: Some(start_ms + 10_000),
            text: text.into(),
            token_est: (text.len() / 4) as i64,
        }
    }

    async fn seed(pool: &SqlitePool, call_id: &str, texts: &[&str]) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(call_id)
        .execute(pool)
        .await
        .unwrap();
        let inputs: Vec<PassageInput> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| passage(t, (i as i64) * 10_000))
            .collect();
        replace_call_passages(pool, call_id, &inputs).await.unwrap();
    }

    #[tokio::test]
    async fn morphology_prefix_finds_inflected_forms() {
        let db = fresh_db().await;
        seed(&db.pool, "c1", &["говорили о приватности данных"]).await;
        let hits = search(&db.pool, "приватность", Scope::Global)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn cyrillic_case_folding_by_tokenizer() {
        // Регресс-гвоздь на инвариант unicode61: case-fold кириллицы делает
        // токенизатор SQLite с обеих сторон MATCH (мы НЕ лоуэркейсим в Rust).
        // Ломается при даунгрейде/подмене SQLite — этот тест поймает.
        let db = fresh_db().await;
        seed(&db.pool, "c1", &["обсуждали бюджет проекта"]).await;
        let hits = search(&db.pool, "Бюджет", Scope::Global).await.unwrap();
        assert_eq!(hits.len(), 1);
        let hits_upper = search(&db.pool, "БЮДЖЕТ", Scope::Global).await.unwrap();
        assert_eq!(hits_upper.len(), 1);
    }

    #[tokio::test]
    async fn or_recall_matches_partial_question() {
        let db = fresh_db().await;
        seed(&db.pool, "c1", &["обсуждали бюджет проекта"]).await;
        // «динозавр» нигде нет — но OR даёт матч по «бюджету».
        let hits = search(&db.pool, "бюджет динозавр", Scope::Global)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn injection_attempts_return_ok() {
        let db = fresh_db().await;
        seed(&db.pool, "c1", &["обычный текст про сроки"]).await;
        for q in [
            "сроки\" OR kind:",
            "NEAR(пилот, 2)",
            "при*вет -минус",
            "\"unbalanced",
            "(скобки) AND всё",
        ] {
            let res = search(&db.pool, q, Scope::Global).await;
            assert!(res.is_ok(), "query must not error: {q}");
        }
    }

    #[tokio::test]
    async fn call_scope_own_first_then_others_with_limits() {
        let db = fresh_db().await;
        let own_texts: Vec<String> = (0..10).map(|i| format!("бюджет пункт {i}")).collect();
        let own_refs: Vec<&str> = own_texts.iter().map(String::as_str).collect();
        seed(&db.pool, "mine", &own_refs).await;
        let other_texts: Vec<String> = (0..6).map(|i| format!("бюджет чужой {i}")).collect();
        let other_refs: Vec<&str> = other_texts.iter().map(String::as_str).collect();
        seed(&db.pool, "other", &other_refs).await;

        let hits = search(&db.pool, "бюджет", Scope::Call("mine"))
            .await
            .unwrap();
        let own_count = hits.iter().filter(|h| h.call_id == "mine").count();
        let other_count = hits.iter().filter(|h| h.call_id == "other").count();
        assert_eq!(own_count, CALL_OWN_LIMIT as usize);
        assert_eq!(other_count, CALL_OTHER_LIMIT as usize);
        // Свои — префикс списка.
        assert!(hits[..own_count].iter().all(|h| h.call_id == "mine"));
    }

    #[tokio::test]
    async fn global_scope_caps_at_limit() {
        let db = fresh_db().await;
        let texts: Vec<String> = (0..20).map(|i| format!("бюджет строка {i}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        seed(&db.pool, "c1", &refs).await;
        let hits = search(&db.pool, "бюджет", Scope::Global).await.unwrap();
        assert_eq!(hits.len(), GLOBAL_LIMIT as usize);
    }

    #[tokio::test]
    async fn unmatchable_question_is_empty_without_db_error() {
        let db = fresh_db().await;
        seed(&db.pool, "c1", &["что-то было"]).await;
        assert!(search(&db.pool, "я и о", Scope::Global)
            .await
            .unwrap()
            .is_empty());
        assert!(search(&db.pool, "ксенофобия", Scope::Global)
            .await
            .unwrap()
            .is_empty());
    }
}
