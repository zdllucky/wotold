//! [M14 T-02] DB helpers для таблицы `open_questions` (migration 0015).
//!
//! Mirror [`crate::db::decisions`] — replace-all semantics, FK CASCADE
//! ON DELETE calls.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::AppError;

/// Input при persist'е.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenQuestionInput {
    pub text: String,
    #[serde(default)]
    pub raised_by: Option<String>,
    #[serde(default)]
    pub evidence_quote: Option<String>,
    #[serde(default)]
    pub evidence_speaker: Option<String>,
    #[serde(default)]
    pub evidence_start_ms: Option<i64>,
}

/// Read shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenQuestionRow {
    pub id: String,
    pub call_id: String,
    pub text: String,
    pub raised_by: Option<String>,
    pub evidence_quote: Option<String>,
    pub evidence_speaker: Option<String>,
    pub evidence_start_ms: Option<i64>,
    pub order_idx: i64,
}

/// Заменить все open_questions для звонка (DELETE + INSERT в транзакции).
pub async fn replace_open_questions(
    pool: &SqlitePool,
    call_id: &str,
    items: &[OpenQuestionInput],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM open_questions WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    for (idx, item) in items.iter().enumerate() {
        let text = item.text.trim();
        if text.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO open_questions (
                id, call_id, text, raised_by, evidence_quote,
                evidence_speaker, evidence_start_ms, order_idx
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(call_id)
        .bind(text)
        .bind(item.raised_by.as_deref())
        .bind(item.evidence_quote.as_deref())
        .bind(item.evidence_speaker.as_deref())
        .bind(item.evidence_start_ms)
        .bind(idx as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Список open_questions для звонка, ordered by `order_idx ASC`.
#[allow(dead_code)] // [M14 T-02] Production callers (Tauri command, UI) — в T-11.
pub async fn list_open_questions(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Vec<OpenQuestionRow>, AppError> {
    type Row = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        i64,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, call_id, text, raised_by, evidence_quote,
                evidence_speaker, evidence_start_ms, order_idx
         FROM open_questions
         WHERE call_id = ?1
         ORDER BY order_idx ASC",
    )
    .bind(call_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| OpenQuestionRow {
            id: r.0,
            call_id: r.1,
            text: r.2,
            raised_by: r.3,
            evidence_quote: r.4,
            evidence_speaker: r.5,
            evidence_start_ms: r.6,
            order_idx: r.7,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    async fn insert_dummy_call(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn replace_open_questions_inserts_with_raised_by_and_evidence() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        let items = vec![
            OpenQuestionInput {
                text: "Should we offer a trial?".into(),
                raised_by: Some("Bob".into()),
                evidence_quote: Some("we should think about trial period".into()),
                evidence_speaker: Some("speaker:0".into()),
                evidence_start_ms: Some(5000),
            },
            OpenQuestionInput {
                text: "What about enterprise SSO?".into(),
                ..Default::default()
            },
        ];
        replace_open_questions(&db.pool, "c1", &items)
            .await
            .unwrap();
        let rows = list_open_questions(&db.pool, "c1").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "Should we offer a trial?");
        assert_eq!(rows[0].raised_by.as_deref(), Some("Bob"));
        assert_eq!(rows[1].order_idx, 1);
    }

    #[tokio::test]
    async fn replace_open_questions_clears_on_empty() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        replace_open_questions(
            &db.pool,
            "c1",
            &[OpenQuestionInput {
                text: "x".into(),
                ..Default::default()
            }],
        )
        .await
        .unwrap();
        replace_open_questions(&db.pool, "c1", &[]).await.unwrap();
        let rows = list_open_questions(&db.pool, "c1").await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn cascade_delete_when_call_removed() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        replace_open_questions(
            &db.pool,
            "c1",
            &[OpenQuestionInput {
                text: "x".into(),
                ..Default::default()
            }],
        )
        .await
        .unwrap();
        sqlx::query("DELETE FROM calls WHERE id = ?1")
            .bind("c1")
            .execute(&db.pool)
            .await
            .unwrap();
        let rows = list_open_questions(&db.pool, "c1").await.unwrap();
        assert!(rows.is_empty());
    }
}
