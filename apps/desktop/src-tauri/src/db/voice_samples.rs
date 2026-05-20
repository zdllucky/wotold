//! M3.6 + #45 (M7.4 follow-up): просмотр и ручное удаление voice_samples.
//!
//! UI на странице контакта показывает накопленные голосовые семплы (количество,
//! качество, source_call). Пользователь может вручную удалить семпл если, например,
//! ошибочно подтвердил спикера и embedding попал в чужой профиль. C3 паспорта.
//!
//! Полная очистка по контакту делается через `delete_contact` (ON DELETE CASCADE).

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSampleView {
    pub id: String,
    pub contact_id: String,
    pub source_call: Option<String>,
    pub quality: Option<f64>,
    pub created_at: String,
    /// Длина embedding-блоба в байтах (для дебага; реальные значения не
    /// раскрываются клиенту — это биометрия, не нужна в UI).
    pub embedding_bytes: i64,
}

pub async fn list_voice_samples(
    pool: &SqlitePool,
    contact_id: &str,
) -> Result<Vec<VoiceSampleView>, AppError> {
    let rows = sqlx::query(
        "SELECT id, contact_id, source_call, quality, created_at, length(embedding) AS embedding_bytes
         FROM voice_samples
         WHERE contact_id = ?1
         ORDER BY created_at DESC",
    )
    .bind(contact_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| VoiceSampleView {
            id: r.get("id"),
            contact_id: r.get("contact_id"),
            source_call: r.get("source_call"),
            quality: r.get("quality"),
            created_at: r.get("created_at"),
            embedding_bytes: r.get("embedding_bytes"),
        })
        .collect())
}

/// Ручное удаление одного семпла (C3 паспорта). Возвращает Err если id не найден.
pub async fn delete_voice_sample(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM voice_samples WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::Other(format!("voice_sample {id} not found")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    async fn seed_contact_and_sample(
        pool: &SqlitePool,
        contact_id: &str,
        sample_id: &str,
        source_call: Option<&str>,
        quality: f64,
    ) {
        let now = "2026-05-20T00:00:00Z";
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES (?1, 'C', 0, '{}', ?2, ?2)",
        )
        .bind(contact_id)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        // [B16] После migration 0003 voice_samples.source_call → FK на calls.id.
        // Тест должен сидить calls-row если source_call задан.
        if let Some(call_id) = source_call {
            sqlx::query(
                "INSERT INTO calls (id, started_at, status, provider, path_label, created_at, updated_at)
                 VALUES (?1, ?2, 'ready', 'soniox', 'managed', ?2, ?2)",
            )
            .bind(call_id)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
        }
        let blob = vec![0u8; 32];
        sqlx::query(
            "INSERT INTO voice_samples (id, contact_id, embedding, source_call, quality, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(sample_id)
        .bind(contact_id)
        .bind(blob)
        .bind(source_call)
        .bind(quality)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_returns_empty_for_unknown_contact() {
        let db = fresh_db().await;
        let v = list_voice_samples(&db.pool, "ghost").await.unwrap();
        assert!(v.is_empty());
    }

    #[tokio::test]
    async fn list_returns_samples_ordered_desc_with_embedding_bytes() {
        let db = fresh_db().await;
        seed_contact_and_sample(&db.pool, "c1", "vs1", Some("call-1"), 0.9).await;
        // Сидим call-2 (FK после 0003).
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, provider, path_label, created_at, updated_at)
             VALUES ('call-2', '2026-05-20T00:00:00Z', 'ready', 'soniox', 'managed', '2026-05-20T00:00:00Z', '2026-05-20T00:00:00Z')",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        // Второй семпл — создадим через прямой INSERT с поздней датой.
        sqlx::query(
            "INSERT INTO voice_samples (id, contact_id, embedding, source_call, quality, created_at)
             VALUES ('vs2', 'c1', ?1, 'call-2', 0.75, '2026-06-01T00:00:00Z')",
        )
        .bind(vec![0u8; 64])
        .execute(&db.pool)
        .await
        .unwrap();

        let v = list_voice_samples(&db.pool, "c1").await.unwrap();
        assert_eq!(v.len(), 2);
        // ORDER BY created_at DESC.
        assert_eq!(v[0].id, "vs2");
        assert_eq!(v[0].embedding_bytes, 64);
        assert_eq!(v[1].id, "vs1");
        assert_eq!(v[1].embedding_bytes, 32);
    }

    #[tokio::test]
    async fn delete_voice_sample_removes_row() {
        let db = fresh_db().await;
        seed_contact_and_sample(&db.pool, "c1", "vs1", None, 0.5).await;
        delete_voice_sample(&db.pool, "vs1").await.unwrap();
        let v = list_voice_samples(&db.pool, "c1").await.unwrap();
        assert!(v.is_empty());
    }

    #[tokio::test]
    async fn delete_voice_sample_unknown_errors() {
        let db = fresh_db().await;
        let err = delete_voice_sample(&db.pool, "ghost").await;
        assert!(err.is_err());
    }
}
