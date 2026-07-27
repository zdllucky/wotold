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

/// [M16.3] RU-стоплист: служебные слова ≥2 симв, проходившие MIN_TOKEN_CHARS.
/// Data-диагностика M16 (живая БД): recap с ответом стоял в топ-4 по
/// `"команд"*`, но ВЫПАДАЛ из top-12 при добавлении `"Что" OR "по" OR "на"` —
/// высокочастотные токены раздувают bm25 длинных транскриптов. Однобуквенные
/// (и/в/с/у/о/к/я) режет MIN_TOKEN_CHARS. «когда/сколько» здесь тоже шум —
/// их семантику обрабатывает интент-раутер ДО retrieval (M16.4).
const RU_STOPWORDS: &[&str] = &[
    // вопросительные
    "что",
    "чем",
    "чём",
    "кто",
    "кого",
    "как",
    "где",
    "когда",
    "почему",
    "зачем",
    "сколько",
    "какой",
    "какая",
    "какое",
    "какие",
    // предлоги/частицы ≥2 симв
    "по",
    "за",
    "на",
    "не",
    "ни",
    "но",
    "же",
    "ли",
    "бы",
    "из",
    "до",
    "от",
    "об",
    "под",
    "при",
    "для",
    "про",
    // связки/местоимения
    "это",
    "эти",
    "этот",
    "эта",
    "там",
    "тут",
    "был",
    "были",
    "было",
    "была",
    "есть",
    "будет",
    "мы",
    "вы",
    "он",
    "она",
    "они",
    "нам",
    "вам",
    "его",
    "нас",
    "вас",
    "их",
    "такой",
    "такая",
    "такое",
    "итог",
    "итоге",
    "итогу",
    "всё",
    "все",
    "или",
    // [B26.2] слова периодов — их семантику берёт темпоральный префильтр
    // (router::period_range), в BM25 они только шумят
    "сегодня",
    "вчера",
    "позавчера",
    "назад",
    "неделю",
    "неделе",
    "недели",
    "неделя",
    "месяц",
    "месяца",
    "месяце",
    "году",
    "года",
    "год",
    "квартал",
    "квартала",
    "квартале",
    "прошлой",
    "прошлом",
    "прошлый",
    "прошлую",
    "прошлого",
];

fn is_stopword(token: &str) -> bool {
    let lower = token.to_lowercase();
    RU_STOPWORDS.contains(&lower.as_str())
}

/// Вопрос → FTS5 MATCH-выражение. `None` — искать нечего (нет валидных
/// токенов), вызывающий возвращает пустой результат без похода в БД.
/// [M16.3] Стоп-слова фильтруются; вопрос целиком из стоп-слов → откат на
/// нефильтрованный набор (честное «пусто» не должно стать ложным None).
pub(crate) fn build_match_expr(question: &str) -> Option<String> {
    let tokens: Vec<&str> = question
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && w.chars().count() >= MIN_TOKEN_CHARS)
        .collect();
    let meaningful: Vec<&str> = tokens.iter().copied().filter(|t| !is_stopword(t)).collect();
    let picked = if meaningful.is_empty() {
        tokens
    } else {
        meaningful
    };
    let terms: Vec<String> = picked
        .into_iter()
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

// ── [M15.11] Гибрид BM25 + cosine (RRF k=60, PRD §6.3) ──

/// Кандидатов на канал до слияния; финальная обрезка — прежние лимиты §4.2.
const FUSION_CANDIDATES: i64 = 30;

/// Гибридный поиск. Деградация (PRD §6.3): нет эмбеддера / пустой кэш
/// векторов → ветка Ph1 (`search`) без изменений поведения.
///
/// Инварианты контракта retrieval→budget→answer:
/// 1. Выход — `Vec<PassageHit>` **best-first**: budget жадно ест по порядку
///    и `rank` не читает; answer-fallback берёт top-3 по порядку.
/// 2. Call-scope: RRF **внутри** каждого прохода (own-fusion → top-8, затем
///    other-fusion → top-4, конкатенация) — свои безусловно раньше чужих.
/// 3. `rank` у гибридных хитов = RRF-score (больше = лучше) — поле
///    диагностическое, даунстрим его не потребляет.
pub async fn search_hybrid(
    pool: &SqlitePool,
    question: &str,
    scope: Scope<'_>,
    embedder: Option<std::sync::Arc<dyn crate::assistant::embedder::TextEmbedder>>,
    cache: &crate::assistant::embed_cache::EmbedCache,
    period: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<PassageHit>, AppError> {
    // [B26.2] Темпоральный префильтр: набор разрешённых call_id за явный
    // период из вопроса. BM25-ветки — пост-фильтр, cosine — в замыкании.
    let keep = |hits: Vec<PassageHit>| -> Vec<PassageHit> {
        match period {
            Some(set) => hits
                .into_iter()
                .filter(|h| set.contains(&h.call_id))
                .collect(),
            None => hits,
        }
    };
    let Some(emb) = embedder else {
        return Ok(keep(search(pool, question, scope).await?));
    };
    // Вопрос без валидных токенов — деген (Ph1 честно отдаёт пусто);
    // мусорный вектор по нему поднял бы случайные пассажи.
    let Some(expr) = build_match_expr(question) else {
        return Ok(Vec::new());
    };
    let rows = cache.snapshot(pool).await?;
    if rows.is_empty() {
        // Вектора ещё не насчитаны (backfill в пути) — чистый BM25.
        return Ok(keep(search(pool, question, scope).await?));
    }
    // Вектор вопроса — вне async-потока (ONNX ~5-20мс).
    let q = question.to_string();
    let emb_clone = emb.clone();
    let query_vec = tokio::task::spawn_blocking(move || emb_clone.embed_query(&q))
        .await
        .map_err(|e| AppError::Other(format!("embed query join: {e}")))??;

    match scope {
        Scope::Global => {
            fuse_pass(
                pool,
                &expr,
                &rows,
                &query_vec,
                GLOBAL_LIMIT,
                None,
                None,
                period,
            )
            .await
        }
        Scope::Call(call_id) => {
            let mut own = fuse_pass(
                pool,
                &expr,
                &rows,
                &query_vec,
                CALL_OWN_LIMIT,
                Some(call_id),
                None,
                period,
            )
            .await?;
            let other = fuse_pass(
                pool,
                &expr,
                &rows,
                &query_vec,
                CALL_OTHER_LIMIT,
                None,
                Some(call_id),
                period,
            )
            .await?;
            own.extend(other);
            Ok(own)
        }
    }
}

/// Один проход гибрида: BM25 top-30 + cosine top-30 → RRF → обрезка до
/// `final_limit` → материализация PassageHit (cosine-only id дозагружаются).
#[allow(clippy::too_many_arguments)]
async fn fuse_pass(
    pool: &SqlitePool,
    expr: &str,
    rows: &[crate::assistant::embed_cache::CachedVec],
    query_vec: &[f32],
    final_limit: i64,
    only_call: Option<&str>,
    exclude_call: Option<&str>,
    period: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<PassageHit>, AppError> {
    // [TD-22] Период — условие ВНУТРИ запроса, а не пост-фильтр. Раньше
    // BM25 брал глобальный топ-30 и только потом отбрасывал звонки не за
    // период: на большом архиве нужный звонок в топ-30 не попадал, и вопрос
    // «что обсуждали вчера про бюджет» получал ложное «ничего не найдено».
    // Cosine-канал так делал с самого начала — каналы расходились.
    let allowed: Option<Vec<String>> = period.map(|set| set.iter().cloned().collect());
    let bm25 = crate::db::assistant_search::search_fts_in_calls(
        pool,
        expr,
        FUSION_CANDIDATES,
        only_call,
        exclude_call,
        allowed.as_deref(),
    )
    .await?;
    let bm25_ids: Vec<i64> = bm25.iter().map(|h| h.id).collect();
    let cosine_ids = cosine_top_n(
        rows,
        query_vec,
        FUSION_CANDIDATES as usize,
        only_call,
        exclude_call,
        period,
    );
    let fused =
        crate::assistant::fusion::rrf_fuse(&bm25_ids, &cosine_ids, crate::assistant::fusion::RRF_K);

    let mut by_id: std::collections::HashMap<i64, PassageHit> =
        bm25.into_iter().map(|h| (h.id, h)).collect();
    let missing: Vec<i64> = fused
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !by_id.contains_key(id))
        .collect();
    for h in crate::db::assistant_embeddings::fetch_passages_by_ids(pool, &missing).await? {
        by_id.insert(h.id, h);
    }

    let mut out = Vec::with_capacity(final_limit as usize);
    for (id, score) in fused {
        if out.len() >= final_limit as usize {
            break;
        }
        // id без строки — гонка со стёртым звонком (stale-кэш) → скип.
        if let Some(mut hit) = by_id.remove(&id) {
            hit.rank = score;
            out.push(hit);
        }
    }
    Ok(out)
}

/// Top-N пассажей по cosine (dot: вектора L2-нормализованы — инвариант
/// `TextEmbedder`/`EmbedCache`). Тай-брейк по passage_id — стабильный выход.
fn cosine_top_n(
    rows: &[crate::assistant::embed_cache::CachedVec],
    query_vec: &[f32],
    n: usize,
    only_call: Option<&str>,
    exclude_call: Option<&str>,
    period: Option<&std::collections::HashSet<String>>,
) -> Vec<i64> {
    let mut scored: Vec<(f32, i64)> = rows
        .iter()
        .filter(|r| match (only_call, exclude_call) {
            (Some(call), _) => r.call_id == call,
            (None, Some(excl)) => r.call_id != excl,
            (None, None) => true,
        })
        // [B26.2] темпоральный префильтр (map_or: MSRV 1.77 без is_none_or)
        .filter(|r| period.map_or(true, |set| set.contains(&r.call_id)))
        // dim-mismatch (вектор чужой модели, гонка с ensure) — не сравним.
        .filter(|r| r.vec.len() == query_vec.len())
        .map(|r| {
            let dot: f32 = r.vec.iter().zip(query_vec).map(|(a, b)| a * b).sum();
            (dot, r.passage_id)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    scored.truncate(n);
    scored.into_iter().map(|(_, id)| id).collect()
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

    // [M16.3] Живой фейл Q2 (data-диагностика): стоп-токены «что/на/по»
    // раздували bm25 длинных транскриптов и топили единственный релевантный
    // recap. Стоплист режет их из MATCH.
    #[test]
    fn stopwords_are_filtered_from_match_expr() {
        assert_eq!(
            build_match_expr("Что решили на звонке по командам").as_deref(),
            Some("\"реши\"* OR \"звон\"* OR \"команд\"*")
        );
        assert_eq!(
            build_match_expr("Что там по делению команд").as_deref(),
            Some("\"делен\"* OR \"кома\"*")
        );
    }

    #[test]
    fn all_stopword_question_falls_back_unfiltered() {
        // Вопрос целиком из стоп-слов → откат на нефильтрованный набор,
        // НЕ None (иначе честный empty превратился бы в ложный).
        assert_eq!(
            build_match_expr("что по нам").as_deref(),
            Some("\"что\" OR \"по\" OR \"нам\"")
        );
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

    // ── [M15.11] Гибрид BM25+cosine (KeywordMock: контролируемая семантика) ──

    use std::sync::Arc;

    use crate::assistant::embed_cache::EmbedCache;
    use crate::assistant::embedder::{l2_normalize, TextEmbedder};

    /// Тест-эмбеддер с «семантикой» по ключевым словам: синонимы одной группы
    /// попадают в одну координату → cosine≈1 при разной лексике — ровно то,
    /// что BM25 не умеет (golden-кейс ROADMAP M15.11).
    struct KeywordMockEmbedder;

    fn keyword_vec(text: &str) -> Vec<f32> {
        let t = text.to_lowercase();
        let mut v = vec![0.0f32; 8];
        if t.contains("срок") || t.contains("дедлайн") {
            v[0] = 1.0;
        }
        if t.contains("дизайн") || t.contains("палитр") {
            v[1] = 1.0;
        }
        if t.contains("бюджет") {
            v[2] = 1.0;
        }
        v[7] = 0.1; // общий фон — вектора без ключевых слов не нулевые
        l2_normalize(&mut v);
        v
    }

    impl TextEmbedder for KeywordMockEmbedder {
        fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, AppError> {
            Ok(texts.iter().map(|t| keyword_vec(t)).collect())
        }
        fn embed_query(&self, question: &str) -> Result<Vec<f32>, AppError> {
            Ok(keyword_vec(question))
        }
        fn dim(&self) -> usize {
            8
        }
    }

    /// Векторизовать все засеянные пассажи KeywordMock'ом.
    async fn embed_all(pool: &SqlitePool) {
        let rows = crate::db::assistant_embeddings::list_passages_missing_embedding(pool, 1000)
            .await
            .unwrap();
        let blobs: Vec<(i64, Vec<u8>)> = rows
            .iter()
            .map(|(id, t)| (*id, crate::embeddings::embedding_to_bytes(&keyword_vec(t))))
            .collect();
        crate::db::assistant_embeddings::upsert_embeddings(pool, 8, &blobs)
            .await
            .unwrap();
    }

    fn kw() -> Option<Arc<dyn TextEmbedder>> {
        Some(Arc::new(KeywordMockEmbedder))
    }

    #[tokio::test]
    async fn hybrid_finds_synonym_passage_bm25_misses() {
        let db = fresh_db().await;
        seed(
            &db.pool,
            "c1",
            &[
                "Дедлайн подписания договора тридцатое мая",
                "Обсудили дизайн лендинга и палитру бренда",
            ],
        )
        .await;
        embed_all(&db.pool).await;
        let cache = EmbedCache::new();

        // BM25 по «какие сроки?» — мимо (лексика не совпадает).
        assert!(search(&db.pool, "какие сроки?", Scope::Global)
            .await
            .unwrap()
            .is_empty());

        // Гибрид достаёт синонимный пассаж через cosine-канал, включая
        // материализацию cosine-only id (в BM25-листе его нет).
        let hy = search_hybrid(&db.pool, "какие сроки?", Scope::Global, kw(), &cache, None)
            .await
            .unwrap();
        assert!(!hy.is_empty(), "вектор обязан найти синоним");
        assert!(hy[0].text.contains("Дедлайн"), "top-1: {}", hy[0].text);
        assert!(hy[0].rank > 0.0, "rank = RRF-score");
    }

    #[tokio::test]
    async fn hybrid_without_embedder_equals_ph1() {
        let db = fresh_db().await;
        seed(&db.pool, "c1", &["обсудили бюджет пилота", "прочее"]).await;
        embed_all(&db.pool).await;
        let cache = EmbedCache::new();

        let ph1 = search(&db.pool, "бюджет", Scope::Global).await.unwrap();
        let hy = search_hybrid(&db.pool, "бюджет", Scope::Global, None, &cache, None)
            .await
            .unwrap();
        let ids = |v: &[PassageHit]| v.iter().map(|h| h.id).collect::<Vec<_>>();
        assert_eq!(ids(&ph1), ids(&hy), "None-эмбеддер = ветка Ph1");
    }

    #[tokio::test]
    async fn hybrid_with_empty_vector_cache_falls_back_to_bm25() {
        let db = fresh_db().await;
        seed(&db.pool, "c1", &["обсудили бюджет пилота"]).await;
        // Векторов нет (backfill «ещё не бежал»).
        let cache = EmbedCache::new();

        let hy = search_hybrid(&db.pool, "бюджет", Scope::Global, kw(), &cache, None)
            .await
            .unwrap();
        assert_eq!(hy.len(), 1, "пустой кэш → чистый BM25");
    }

    #[tokio::test]
    async fn hybrid_call_scope_keeps_own_before_other() {
        let db = fresh_db().await;
        // У «чужого» звонка совпадение сильнее (бюджет + дедлайн), но свои
        // пассажи обязаны идти раньше — RRF внутри прохода, не поверх.
        seed(&db.pool, "c1", &["немного про бюджет"]).await;
        seed(&db.pool, "c2", &["Бюджет маркетинга и дедлайн финализации"]).await;
        embed_all(&db.pool).await;
        let cache = EmbedCache::new();

        let hy = search_hybrid(&db.pool, "бюджет", Scope::Call("c1"), kw(), &cache, None)
            .await
            .unwrap();
        assert!(hy.len() >= 2);
        assert_eq!(hy[0].call_id, "c1", "свои раньше чужих");
        assert_eq!(hy.last().unwrap().call_id, "c2");
    }

    #[tokio::test]
    async fn hybrid_output_is_stable_between_runs() {
        let db = fresh_db().await;
        seed(
            &db.pool,
            "c1",
            &["дедлайн проекта", "сроки согласования", "дизайн главной"],
        )
        .await;
        embed_all(&db.pool).await;
        let cache = EmbedCache::new();

        let a = search_hybrid(&db.pool, "какие сроки?", Scope::Global, kw(), &cache, None)
            .await
            .unwrap();
        let b = search_hybrid(&db.pool, "какие сроки?", Scope::Global, kw(), &cache, None)
            .await
            .unwrap();
        let ids = |v: &[PassageHit]| v.iter().map(|h| h.id).collect::<Vec<_>>();
        assert_eq!(ids(&a), ids(&b), "детерминированный порядок (тай-брейки)");
    }
}
