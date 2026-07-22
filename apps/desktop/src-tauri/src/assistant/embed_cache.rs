//! [M15.10.5] In-memory кэш векторов пассажей — источник cosine-канала
//! retrieval (M15.11). Инвалидация по штампу `EmbeddingStamp` (PRD §6.3:
//! «по assistant_index_state»): один дешёвый запрос перед каждым поиском,
//! mismatch → полная перезагрузка (~46MB на масштабе 1000 звонков, PRD §5.2).
//!
//! Инвариант: вектора в БД уже L2-нормализованы (гарантия `TextEmbedder`) —
//! cosine дальше считается как dot без ре-нормализации.

// Потребитель — retrieval M15.11; до врезки dead_code allow (паттерн embedder.rs).
#![allow(dead_code)]

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::db::assistant_embeddings::{embedding_stamp, load_all_embeddings, EmbeddingStamp};
use crate::embeddings::bytes_to_embedding;
use crate::AppError;

/// Строка кэша: декодированный L2-нормализованный вектор + привязка к звонку
/// (scope-фильтры call-scope в retrieval).
pub struct CachedVec {
    pub passage_id: i64,
    pub call_id: String,
    pub vec: Vec<f32>,
}

struct CacheState {
    stamp: EmbeddingStamp,
    rows: Arc<Vec<CachedVec>>,
}

/// Кэш — инстансный (тесты изолируются своим экземпляром; у fresh_db-баз
/// пустые штампы совпадают, глобальный static их бы перепутал). Прод берёт
/// процессный синглтон через `global()`.
pub struct EmbedCache(tokio::sync::Mutex<Option<CacheState>>);

impl EmbedCache {
    pub fn new() -> Self {
        Self(tokio::sync::Mutex::new(None))
    }

    /// Снимок, консистентный текущему штампу БД. Битые строки (кривой BLOB,
    /// dim-mismatch) скипаются с warn — одна повреждённая запись не роняет
    /// весь семантический канал.
    pub async fn snapshot(&self, pool: &SqlitePool) -> Result<Arc<Vec<CachedVec>>, AppError> {
        let stamp = embedding_stamp(pool).await?;
        let mut guard = self.0.lock().await;
        if let Some(state) = guard.as_ref() {
            if state.stamp == stamp {
                return Ok(state.rows.clone());
            }
        }
        let raw = load_all_embeddings(pool).await?;
        let mut rows = Vec::with_capacity(raw.len());
        for r in raw {
            match bytes_to_embedding(&r.vec) {
                Ok(v) if v.len() == r.dim as usize => rows.push(CachedVec {
                    passage_id: r.passage_id,
                    call_id: r.call_id,
                    vec: v,
                }),
                Ok(v) => log::warn!(
                    "embed cache: passage {} dim mismatch ({} != {}), skipped",
                    r.passage_id,
                    v.len(),
                    r.dim
                ),
                Err(e) => log::warn!("embed cache: passage {} bad blob: {e}", r.passage_id),
            }
        }
        let rows = Arc::new(rows);
        *guard = Some(CacheState {
            stamp,
            rows: rows.clone(),
        });
        Ok(rows)
    }
}

impl Default for EmbedCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Процессный синглтон для прод-пути retrieval.
pub fn global() -> &'static EmbedCache {
    static GLOBAL: std::sync::OnceLock<EmbedCache> = std::sync::OnceLock::new();
    GLOBAL.get_or_init(EmbedCache::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::types::AssistantPassageKind;
    use crate::db::assistant::{replace_call_passages, PassageInput};
    use crate::db::assistant_embeddings::{list_call_passage_texts, upsert_embeddings};
    use crate::db::test_support::fresh_db;
    use crate::embeddings::embedding_to_bytes;

    async fn seed(pool: &SqlitePool, call_id: &str, texts: &[&str]) -> Vec<i64> {
        sqlx::query(
            "INSERT INTO calls (id, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 60, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(call_id)
        .execute(pool)
        .await
        .unwrap();
        let passages: Vec<PassageInput> = texts
            .iter()
            .map(|t| PassageInput {
                kind: AssistantPassageKind::Transcript,
                speaker: None,
                start_ms: Some(0),
                end_ms: Some(1000),
                text: (*t).into(),
                token_est: 2,
            })
            .collect();
        replace_call_passages(pool, call_id, &passages).await.unwrap();
        list_call_passage_texts(pool, call_id)
            .await
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    #[tokio::test]
    async fn snapshot_reuses_arc_until_stamp_changes() {
        let db = fresh_db().await;
        let cache = EmbedCache::new();
        let ids = seed(&db.pool, "c1", &["a", "b"]).await;
        upsert_embeddings(
            &db.pool,
            2,
            &[(ids[0], embedding_to_bytes(&[1.0, 0.0]))],
        )
        .await
        .unwrap();

        let s1 = cache.snapshot(&db.pool).await.unwrap();
        assert_eq!(s1.len(), 1);
        let s2 = cache.snapshot(&db.pool).await.unwrap();
        assert!(Arc::ptr_eq(&s1, &s2), "штамп не менялся — тот же снимок");

        upsert_embeddings(
            &db.pool,
            2,
            &[(ids[1], embedding_to_bytes(&[0.0, 1.0]))],
        )
        .await
        .unwrap();
        let s3 = cache.snapshot(&db.pool).await.unwrap();
        assert_eq!(s3.len(), 2, "новый вектор → перезагрузка");
        assert!(!Arc::ptr_eq(&s1, &s3));
        assert_eq!(s3[0].call_id, "c1");
    }

    #[tokio::test]
    async fn snapshot_skips_corrupt_rows() {
        let db = fresh_db().await;
        let cache = EmbedCache::new();
        let ids = seed(&db.pool, "c1", &["a", "b", "c"]).await;
        // Валидный, dim-mismatch (заявлено 3, в BLOB 2), кривой BLOB (5 байт).
        upsert_embeddings(&db.pool, 3, &[(ids[0], embedding_to_bytes(&[1.0, 0.0, 0.0]))])
            .await
            .unwrap();
        upsert_embeddings(&db.pool, 3, &[(ids[1], embedding_to_bytes(&[1.0, 0.0]))])
            .await
            .unwrap();
        sqlx::query("INSERT INTO assistant_embeddings (passage_id, dim, vec) VALUES (?1, 3, X'0102030405')")
            .bind(ids[2])
            .execute(&db.pool)
            .await
            .unwrap();

        let snap = cache.snapshot(&db.pool).await.unwrap();
        assert_eq!(snap.len(), 1, "битые строки скипнуты, валидная жива");
        assert_eq!(snap[0].passage_id, ids[0]);
    }

    #[tokio::test]
    async fn reindex_cascade_invalidates_to_empty() {
        let db = fresh_db().await;
        let cache = EmbedCache::new();
        let ids = seed(&db.pool, "c1", &["a"]).await;
        upsert_embeddings(&db.pool, 1, &[(ids[0], embedding_to_bytes(&[1.0]))])
            .await
            .unwrap();
        assert_eq!(cache.snapshot(&db.pool).await.unwrap().len(), 1);

        // Переиндексация: каскад сносит вектора, штамп меняется.
        replace_call_passages(
            &db.pool,
            "c1",
            &[PassageInput {
                kind: AssistantPassageKind::Transcript,
                speaker: None,
                start_ms: Some(0),
                end_ms: Some(1000),
                text: "новый".into(),
                token_est: 2,
            }],
        )
        .await
        .unwrap();
        assert_eq!(
            cache.snapshot(&db.pool).await.unwrap().len(),
            0,
            "каскад + смена штампа → пустой снимок"
        );
    }
}
