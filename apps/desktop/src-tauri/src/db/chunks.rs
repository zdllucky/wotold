//! [M13.1.3b] DB helpers для таблицы `call_chunks` (migrations/0013).
//!
//! Status FSM: `pending → processing → done|failed`. Каждый
//! `mark_*` enforce'ит legal transition через `WHERE status = ?` clause.
//! Если транзишн нелегален — rows_affected == 0, возвращаем `Other` ошибку
//! (caller обычно интерпретирует как «уже processed, skip»).
//!
//! Style: mirror `db::calls::lifecycle` — thin SQL functions, без
//! бизнес-логики. Бизнес-логика живёт в [`crate::pipeline::chunk_runner`].

use std::path::Path;

use sqlx::{Row, SqlitePool};

use crate::AppError;

/// Snapshot записи в `call_chunks` для read queries (list/get).
#[derive(Debug, Clone)]
pub struct ChunkRow {
    pub call_id: String,
    pub chunk_idx: u32,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    /// Read только tests'ами / UI inspectors (Phase 3). Production assembly
    /// читает transcript_json + system_transcript_json напрямую.
    #[allow(dead_code)]
    pub mic_path: String,
    #[allow(dead_code)]
    pub system_path: String,
    pub status: String,
    /// [M13.1.5d] Сериализованный `DiarizedTranscript` mic-дорожки.
    pub transcript_json: Option<String>,
    /// [M13.1.5d] Сериализованный `DiarizedTranscript` system-дорожки.
    /// `None` для legacy chunks (M13.1.5c, mic-only) или когда system STT
    /// деградировал — assembly обрабатывает это как пустой system track.
    pub system_transcript_json: Option<String>,
    /// Phase 2 (M13.2.2) per-chunk WeSpeaker embeddings для cross-chunk
    /// speaker re-clustering. Сейчас всегда None — заполняется в Phase 2.
    #[allow(dead_code)]
    pub embeddings_json: Option<String>,
}

/// Создать chunk row со статусом `pending`. Идемпотентно через
/// `INSERT OR IGNORE` — если (call_id, chunk_idx) уже есть, no-op
/// (могла остаться partial-запись после crash, не блокируем retry).
pub async fn insert_chunk(
    pool: &SqlitePool,
    call_id: &str,
    chunk_idx: u32,
    start_ms: u64,
    mic_path: &Path,
    system_path: &Path,
) -> Result<(), AppError> {
    let mic = mic_path.to_string_lossy();
    let system = system_path.to_string_lossy();
    sqlx::query(
        "INSERT OR IGNORE INTO call_chunks
            (call_id, chunk_idx, start_ms, mic_path, system_path, status,
             created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending',
             CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(call_id)
    .bind(chunk_idx)
    .bind(start_ms as i64)
    .bind(mic.as_ref())
    .bind(system.as_ref())
    .execute(pool)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

/// [Tech-debt P0.2] Транзишн `failed → pending`. Используется
/// `commands::recording::retry_chunk` чтобы переcпавнить chunk_runner
/// после fail'а. FSM gate `failed → pending` only — protect от race с
/// running chunk (processing → pending запрещён, иначе chunk_runner
/// finish может смешаться с retry-spawn'ом).
pub async fn mark_chunk_pending(
    pool: &SqlitePool,
    call_id: &str,
    chunk_idx: u32,
) -> Result<(), AppError> {
    let res = sqlx::query(
        "UPDATE call_chunks
         SET status = 'pending', updated_at = CURRENT_TIMESTAMP
         WHERE call_id = ?1 AND chunk_idx = ?2 AND status = 'failed'",
    )
    .bind(call_id)
    .bind(chunk_idx)
    .execute(pool)
    .await
    .map_err(AppError::from)?;
    if res.rows_affected() == 0 {
        return Err(AppError::Other(format!(
            "mark_chunk_pending: chunk {call_id}/{chunk_idx} not in 'failed' status"
        )));
    }
    Ok(())
}

/// Транзишн `pending → processing`. Используется chunk_runner перед
/// reached'ом STT. Если status был не pending — Err (caller skip'ает).
pub async fn mark_chunk_processing(
    pool: &SqlitePool,
    call_id: &str,
    chunk_idx: u32,
) -> Result<(), AppError> {
    let res = sqlx::query(
        "UPDATE call_chunks
         SET status = 'processing', updated_at = CURRENT_TIMESTAMP
         WHERE call_id = ?1 AND chunk_idx = ?2 AND status = 'pending'",
    )
    .bind(call_id)
    .bind(chunk_idx)
    .execute(pool)
    .await
    .map_err(AppError::from)?;
    if res.rows_affected() == 0 {
        return Err(AppError::Other(format!(
            "mark_chunk_processing: chunk {call_id}/{chunk_idx} not in 'pending' status"
        )));
    }
    Ok(())
}

/// Транзишн `processing → done` + sets end_ms + transcript_json (+ optional
/// system_transcript_json + optional embeddings_json). После этого
/// chunk_runner может извлечь tail-prompt для следующего чанка.
///
/// [M13.1.5d] `system_transcript_json` опционален: при degraded-ok сценарии
/// (mic transcribed, system failed) сохраняем mic + NULL system. Assembly
/// помечает такие chunks как «mic-only».
///
/// [M13.2.1] `embeddings_json` — сериализованный `HashMap<String, Vec<f32>>`
/// per-speaker_tag cluster embeddings (mean-pooled WeSpeaker, L2-normalized).
/// `None` для legacy pre-Phase 2 chunks; `Some("{}")` — Phase 2 chunk без
/// real embeddings (voice-onnx feature off / model not downloaded). Phase 2
/// assembly использует non-None embeddings для cross-chunk speaker
/// re-clustering.
pub async fn mark_chunk_done(
    pool: &SqlitePool,
    call_id: &str,
    chunk_idx: u32,
    end_ms: u64,
    transcript_json: &str,
    system_transcript_json: Option<&str>,
    embeddings_json: Option<&str>,
) -> Result<(), AppError> {
    let res = sqlx::query(
        "UPDATE call_chunks
         SET status = 'done',
             end_ms = ?3,
             transcript_json = ?4,
             system_transcript_json = ?5,
             embeddings_json = ?6,
             updated_at = CURRENT_TIMESTAMP
         WHERE call_id = ?1 AND chunk_idx = ?2 AND status = 'processing'",
    )
    .bind(call_id)
    .bind(chunk_idx)
    .bind(end_ms as i64)
    .bind(transcript_json)
    .bind(system_transcript_json)
    .bind(embeddings_json)
    .execute(pool)
    .await
    .map_err(AppError::from)?;
    if res.rows_affected() == 0 {
        return Err(AppError::Other(format!(
            "mark_chunk_done: chunk {call_id}/{chunk_idx} not in 'processing' status"
        )));
    }
    Ok(())
}

/// Транзишн `* → failed`. Принимаем из любого статуса — обычно из processing,
/// но из pending тоже валидно (например chunk файл повреждён до STT).
/// `reason` логируется (не persistим — failed обычно retry'ится).
pub async fn mark_chunk_failed(
    pool: &SqlitePool,
    call_id: &str,
    chunk_idx: u32,
    reason: &str,
) -> Result<(), AppError> {
    log::warn!("chunk {call_id}/{chunk_idx} failed: {reason}");
    sqlx::query(
        "UPDATE call_chunks
         SET status = 'failed', updated_at = CURRENT_TIMESTAMP
         WHERE call_id = ?1 AND chunk_idx = ?2",
    )
    .bind(call_id)
    .bind(chunk_idx)
    .execute(pool)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

/// Список chunks для звонка, отсортированных по chunk_idx asc. Используется
/// в global re-clustering (Phase 2) + UI progress strip.
pub async fn list_chunks_by_call(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Vec<ChunkRow>, AppError> {
    let rows = sqlx::query(
        "SELECT call_id, chunk_idx, start_ms, end_ms, mic_path, system_path,
                status, transcript_json, system_transcript_json, embeddings_json
         FROM call_chunks
         WHERE call_id = ?1
         ORDER BY chunk_idx ASC",
    )
    .bind(call_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)?;
    Ok(rows
        .into_iter()
        .map(|r| ChunkRow {
            call_id: r.get::<String, _>("call_id"),
            chunk_idx: r.get::<i64, _>("chunk_idx") as u32,
            start_ms: r.get::<i64, _>("start_ms"),
            end_ms: r.get::<Option<i64>, _>("end_ms"),
            mic_path: r.get::<String, _>("mic_path"),
            system_path: r.get::<String, _>("system_path"),
            status: r.get::<String, _>("status"),
            transcript_json: r.get::<Option<String>, _>("transcript_json"),
            system_transcript_json: r.get::<Option<String>, _>("system_transcript_json"),
            embeddings_json: r.get::<Option<String>, _>("embeddings_json"),
        })
        .collect())
}

/// [P-fix4] Удалить все chunk-строки звонка — для полной переобработки
/// «Переобработать целиком» (всегда заново из аудио, включая STT).
///
/// Удаление (а не reset→pending) важно: после него у звонка **0 chunks** →
/// `ensure_all_chunks_done` проходит (Ok на пустом наборе) и
/// `load_chunked_transcripts` возвращает None → pipeline идёт на **full-file
/// STT** по полному root `mic.wav`/`system.wav`. Reset→pending халтил бы
/// pipeline (`ensure_all_chunks_done` возвращает Err на pending).
///
/// Chunk-аудио на диске (`chunks/{idx}/*.wav`) НЕ трогается — root WAV уже
/// склеен на всю длительность. Возвращает количество удалённых rows
/// (0 для cloud / non-chunked — no-op).
pub async fn delete_chunks_for_call(pool: &SqlitePool, call_id: &str) -> Result<u64, AppError> {
    let res = sqlx::query("DELETE FROM call_chunks WHERE call_id = ?1")
        .bind(call_id)
        .execute(pool)
        .await
        .map_err(AppError::from)?;
    Ok(res.rows_affected())
}

/// [P11.1] Все chunks для звонка имеют status='done'. `false` если хотя бы
/// один pending/processing/failed, либо если у звонка вообще нет chunks
/// (cloud-managed / non-chunked path → caller должен использовать другую
/// логику, не auto-resume).
pub async fn all_chunks_done(pool: &SqlitePool, call_id: &str) -> Result<bool, AppError> {
    let row: (i64, i64) = sqlx::query_as(
        "SELECT
           COUNT(*) AS total,
           COUNT(CASE WHEN status = 'done' THEN 1 END) AS done
         FROM call_chunks
         WHERE call_id = ?1",
    )
    .bind(call_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::from)?;
    let (total, done) = row;
    Ok(total > 0 && total == done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use std::path::PathBuf;

    async fn insert_dummy_call(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 'recording', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn insert_then_read_back() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/abs/c1/chunks/0/mic.wav"),
            &PathBuf::from("/abs/c1/chunks/0/system.wav"),
        )
        .await
        .unwrap();
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].chunk_idx, 0);
        assert_eq!(rows[0].status, "pending");
        assert_eq!(rows[0].mic_path, "/abs/c1/chunks/0/mic.wav");
        assert!(rows[0].end_ms.is_none());
    }

    #[tokio::test]
    async fn all_chunks_done_returns_true_only_when_all_done() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;

        // No chunks → false.
        assert!(!all_chunks_done(&test_db.pool, "c1").await.unwrap());

        // 1 chunk pending → false.
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m0"),
            &PathBuf::from("/s0"),
        )
        .await
        .unwrap();
        assert!(!all_chunks_done(&test_db.pool, "c1").await.unwrap());

        // Mark chunk 0 done — single chunk, all done → true.
        mark_chunk_processing(&test_db.pool, "c1", 0).await.unwrap();
        mark_chunk_done(
            &test_db.pool,
            "c1",
            0,
            600_000,
            r#"{"segments":[]}"#,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(all_chunks_done(&test_db.pool, "c1").await.unwrap());

        // Add chunk 1 pending → false again.
        insert_chunk(
            &test_db.pool,
            "c1",
            1,
            600_000,
            &PathBuf::from("/m1"),
            &PathBuf::from("/s1"),
        )
        .await
        .unwrap();
        assert!(!all_chunks_done(&test_db.pool, "c1").await.unwrap());
    }

    #[tokio::test]
    async fn mark_processing_changes_status() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        mark_chunk_processing(&test_db.pool, "c1", 0).await.unwrap();
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(rows[0].status, "processing");
    }

    #[tokio::test]
    async fn mark_done_sets_end_ms_and_transcript() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        mark_chunk_processing(&test_db.pool, "c1", 0).await.unwrap();
        mark_chunk_done(
            &test_db.pool,
            "c1",
            0,
            600_000,
            r#"{"segments":[]}"#,
            None,
            None,
        )
        .await
        .unwrap();
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(rows[0].status, "done");
        assert_eq!(rows[0].end_ms, Some(600_000));
        assert_eq!(
            rows[0].transcript_json.as_deref(),
            Some(r#"{"segments":[]}"#)
        );
        assert!(rows[0].system_transcript_json.is_none());
        assert!(rows[0].embeddings_json.is_none());
    }

    #[tokio::test]
    async fn mark_done_persists_both_tracks() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        mark_chunk_processing(&test_db.pool, "c1", 0).await.unwrap();
        mark_chunk_done(
            &test_db.pool,
            "c1",
            0,
            600_000,
            r#"{"mic":1}"#,
            Some(r#"{"sys":1}"#),
            None,
        )
        .await
        .unwrap();
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(rows[0].transcript_json.as_deref(), Some(r#"{"mic":1}"#));
        assert_eq!(
            rows[0].system_transcript_json.as_deref(),
            Some(r#"{"sys":1}"#)
        );
    }

    #[tokio::test]
    async fn mark_done_persists_embeddings_json() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        mark_chunk_processing(&test_db.pool, "c1", 0).await.unwrap();
        let emb = r#"{"speaker:0":[0.1,0.2,0.3]}"#;
        mark_chunk_done(
            &test_db.pool,
            "c1",
            0,
            600_000,
            r#"{"mic":1}"#,
            None,
            Some(emb),
        )
        .await
        .unwrap();
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(rows[0].embeddings_json.as_deref(), Some(emb));
    }

    #[tokio::test]
    async fn mark_done_rejects_non_processing_status() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        // Skip processing — try done directly. Должно fail'нуться.
        let res = mark_chunk_done(&test_db.pool, "c1", 0, 600_000, "{}", None, None).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn mark_failed_accepts_any_status() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        // Failed from pending.
        mark_chunk_failed(&test_db.pool, "c1", 0, "test reason")
            .await
            .unwrap();
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(rows[0].status, "failed");
    }

    // [Tech-debt P0.2] mark_chunk_pending — FSM gate failed → pending only.
    #[tokio::test]
    async fn mark_pending_from_failed_ok() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        mark_chunk_failed(&test_db.pool, "c1", 0, "test reason")
            .await
            .unwrap();
        // failed → pending OK.
        mark_chunk_pending(&test_db.pool, "c1", 0).await.unwrap();
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(rows[0].status, "pending");
    }

    #[tokio::test]
    async fn mark_pending_from_pending_errors() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        // pending → pending запрещён (FSM gate).
        let err = mark_chunk_pending(&test_db.pool, "c1", 0)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not in 'failed' status"),
            "expected FSM err, got: {err}"
        );
    }

    #[tokio::test]
    async fn mark_pending_from_processing_errors() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        mark_chunk_processing(&test_db.pool, "c1", 0).await.unwrap();
        // processing → pending запрещён (защита от race).
        let err = mark_chunk_pending(&test_db.pool, "c1", 0)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not in 'failed' status"));
    }

    // [P-fix4] delete_chunks_for_call — полное удаление chunk-строк для re-STT.
    #[tokio::test]
    async fn delete_chunks_for_call_removes_all_rows() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_dummy_call(&test_db.pool, "c2").await;
        for idx in [0u32, 1, 2] {
            insert_chunk(
                &test_db.pool,
                "c1",
                idx,
                u64::from(idx) * 600_000,
                &PathBuf::from(format!("/m{idx}")),
                &PathBuf::from(format!("/s{idx}")),
            )
            .await
            .unwrap();
            mark_chunk_processing(&test_db.pool, "c1", idx)
                .await
                .unwrap();
        }
        mark_chunk_done(
            &test_db.pool,
            "c1",
            0,
            600_000,
            r#"{"mic":1}"#,
            Some(r#"{"sys":1}"#),
            Some(r#"{"speaker:0":[0.1]}"#),
        )
        .await
        .unwrap();
        // Чужой звонок c2 — чтобы убедиться что delete не задевает его.
        insert_chunk(
            &test_db.pool,
            "c2",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();

        let n = delete_chunks_for_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(n, 3);
        // 0 chunks → full-file STT path.
        assert!(list_chunks_by_call(&test_db.pool, "c1")
            .await
            .unwrap()
            .is_empty());
        // Чужой звонок не тронут.
        assert_eq!(
            list_chunks_by_call(&test_db.pool, "c2").await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn delete_chunks_for_call_noop_when_no_chunks() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        let n = delete_chunks_for_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn list_chunks_sorted_by_idx() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        // Insert в обратном порядке — list должен вернуть отсортированно.
        for idx in [2, 0, 1] {
            insert_chunk(
                &test_db.pool,
                "c1",
                idx,
                idx as u64 * 600_000,
                &PathBuf::from(format!("/m{idx}")),
                &PathBuf::from(format!("/s{idx}")),
            )
            .await
            .unwrap();
        }
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].chunk_idx, 0);
        assert_eq!(rows[1].chunk_idx, 1);
        assert_eq!(rows[2].chunk_idx, 2);
    }

    #[tokio::test]
    async fn cascade_delete_when_call_removed() {
        let test_db = fresh_db().await;
        insert_dummy_call(&test_db.pool, "c1").await;
        insert_chunk(
            &test_db.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        // Удалить родительский call — chunks должны исчезнуть (FK ON DELETE CASCADE).
        sqlx::query("DELETE FROM calls WHERE id = ?1")
            .bind("c1")
            .execute(&test_db.pool)
            .await
            .unwrap();
        let rows = list_chunks_by_call(&test_db.pool, "c1").await.unwrap();
        assert!(rows.is_empty());
    }
}
