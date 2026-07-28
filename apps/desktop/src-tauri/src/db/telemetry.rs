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

// ── Падения чанков (migration 0024) ─────────────────────────────────────────

/// Ключ настройки с активным пресетом. Дублировать `local_engine::preset`
/// нельзя (модуль только под macOS), а разбивка по пресету нужна везде.
const SETTING_ACTIVE_PRESET: &str = "local_engine.active_preset";

/// Обрезка текста ошибки. Причина падения приходит из произвольного места
/// пайплайна (в том числе из stderr сайдкара) — без потолка одна запись может
/// весить мегабайты.
const MAX_REASON_CHARS: usize = 300;

/// Сводка по падениям чанков за окно. Потребитель — dev-диагностика; UI
/// dashboard не входит в задачу (см. `count_by_schema_version`, тот же статус).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkFailureStats {
    /// Всего падений, включая повторные по одному чанку.
    pub failures: i64,
    /// Сколько разных чанков падало хотя бы раз.
    pub distinct_chunks: i64,
    /// Сколько чанков всего создано за окно — знаменатель для «X% упало».
    pub chunks_total: i64,
    /// `(preset, падений)`, по убыванию.
    pub by_preset: Vec<(String, i64)>,
    /// `(reason, падений)`, по убыванию.
    pub by_reason: Vec<(String, i64)>,
}

impl ChunkFailureStats {
    /// Доля упавших чанков в окне, 0.0–100.0. Без созданных чанков — 0.
    pub fn failed_pct(&self) -> f64 {
        if self.chunks_total <= 0 {
            return 0.0;
        }
        (self.distinct_chunks as f64) * 100.0 / (self.chunks_total as f64)
    }
}

/// Записать падение чанка. Вызывается из `mark_chunk_failed` — единственная
/// точка перехода `* → failed`.
///
/// `retry_idx` не передаётся снаружи, а считается из уже записанных падений
/// той же пары: вызывающие о номере попытки не знают, а состояние ретрая живёт
/// в `call_chunks.status` и после успеха стирается.
pub async fn record_chunk_failure(
    pool: &SqlitePool,
    call_id: &str,
    chunk_idx: u32,
    reason: &str,
) -> Result<(), AppError> {
    let preset = super::get_setting(pool, SETTING_ACTIVE_PRESET)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    let reason: String = reason.chars().take(MAX_REASON_CHARS).collect();
    sqlx::query(
        "INSERT INTO chunk_failure_log (call_id, chunk_idx, reason, retry_idx, preset)
         VALUES (?1, ?2, ?3,
                 (SELECT COUNT(*) FROM chunk_failure_log
                  WHERE call_id = ?1 AND chunk_idx = ?2),
                 ?4)",
    )
    .bind(call_id)
    .bind(chunk_idx)
    .bind(&reason)
    .bind(&preset)
    .execute(pool)
    .await?;
    Ok(())
}

/// Сводка за последние `days` суток.
pub async fn chunk_failure_stats(
    pool: &SqlitePool,
    days: i64,
) -> Result<ChunkFailureStats, AppError> {
    let since = format!("-{days} days");
    let (failures, distinct_chunks): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(DISTINCT call_id || ':' || chunk_idx)
         FROM chunk_failure_log
         WHERE created_at >= datetime('now', ?1)",
    )
    .bind(&since)
    .fetch_one(pool)
    .await?;
    let (chunks_total,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM call_chunks WHERE created_at >= datetime('now', ?1)")
            .bind(&since)
            .fetch_one(pool)
            .await?;
    let by_preset: Vec<(String, i64)> = sqlx::query_as(
        "SELECT preset, COUNT(*) AS n FROM chunk_failure_log
         WHERE created_at >= datetime('now', ?1)
         GROUP BY preset ORDER BY n DESC, preset ASC",
    )
    .bind(&since)
    .fetch_all(pool)
    .await?;
    let by_reason: Vec<(String, i64)> = sqlx::query_as(
        "SELECT reason, COUNT(*) AS n FROM chunk_failure_log
         WHERE created_at >= datetime('now', ?1)
         GROUP BY reason ORDER BY n DESC, reason ASC",
    )
    .bind(&since)
    .fetch_all(pool)
    .await?;
    Ok(ChunkFailureStats {
        failures,
        distinct_chunks,
        chunks_total,
        by_preset,
        by_reason,
    })
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

    async fn insert_chunk(pool: &SqlitePool, call_id: &str, idx: i64) {
        sqlx::query(
            "INSERT INTO call_chunks (call_id, chunk_idx, start_ms, mic_path, system_path)
             VALUES (?1, ?2, 0, 'mic.wav', 'sys.wav')",
        )
        .bind(call_id)
        .bind(idx)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn retry_idx_counts_repeats_of_the_same_chunk() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        for _ in 0..3 {
            record_chunk_failure(&db.pool, "c1", 0, "stt timeout")
                .await
                .unwrap();
        }
        // Другой чанк того же звонка считается отдельно.
        record_chunk_failure(&db.pool, "c1", 1, "stt timeout")
            .await
            .unwrap();
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT chunk_idx, retry_idx FROM chunk_failure_log ORDER BY id ASC")
                .fetch_all(&db.pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![(0, 0), (0, 1), (0, 2), (1, 0)]);
    }

    #[tokio::test]
    async fn preset_is_frozen_at_failure_time() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        // Настройки ещё нет — падение всё равно пишется.
        record_chunk_failure(&db.pool, "c1", 0, "no models")
            .await
            .unwrap();
        crate::db::set_setting(&db.pool, "local_engine.active_preset", "quality")
            .await
            .unwrap();
        record_chunk_failure(&db.pool, "c1", 1, "no models")
            .await
            .unwrap();
        // Смена пресета задним числом первую запись не переписывает.
        crate::db::set_setting(&db.pool, "local_engine.active_preset", "light")
            .await
            .unwrap();
        let presets: Vec<(String,)> =
            sqlx::query_as("SELECT preset FROM chunk_failure_log ORDER BY id ASC")
                .fetch_all(&db.pool)
                .await
                .unwrap();
        assert_eq!(
            presets.into_iter().map(|r| r.0).collect::<Vec<_>>(),
            vec!["unknown", "quality"]
        );
    }

    #[tokio::test]
    async fn reason_is_truncated() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        record_chunk_failure(&db.pool, "c1", 0, &"я".repeat(1000))
            .await
            .unwrap();
        let (reason,): (String,) = sqlx::query_as("SELECT reason FROM chunk_failure_log")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        // Считаем символы, а не байты: обрезка по байтам порвала бы UTF-8.
        assert_eq!(reason.chars().count(), MAX_REASON_CHARS);
    }

    #[tokio::test]
    async fn stats_split_repeats_from_distinct_chunks() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        for idx in 0..4 {
            insert_chunk(&db.pool, "c1", idx).await;
        }
        crate::db::set_setting(&db.pool, "local_engine.active_preset", "balanced")
            .await
            .unwrap();
        record_chunk_failure(&db.pool, "c1", 0, "stt timeout")
            .await
            .unwrap();
        record_chunk_failure(&db.pool, "c1", 0, "stt timeout")
            .await
            .unwrap();
        record_chunk_failure(&db.pool, "c1", 1, "wav truncated")
            .await
            .unwrap();

        let s = chunk_failure_stats(&db.pool, 7).await.unwrap();
        assert_eq!(s.failures, 3, "повторы считаются");
        assert_eq!(s.distinct_chunks, 2, "но чанков упало два");
        assert_eq!(s.chunks_total, 4);
        assert_eq!(s.failed_pct(), 50.0);
        assert_eq!(s.by_preset, vec![("balanced".to_string(), 3)]);
        assert_eq!(
            s.by_reason,
            vec![
                ("stt timeout".to_string(), 2),
                ("wav truncated".to_string(), 1)
            ]
        );
    }

    #[tokio::test]
    async fn stats_are_zero_without_data_and_pct_survives_empty_window() {
        let db = fresh_db().await;
        let s = chunk_failure_stats(&db.pool, 7).await.unwrap();
        assert_eq!(s.failures, 0);
        assert_eq!(s.chunks_total, 0);
        // Деления на ноль быть не должно.
        assert_eq!(s.failed_pct(), 0.0);
    }

    #[tokio::test]
    async fn failure_log_dies_with_the_call() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1").await;
        record_chunk_failure(&db.pool, "c1", 0, "stt timeout")
            .await
            .unwrap();
        sqlx::query("DELETE FROM calls WHERE id = ?1")
            .bind("c1")
            .execute(&db.pool)
            .await
            .unwrap();
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chunk_failure_log")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}
