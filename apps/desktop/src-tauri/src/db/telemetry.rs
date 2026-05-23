//! [M14 T-14] Local-only telemetry log для summary generation.
//!
//! Persists 1 row per recap run в `summary_generation_log` (migration 0016).
//! Consumer (UI dashboard) — M14.5, не часть T-14. Сейчас только write-path
//! + read helper для будущих aggregate queries.
//!
//! R7/R8: никаких сетевых отправок. Всё локально.

use sqlx::SqlitePool;

use crate::AppError;

/// One log entry — записывается после `persist_recap_from_json` в recap.rs.
#[derive(Debug, Clone)]
pub struct SummaryLogEntry {
    pub call_id: String,
    pub engine: String,
    pub schema_version: i64,
    pub flag_state: bool,
    pub generation_ms: i64,
}

/// Insert single log row. `created_at` берётся из `CURRENT_TIMESTAMP` (SQL default).
pub async fn record_summary_generation(
    pool: &SqlitePool,
    entry: SummaryLogEntry,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO summary_generation_log
            (call_id, engine, schema_version, flag_state, generation_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&entry.call_id)
    .bind(&entry.engine)
    .bind(entry.schema_version)
    .bind(i64::from(entry.flag_state))
    .bind(entry.generation_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// `(v1_count, v2_count)` — для будущей dashboard UI. M14.5.
#[allow(dead_code)]
pub async fn count_by_schema_version(pool: &SqlitePool) -> Result<(i64, i64), AppError> {
    let row: (i64, i64) = sqlx::query_as(
        "SELECT
            COALESCE(SUM(CASE WHEN schema_version = 1 THEN 1 ELSE 0 END), 0) AS v1,
            COALESCE(SUM(CASE WHEN schema_version = 2 THEN 1 ELSE 0 END), 0) AS v2
         FROM summary_generation_log",
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
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
    async fn record_inserts_row() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        record_summary_generation(
            &db.pool,
            SummaryLogEntry {
                call_id: "c1".into(),
                engine: "cloud-managed".into(),
                schema_version: 2,
                flag_state: true,
                generation_ms: 1234,
            },
        )
        .await
        .unwrap();
        let (v1, v2) = count_by_schema_version(&db.pool).await.unwrap();
        assert_eq!(v1, 0);
        assert_eq!(v2, 1);
    }

    #[tokio::test]
    async fn count_splits_v1_v2() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        for (schema, flag) in [(1_i64, false), (2, true), (2, true), (1, false), (2, true)] {
            record_summary_generation(
                &db.pool,
                SummaryLogEntry {
                    call_id: "c1".into(),
                    engine: "cloud-managed".into(),
                    schema_version: schema,
                    flag_state: flag,
                    generation_ms: 500,
                },
            )
            .await
            .unwrap();
        }
        let (v1, v2) = count_by_schema_version(&db.pool).await.unwrap();
        assert_eq!(v1, 2);
        assert_eq!(v2, 3);
    }

    #[tokio::test]
    async fn cascade_delete_with_call() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        record_summary_generation(
            &db.pool,
            SummaryLogEntry {
                call_id: "c1".into(),
                engine: "cloud-managed".into(),
                schema_version: 2,
                flag_state: true,
                generation_ms: 500,
            },
        )
        .await
        .unwrap();
        sqlx::query("DELETE FROM calls WHERE id = ?1")
            .bind("c1")
            .execute(&db.pool)
            .await
            .unwrap();
        let (v1, v2) = count_by_schema_version(&db.pool).await.unwrap();
        assert_eq!(v1, 0);
        assert_eq!(v2, 0);
    }
}
