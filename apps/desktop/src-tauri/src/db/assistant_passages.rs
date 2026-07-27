//! [M15.3] Пассажи индекса ассистента: запись, удаление и статистика.
//!
//! [TD-41] Выделено из `db/assistant.rs` (806 строк при лимите 800, правило 8)
//! вместе с тестами. Граница естественная: чаты и сообщения — переписка
//! пользователя, пассажи — производная от звонка, которую пересобирает
//! индексер. FTS-таблицу синхронизируют триггеры миграции 0019, поэтому
//! тесты здесь проверяют именно их. Логика не менялась.

use sqlx::SqlitePool;

use crate::assistant::types::{AssistantIndexStats, AssistantPassageKind};
use crate::AppError;

use super::assistant::now_iso;

// ── Пассажи индекса ───────────────────────────────────────────────────

/// Input пассажа при индексации (M15.3 передаёт из passage builder).
/// `kind` — enum: невалидное значение не соберётся, CHECK в SQL не сработает.
#[derive(Debug, Clone)]
pub struct PassageInput {
    pub kind: AssistantPassageKind,
    pub speaker: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub text: String,
    pub token_est: i64,
}

/// Заменить все пассажи звонка (DELETE + INSERT + upsert index_state, одна tx).
/// Возвращает (passage_count, token_total).
pub async fn replace_call_passages(
    pool: &SqlitePool,
    call_id: &str,
    passages: &[PassageInput],
) -> Result<(i64, i64), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM assistant_passages WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    let mut count = 0i64;
    let mut tokens = 0i64;
    for p in passages {
        let text = p.text.trim();
        if text.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO assistant_passages (call_id, kind, speaker, start_ms, end_ms, text, token_est)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(call_id)
        .bind(p.kind.as_str())
        .bind(p.speaker.as_deref())
        .bind(p.start_ms)
        .bind(p.end_ms)
        .bind(text)
        .bind(p.token_est)
        .execute(&mut *tx)
        .await?;
        count += 1;
        tokens += p.token_est;
    }
    sqlx::query(
        "INSERT INTO assistant_index_state (call_id, indexed_at, passage_count, token_total)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(call_id) DO UPDATE SET
           indexed_at = excluded.indexed_at,
           passage_count = excluded.passage_count,
           token_total = excluded.token_total",
    )
    .bind(call_id)
    .bind(now_iso())
    .bind(count)
    .bind(tokens)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((count, tokens))
}

/// Деиндексация звонка (reprocess): пассажи + index_state. FTS чистят триггеры.
pub async fn delete_call_passages(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM assistant_passages WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM assistant_index_state WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Сбросить только index_state (self-heal: звонок вернётся в backfill-sweep).
/// Пассажи НЕ трогаем — до переиндексации поиск работает по старым.
pub async fn clear_index_state(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM assistant_index_state WHERE call_id = ?1")
        .bind(call_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub use crate::db::assistant_search::search_fts;

/// Хит полнотекстового поиска. rank = bm25 (меньше = релевантнее).
#[derive(Debug, Clone)]
pub struct PassageHit {
    pub id: i64,
    pub call_id: String,
    pub kind: String,
    pub speaker: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub text: String,
    pub token_est: i64,
    pub rank: f64,
}

/// Статистика для чипа «в поиске X из Y звонков · ЧЧ ч ММ мин».
/// X = проиндексированные ready-звонки, Y = все звонки, duration — по X.
pub async fn index_stats(pool: &SqlitePool) -> Result<AssistantIndexStats, AppError> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT COUNT(*) FROM assistant_index_state),
           (SELECT COUNT(*) FROM calls),
           (SELECT COALESCE(SUM(c.duration_sec), 0)
              FROM assistant_index_state s JOIN calls c ON c.id = s.call_id)",
    )
    .fetch_one(pool)
    .await?;
    Ok(AssistantIndexStats {
        indexed_calls: row.0 as u32,
        total_calls: row.1 as u32,
        total_duration_sec: row.2.max(0) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    async fn insert_dummy_call(pool: &SqlitePool, id: &str, duration_sec: i64) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, ?2, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .bind(duration_sec)
        .execute(pool)
        .await
        .unwrap();
    }

    fn passage(kind: AssistantPassageKind, text: &str, start_ms: Option<i64>) -> PassageInput {
        PassageInput {
            kind,
            speaker: Some("Дмитрий Петров".into()),
            start_ms,
            end_ms: start_ms.map(|s| s + 10_000),
            text: text.into(),
            token_est: (text.len() / 4) as i64,
        }
    }

    #[tokio::test]
    async fn fts_trigger_sync_insert_replace_delete() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;

        // Insert → MATCH находит.
        replace_call_passages(
            &db.pool,
            "c1",
            &[
                passage(
                    AssistantPassageKind::Transcript,
                    "обсуждали приватность и локальный режим",
                    Some(62_000),
                ),
                passage(
                    AssistantPassageKind::Decision,
                    "показываем локальный режим на демо",
                    None,
                ),
            ],
        )
        .await
        .unwrap();
        let hits = search_fts(&db.pool, "\"приватность\"", 10, None, None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].call_id, "c1");
        assert_eq!(hits[0].kind, "transcript");
        assert_eq!(hits[0].start_ms, Some(62_000));

        // Re-replace (идемпотентная переиндексация) → старый текст исчез.
        replace_call_passages(
            &db.pool,
            "c1",
            &[passage(
                AssistantPassageKind::Recap,
                "итог: пилот на двадцать мест",
                None,
            )],
        )
        .await
        .unwrap();
        let old = search_fts(&db.pool, "\"приватность\"", 10, None, None)
            .await
            .unwrap();
        assert!(old.is_empty(), "replaced passages must leave FTS");
        let new = search_fts(&db.pool, "\"пилот\"", 10, None, None)
            .await
            .unwrap();
        assert_eq!(new.len(), 1);

        // Deindex → пусто.
        delete_call_passages(&db.pool, "c1").await.unwrap();
        let after = search_fts(&db.pool, "\"пилот\"", 10, None, None)
            .await
            .unwrap();
        assert!(after.is_empty());
        assert_eq!(index_stats(&db.pool).await.unwrap().indexed_calls, 0);
    }

    #[tokio::test]
    async fn fts_scope_filters_only_and_exclude() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        insert_dummy_call(&db.pool, "c2", 900).await;
        replace_call_passages(
            &db.pool,
            "c1",
            &[passage(
                AssistantPassageKind::Transcript,
                "бюджет проекта",
                Some(1_000),
            )],
        )
        .await
        .unwrap();
        replace_call_passages(
            &db.pool,
            "c2",
            &[passage(
                AssistantPassageKind::Transcript,
                "бюджет отдела",
                Some(2_000),
            )],
        )
        .await
        .unwrap();

        let only = search_fts(&db.pool, "\"бюджет\"", 10, Some("c1"), None)
            .await
            .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].call_id, "c1");

        let excl = search_fts(&db.pool, "\"бюджет\"", 10, None, Some("c1"))
            .await
            .unwrap();
        assert_eq!(excl.len(), 1);
        assert_eq!(excl[0].call_id, "c2");
    }

    #[tokio::test]
    async fn fts_prefix_query_matches_russian_inflection() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        replace_call_passages(
            &db.pool,
            "c1",
            &[passage(
                AssistantPassageKind::Transcript,
                "говорили о приватности данных",
                Some(5_000),
            )],
        )
        .await
        .unwrap();
        // Префикс-запрос (заготовка под M15.5): "приватн"* ловит «приватности».
        let hits = search_fts(&db.pool, "\"приватн\"*", 10, None, None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn replace_skips_blank_text_and_counts_tokens() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        let (count, tokens) = replace_call_passages(
            &db.pool,
            "c1",
            &[
                passage(AssistantPassageKind::Transcript, "непустой текст", Some(0)),
                passage(AssistantPassageKind::Transcript, "   ", Some(1_000)),
            ],
        )
        .await
        .unwrap();
        assert_eq!(count, 1);
        assert!(tokens > 0);
        let stats = index_stats(&db.pool).await.unwrap();
        assert_eq!(stats.indexed_calls, 1);
        assert_eq!(stats.total_calls, 1);
        assert_eq!(stats.total_duration_sec, 600);
    }
}
