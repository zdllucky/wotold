//! [M14 T-02] DB helpers для таблицы `decisions` (migration 0015).
//!
//! Replace-all semantics — recap перегенерируется целиком, не diff'ом.
//! Симметрично [`crate::db::action_items`].

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::AppError;

/// Input при persist'е (после parsing cloud response).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecisionInput {
    pub text: String,
    #[serde(default)]
    pub evidence_quote: Option<String>,
    #[serde(default)]
    pub evidence_speaker: Option<String>,
    #[serde(default)]
    pub evidence_start_ms: Option<i64>,
    #[serde(default)]
    pub evidence_end_ms: Option<i64>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

/// Read shape для UI / API queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRow {
    pub id: String,
    pub call_id: String,
    pub text: String,
    pub evidence_quote: Option<String>,
    pub evidence_speaker: Option<String>,
    pub evidence_start_ms: Option<i64>,
    pub evidence_end_ms: Option<i64>,
    pub confidence: Option<f64>,
    pub order_idx: i64,
}

/// Заменить все decisions для звонка (DELETE + INSERT в транзакции).
pub async fn replace_decisions(
    pool: &SqlitePool,
    call_id: &str,
    items: &[DecisionInput],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM decisions WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    for (idx, item) in items.iter().enumerate() {
        let text = item.text.trim();
        if text.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO decisions (
                id, call_id, text, evidence_quote, evidence_speaker,
                evidence_start_ms, evidence_end_ms, confidence, order_idx
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(call_id)
        .bind(text)
        .bind(item.evidence_quote.as_deref())
        .bind(item.evidence_speaker.as_deref())
        .bind(item.evidence_start_ms)
        .bind(item.evidence_end_ms)
        .bind(item.confidence)
        .bind(idx as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Список decisions для звонка, ordered by `order_idx ASC`.
#[allow(dead_code)] // [M14 T-02] Production callers (Tauri command, UI) — в T-11.
pub async fn list_decisions(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Vec<DecisionRow>, AppError> {
    type Row = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<f64>,
        i64,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, call_id, text, evidence_quote, evidence_speaker,
                evidence_start_ms, evidence_end_ms, confidence, order_idx
         FROM decisions
         WHERE call_id = ?1
         ORDER BY order_idx ASC",
    )
    .bind(call_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| DecisionRow {
            id: r.0,
            call_id: r.1,
            text: r.2,
            evidence_quote: r.3,
            evidence_speaker: r.4,
            evidence_start_ms: r.5,
            evidence_end_ms: r.6,
            confidence: r.7,
            order_idx: r.8,
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
    async fn replace_decisions_empty_clears_table() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        // Pre-seed one row.
        replace_decisions(
            &db.pool,
            "c1",
            &[DecisionInput {
                text: "lock pricing".into(),
                ..Default::default()
            }],
        )
        .await
        .unwrap();
        // Replace с empty — должно clear.
        replace_decisions(&db.pool, "c1", &[]).await.unwrap();
        let rows = list_decisions(&db.pool, "c1").await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn replace_decisions_inserts_with_order_idx_evidence() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        let items = vec![
            DecisionInput {
                text: "Lock enterprise tier at $499".into(),
                evidence_quote: Some("we agreed on 499 dollars".into()),
                evidence_speaker: Some("Alice".into()),
                evidence_start_ms: Some(1500),
                evidence_end_ms: Some(3500),
                confidence: Some(0.92),
            },
            DecisionInput {
                text: "Launch beta next week".into(),
                ..Default::default()
            },
        ];
        replace_decisions(&db.pool, "c1", &items).await.unwrap();
        let rows = list_decisions(&db.pool, "c1").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].order_idx, 0);
        assert_eq!(rows[0].text, "Lock enterprise tier at $499");
        assert_eq!(
            rows[0].evidence_quote.as_deref(),
            Some("we agreed on 499 dollars")
        );
        assert_eq!(rows[0].confidence, Some(0.92));
        assert_eq!(rows[1].order_idx, 1);
        assert!(rows[1].evidence_quote.is_none());
    }

    #[tokio::test]
    async fn replace_decisions_skips_empty_text() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        let items = vec![
            DecisionInput {
                text: "valid one".into(),
                ..Default::default()
            },
            DecisionInput {
                text: "   ".into(),
                ..Default::default()
            },
        ];
        replace_decisions(&db.pool, "c1", &items).await.unwrap();
        let rows = list_decisions(&db.pool, "c1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "valid one");
    }

    #[tokio::test]
    async fn cascade_delete_when_call_removed() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        replace_decisions(
            &db.pool,
            "c1",
            &[DecisionInput {
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
        let rows = list_decisions(&db.pool, "c1").await.unwrap();
        assert!(rows.is_empty());
    }
}
