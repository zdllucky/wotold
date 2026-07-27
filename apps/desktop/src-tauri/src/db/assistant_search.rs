//! [TD-22] FTS5-поиск по индексу ассистента.
//!
//! Выделен из `db/assistant.rs` — тот упёрся в лимит когезии 800 строк, а
//! фикс требовал добавить в запрос темпоральное условие (правило 8). Сосед
//! `db/assistant_embeddings.rs` заведён по тому же принципу.

use sqlx::SqlitePool;

use crate::db::assistant::PassageHit;
use crate::AppError;

/// FTS5 MATCH по индексу.
///
/// `match_expr` ДОЛЖЕН быть заранее экранирован вызывающей стороной
/// (retrieval M15.5: каждый токен в кавычках) — сырой пользовательский ввод
/// сюда не передавать (MATCH-синтаксис-инъекция).
pub async fn search_fts(
    pool: &SqlitePool,
    match_expr: &str,
    limit: i64,
    only_call: Option<&str>,
    exclude_call: Option<&str>,
) -> Result<Vec<PassageHit>, AppError> {
    search_fts_in_calls(pool, match_expr, limit, only_call, exclude_call, None).await
}

/// [TD-22] То же, но с ограничением по набору звонков **внутри** запроса.
///
/// Раньше период применялся пост-фильтром: BM25 отбирал глобальный топ-30,
/// и только потом из него выбрасывались звонки не за период. На большом
/// архиве вчерашний звонок в топ-30 просто не попадал, и вопрос «что
/// обсуждали вчера про бюджет» отвечался ложным «ничего не найдено» — при
/// том что ответ в базе есть. Cosine-канал фильтровал правильно (условие
/// стоит в отборе кандидатов), BM25 — нет; каналы расходились.
///
/// `allowed` — id звонков за период. `None` — без ограничения. Пустой срез
/// означает «ни одного подходящего звонка» и сразу даёт пустой результат:
/// строить `IN ()` нельзя, да и запрос заведомо ничего не вернёт.
pub async fn search_fts_in_calls(
    pool: &SqlitePool,
    match_expr: &str,
    limit: i64,
    only_call: Option<&str>,
    exclude_call: Option<&str>,
    allowed: Option<&[String]>,
) -> Result<Vec<PassageHit>, AppError> {
    type Row = (
        i64,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        String,
        i64,
        f64,
    );
    if allowed.is_some_and(<[String]>::is_empty) {
        return Ok(Vec::new());
    }
    // Плейсхолдеры под IN — по образцу `prune_call_speakers_not_in`. Сами
    // значения биндятся, в SQL уезжает только их количество.
    let in_clause = allowed
        .map(|ids| format!(" AND p.call_id IN ({})", vec!["?"; ids.len()].join(",")))
        .unwrap_or_default();
    let sql = format!(
        "SELECT p.id, p.call_id, p.kind, p.speaker, p.start_ms, p.end_ms,
                p.text, p.token_est, bm25(assistant_fts) AS rank
         FROM assistant_fts
         JOIN assistant_passages p ON p.id = assistant_fts.rowid
         WHERE assistant_fts MATCH ?
           AND (? IS NULL OR p.call_id = ?)
           AND (? IS NULL OR p.call_id <> ?){in_clause}
         ORDER BY rank ASC
         LIMIT ?"
    );
    let mut q = sqlx::query_as(&sql)
        .bind(match_expr)
        .bind(only_call)
        .bind(only_call)
        .bind(exclude_call)
        .bind(exclude_call);
    for id in allowed.unwrap_or(&[]) {
        q = q.bind(id);
    }
    let rows: Vec<Row> = q.bind(limit).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| PassageHit {
            id: r.0,
            call_id: r.1,
            kind: r.2,
            speaker: r.3,
            start_ms: r.4,
            end_ms: r.5,
            text: r.6,
            token_est: r.7,
            rank: r.8,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::assistant::{replace_call_passages, PassageInput};
    use crate::db::test_support::fresh_db;

    async fn seed(pool: &SqlitePool, id: &str, started_at: &str, texts: &[&str]) {
        sqlx::query(
            "INSERT INTO calls (id, title, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, ?1, ?2, 600, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .bind(started_at)
        .execute(pool)
        .await
        .unwrap();
        let passages: Vec<PassageInput> = texts
            .iter()
            .map(|t| PassageInput {
                kind: crate::assistant::types::AssistantPassageKind::Transcript,
                speaker: None,
                start_ms: Some(0),
                end_ms: Some(1000),
                text: (*t).to_string(),
                token_est: 8,
            })
            .collect();
        replace_call_passages(pool, id, &passages).await.unwrap();
    }

    #[tokio::test]
    async fn period_filter_beats_the_limit_not_the_other_way_round() {
        // Регрессия TD-22. Раньше период применялся ПОСЛЕ `LIMIT`: BM25
        // отбирал глобальный топ-N, и нужный звонок в него не попадал, если
        // более релевантных было больше N. Ответ — ложное «ничего не найдено».
        //
        // Строим ровно эту ситуацию: 5 «шумных» звонков с той же темой стоят
        // в выдаче выше, лимит равен 3. Без условия в SQL искомый звонок
        // отсекается лимитом ещё до фильтрации.
        let db = fresh_db().await;
        for i in 0..5 {
            seed(
                &db.pool,
                &format!("noise{i}"),
                &format!("2020-01-0{}T09:00:00+00:00", i + 1),
                &["бюджет бюджет бюджет бюджет"],
            )
            .await;
        }
        seed(
            &db.pool,
            "target",
            "2026-07-26T09:00:00+00:00",
            &["обсудили бюджет на следующий квартал"],
        )
        .await;

        let allowed = vec!["target".to_string()];
        let hits = search_fts_in_calls(&db.pool, "\"бюджет\"*", 3, None, None, Some(&allowed))
            .await
            .unwrap();
        assert!(
            hits.iter().any(|h| h.call_id == "target"),
            "звонок за период обязан находиться, а не отсекаться лимитом; получили {:?}",
            hits.iter().map(|h| &h.call_id).collect::<Vec<_>>()
        );

        // Контроль: без ограничения тот же лимит его действительно теряет.
        let unfiltered = search_fts_in_calls(&db.pool, "\"бюджет\"*", 3, None, None, None)
            .await
            .unwrap();
        assert_eq!(unfiltered.len(), 3, "лимит работает");
    }

    #[tokio::test]
    async fn empty_allowed_set_returns_nothing_without_broken_sql() {
        // `IN ()` — синтаксическая ошибка в SQLite. Пустой набор значит
        // «подходящих звонков нет», и ответ обязан быть пустым, а не Err.
        let db = fresh_db().await;
        seed(&db.pool, "c1", "2026-07-26T09:00:00+00:00", &["бюджет"]).await;
        let hits = search_fts_in_calls(&db.pool, "\"бюджет\"*", 10, None, None, Some(&[]))
            .await
            .unwrap();
        assert!(hits.is_empty());
    }
}
