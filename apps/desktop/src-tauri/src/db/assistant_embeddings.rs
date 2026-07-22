//! [M15.10] Репозиторий векторов пассажей ассистента (`assistant_embeddings`,
//! миграция 0020). Отдельный от `db/assistant.rs`: тот на пределе 800-строк,
//! а embeddings — самостоятельная ког-единица (Ph2 retrieval).
//!
//! BLOB-формат — little-endian f32 (`embeddings.rs::embedding_to_bytes`),
//! `dim` per-row (текстовый e5-small = 384 ≠ голосовой 256). Каскады чистят
//! вектора при переиндексации/удалении звонка — код не обязан.

// Потребители подключаются в M15.10.4 (embed-hook индексера) и M15.11
// (retrieval-кэш) — до врезки dead_code allow (паттерн embedder.rs).
#![allow(dead_code)]

use sqlx::SqlitePool;

use crate::AppError;

/// Строка кэша векторов. `vec` — сырой BLOB, декодирование f32 — на стороне
/// кэша (`bytes_to_embedding`), чтобы битые строки скипались, не роняя всё.
#[derive(Debug, Clone)]
pub struct EmbeddingRow {
    pub passage_id: i64,
    pub call_id: String,
    pub dim: i64,
    pub vec: Vec<u8>,
}

/// Штамп инвалидации in-memory кэша (PRD §6.3: «инвалидация по
/// assistant_index_state»). Меняется при любой (пере)индексации звонка и
/// при изменении числа векторов (backfill / clear).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingStamp {
    pub indexed_calls: i64,
    pub last_indexed_at: String,
    pub embedding_count: i64,
}

/// id + текст всех пассажей звонка — вход batch-эмбеддинга сразу после
/// `replace_call_passages` (id вставленных строк тот не возвращает).
pub async fn list_call_passage_texts(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Vec<(i64, String)>, AppError> {
    sqlx::query_as("SELECT id, text FROM assistant_passages WHERE call_id = ?1 ORDER BY id ASC")
        .bind(call_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::from)
}

/// Пассажи без вектора — батч фонового backfill'а. Порядок стабильный (id).
pub async fn list_passages_missing_embedding(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<(i64, String)>, AppError> {
    sqlx::query_as(
        "SELECT p.id, p.text FROM assistant_passages p
         LEFT JOIN assistant_embeddings e ON e.passage_id = p.id
         WHERE e.passage_id IS NULL
         ORDER BY p.id ASC
         LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

/// Batch-upsert векторов одной транзакцией. `rows` — (passage_id, BLOB f32 LE).
pub async fn upsert_embeddings(
    pool: &SqlitePool,
    dim: i64,
    rows: &[(i64, Vec<u8>)],
) -> Result<(), AppError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    for (passage_id, blob) in rows {
        sqlx::query(
            "INSERT INTO assistant_embeddings (passage_id, dim, vec) VALUES (?1, ?2, ?3)
             ON CONFLICT(passage_id) DO UPDATE SET dim = excluded.dim, vec = excluded.vec",
        )
        .bind(passage_id)
        .bind(dim)
        .bind(blob.as_slice())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Все вектора для in-memory кэша retrieval (M15.11). ~46MB на масштабе
/// 1000 звонков (PRD §5.2) — грузится целиком.
pub async fn load_all_embeddings(pool: &SqlitePool) -> Result<Vec<EmbeddingRow>, AppError> {
    let rows: Vec<(i64, String, i64, Vec<u8>)> = sqlx::query_as(
        "SELECT e.passage_id, p.call_id, e.dim, e.vec
         FROM assistant_embeddings e
         JOIN assistant_passages p ON p.id = e.passage_id
         ORDER BY e.passage_id ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(passage_id, call_id, dim, vec)| EmbeddingRow {
            passage_id,
            call_id,
            dim,
            vec,
        })
        .collect())
}

/// Снести все вектора — смена модели эмбеддера (M15.10.3): backfill
/// пересчитает новой моделью.
pub async fn clear_embeddings(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::query("DELETE FROM assistant_embeddings")
        .execute(pool)
        .await?;
    Ok(())
}

/// Текущий штамп инвалидации кэша — один дешёвый запрос перед поиском.
pub async fn embedding_stamp(pool: &SqlitePool) -> Result<EmbeddingStamp, AppError> {
    let (indexed_calls, last_indexed_at, embedding_count): (i64, String, i64) = sqlx::query_as(
        "SELECT
           (SELECT COUNT(*) FROM assistant_index_state),
           (SELECT COALESCE(MAX(indexed_at), '') FROM assistant_index_state),
           (SELECT COUNT(*) FROM assistant_embeddings)",
    )
    .fetch_one(pool)
    .await?;
    Ok(EmbeddingStamp {
        indexed_calls,
        last_indexed_at,
        embedding_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::types::AssistantPassageKind;
    use crate::db::assistant::{replace_call_passages, PassageInput};
    use crate::db::test_support::fresh_db;
    use crate::embeddings::{bytes_to_embedding, embedding_to_bytes};

    async fn insert_dummy_call(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 60, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    fn passage(text: &str) -> PassageInput {
        PassageInput {
            kind: AssistantPassageKind::Transcript,
            speaker: None,
            start_ms: Some(0),
            end_ms: Some(10_000),
            text: text.into(),
            token_est: 10,
        }
    }

    async fn count_embeddings(pool: &SqlitePool) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM assistant_embeddings")
            .fetch_one(pool)
            .await
            .unwrap();
        n
    }

    #[tokio::test]
    async fn upsert_and_load_roundtrip_via_blob_helpers() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        replace_call_passages(&db.pool, "c1", &[passage("первый"), passage("второй")])
            .await
            .unwrap();

        let texts = list_call_passage_texts(&db.pool, "c1").await.unwrap();
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].1, "первый");

        let v = vec![0.5f32, -0.25, 1.0];
        let rows: Vec<(i64, Vec<u8>)> = texts
            .iter()
            .map(|(id, _)| (*id, embedding_to_bytes(&v)))
            .collect();
        upsert_embeddings(&db.pool, 3, &rows).await.unwrap();

        let loaded = load_all_embeddings(&db.pool).await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].call_id, "c1");
        assert_eq!(loaded[0].dim, 3);
        assert_eq!(bytes_to_embedding(&loaded[0].vec).unwrap(), v);

        // Upsert перезаписывает (не дублирует).
        upsert_embeddings(&db.pool, 3, &rows).await.unwrap();
        assert_eq!(count_embeddings(&db.pool).await, 2);
    }

    #[tokio::test]
    async fn missing_embedding_listing_shrinks_after_upsert() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        replace_call_passages(&db.pool, "c1", &[passage("a"), passage("b")])
            .await
            .unwrap();

        let missing = list_passages_missing_embedding(&db.pool, 10).await.unwrap();
        assert_eq!(missing.len(), 2);

        let first = missing[0].0;
        upsert_embeddings(&db.pool, 2, &[(first, embedding_to_bytes(&[1.0, 0.0]))])
            .await
            .unwrap();

        let missing = list_passages_missing_embedding(&db.pool, 10).await.unwrap();
        assert_eq!(missing.len(), 1);
        assert_ne!(missing[0].0, first);

        // Лимит батча уважается.
        let limited = list_passages_missing_embedding(&db.pool, 1).await.unwrap();
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn cascades_wipe_embeddings_on_reindex_and_call_delete() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        replace_call_passages(&db.pool, "c1", &[passage("x")]).await.unwrap();
        let texts = list_call_passage_texts(&db.pool, "c1").await.unwrap();
        upsert_embeddings(
            &db.pool,
            1,
            &[(texts[0].0, embedding_to_bytes(&[1.0]))],
        )
        .await
        .unwrap();
        assert_eq!(count_embeddings(&db.pool).await, 1);

        // Переиндексация = DELETE+INSERT пассажей → каскад сносит вектора.
        replace_call_passages(&db.pool, "c1", &[passage("y")]).await.unwrap();
        assert_eq!(
            count_embeddings(&db.pool).await,
            0,
            "reindex обязан сбросить вектора каскадом"
        );

        // DELETE звонка → каскад через passages.
        let texts = list_call_passage_texts(&db.pool, "c1").await.unwrap();
        upsert_embeddings(
            &db.pool,
            1,
            &[(texts[0].0, embedding_to_bytes(&[1.0]))],
        )
        .await
        .unwrap();
        sqlx::query("DELETE FROM calls WHERE id = 'c1'")
            .execute(&db.pool)
            .await
            .unwrap();
        assert_eq!(count_embeddings(&db.pool).await, 0);
    }

    #[tokio::test]
    async fn stamp_changes_on_index_upsert_and_clear() {
        let db = fresh_db().await;
        let s0 = embedding_stamp(&db.pool).await.unwrap();
        assert_eq!(s0.indexed_calls, 0);
        assert_eq!(s0.embedding_count, 0);

        insert_dummy_call(&db.pool, "c1").await;
        replace_call_passages(&db.pool, "c1", &[passage("a")]).await.unwrap();
        let s1 = embedding_stamp(&db.pool).await.unwrap();
        assert_ne!(s0, s1, "индексация меняет штамп");

        let texts = list_call_passage_texts(&db.pool, "c1").await.unwrap();
        upsert_embeddings(
            &db.pool,
            1,
            &[(texts[0].0, embedding_to_bytes(&[1.0]))],
        )
        .await
        .unwrap();
        let s2 = embedding_stamp(&db.pool).await.unwrap();
        assert_ne!(s1, s2, "новые вектора меняют штамп");

        clear_embeddings(&db.pool).await.unwrap();
        let s3 = embedding_stamp(&db.pool).await.unwrap();
        assert_eq!(s3.embedding_count, 0);
        assert_ne!(s2, s3, "clear меняет штамп");
    }
}
