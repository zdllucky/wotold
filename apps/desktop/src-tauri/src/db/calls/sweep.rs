//! [B19.6 / B28.2] Уборка после краша: строки, застрявшие в `recording` и
//! `processing`, и кандидаты авто-восстановления.
//!
//! [TD-41] Выделено из `calls/lifecycle.rs` (1423 строки при лимите 800,
//! правило 8) вместе с тестами. Вызывается на старте приложения
//! (`state::init` → sweep + `reconcile_orphan_recordings`). Логика не
//! менялась.

use sqlx::SqlitePool;

use crate::AppError;

/// Stale-sweep: при старте приложения все `recording` и `processing` row'ы
/// помечаются `failed`. Это означает что в прошлой сессии запись или
/// пайплайн были прерваны (краш, force-quit, потеря питания). Возвращает
/// количество затронутых строк — пригодится для лога.
///
/// [M13 review fix] Также sweep'аем `call_chunks` в `processing` — без этого
/// crash во время chunk_runner оставлял бы row застрявшим, и
/// `chunk_assembly::load_chunked_transcripts` молча skip'ал бы его (filter
/// status='done'), что приводило бы к silent data loss для chunk'а в
/// финальном transcript'е.
/// [B28.2] Кандидаты авто-восстановления: failed БЕЗ failed_reason — так
/// помечают только sweep_stale_calls / reconcile_orphan_recordings (прерывание
/// крашем/quit), настоящие фейлы пайплайна идут через mark_call_failed с
/// reason. Дальнейшие гейты (аудио есть, транскрипта нет, лимит попыток) —
/// на стороне caller'а по файловой системе.
pub async fn list_interrupted_failed_calls(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM calls
         WHERE status = 'failed' AND failed_reason IS NULL
         ORDER BY started_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

pub async fn sweep_stale_calls(pool: &SqlitePool) -> Result<u64, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    // [B19.6] Только 'processing' (у них есть финализированное аудио → recoverable
    // как 'failed'). Орфан-'recording' (краш во время записи) обрабатываются
    // отдельно в `reconcile_orphan_recordings` — там по длине частичного WAV
    // решается удалить (<30с) или пометить failed (≥30с).
    let res = sqlx::query(
        "UPDATE calls
         SET status = 'failed',
             ended_at = COALESCE(ended_at, ?1),
             updated_at = ?1
         WHERE status = 'processing'",
    )
    .bind(&now)
    .execute(pool)
    .await?;
    // Sweep stuck chunks (idempotent — no-op если ничего не застряло).
    let chunks_swept = sqlx::query(
        "UPDATE call_chunks
         SET status = 'failed', updated_at = CURRENT_TIMESTAMP
         WHERE status = 'processing'",
    )
    .execute(pool)
    .await?
    .rows_affected();
    if chunks_swept > 0 {
        log::warn!("sweep_stale_chunks: {chunks_swept} processing chunks → failed");
    }
    // [Tech-debt P0.3] Pending chunks для уже-неактивных calls — тоже
    // orphaned (chunk_runner не успел mark_processing до crash, или sidecar
    // создал row но не вызвал runner). Помечаем failed, чтобы UI показал
    // retry button (`retry_chunk` FSM gate failed→pending).
    //
    // Active recordings (status='recording') исключаем: live sidecar может
    // legitимно держать pending row до первой ротации. На практике sweep
    // запускается ДО старта новых recordings (state::init line 72), плюс
    // выше уже recording→failed UPDATE — значит pending row'ы остаются
    // только у уже-stale calls.
    let pending_swept = sqlx::query(
        "UPDATE call_chunks
         SET status = 'failed', updated_at = CURRENT_TIMESTAMP
         WHERE status = 'pending'
           AND call_id IN (
             SELECT id FROM calls WHERE status NOT IN ('recording')
           )",
    )
    .execute(pool)
    .await?
    .rows_affected();
    if pending_swept > 0 {
        log::warn!(
            "sweep_stale_chunks: {pending_swept} pending chunks of non-active calls → failed"
        );
    }
    Ok(res.rows_affected())
}

/// [B19.6] id'шники всех строк, застрявших в status='recording' (краш/force-quit
/// во время записи). `reconcile_orphan_recordings` решает их судьбу по длине WAV.
pub async fn list_orphan_recording_ids(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM calls WHERE status = 'recording'")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

#[cfg(test)]
mod tests {
    // Соседние функции звонка приходят через фасад `db::calls`:
    // тесты домена всё равно строят строку через `insert_recording`.
    use crate::db::calls::*;
    use crate::db::test_support::fresh_db;

    // [Tech-debt P0.3] sweep_stale_chunks дополнительно покрывает pending
    // chunks для неактивных calls (orphaned после crash до mark_processing).
    #[tokio::test]
    async fn sweep_marks_pending_chunks_of_non_active_calls_failed() {
        use crate::db::chunks::{insert_chunk, list_chunks_by_call};
        use std::path::PathBuf;
        let db = fresh_db().await;
        // Call A — был processing, имеет pending chunk → после sweep:
        // call → failed, chunk → failed.
        let a = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &a.id, 5.0).await.unwrap();
        // a сейчас status='processing' (finish_recording). Создаём pending chunk.
        insert_chunk(
            &db.pool,
            &a.id,
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        // Call B — уже ready, тоже с pending chunk (legacy / partial state).
        let b = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &b.id, 5.0).await.unwrap();
        mark_call_ready(&db.pool, &b.id).await.unwrap();
        insert_chunk(
            &db.pool,
            &b.id,
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();

        sweep_stale_calls(&db.pool).await.unwrap();

        let a_chunks = list_chunks_by_call(&db.pool, &a.id).await.unwrap();
        let b_chunks = list_chunks_by_call(&db.pool, &b.id).await.unwrap();
        assert_eq!(
            a_chunks[0].status, "failed",
            "processing call's pending chunk → failed"
        );
        assert_eq!(
            b_chunks[0].status, "failed",
            "ready call's pending chunk → failed"
        );
    }

    #[tokio::test]
    async fn sweep_stale_calls_marks_only_processing_failed() {
        // [B19.6] sweep handles ONLY 'processing' orphans (have finalized audio →
        // recoverable as 'failed'). 'recording' orphans are left for
        // reconcile_orphan_recordings (WAV-duration based delete/fail).
        let db = fresh_db().await;
        let a = insert_recording(&db.pool, "managed").await.unwrap(); // recording
        let b = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &b.id, 5.0).await.unwrap(); // processing
        let c = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &c.id, 5.0).await.unwrap();
        mark_call_ready(&db.pool, &c.id).await.unwrap(); // ready

        let affected = sweep_stale_calls(&db.pool).await.unwrap();
        assert_eq!(affected, 1, "only b (processing) → failed");

        let a_after = get_call(&db.pool, &a.id).await.unwrap().unwrap();
        let b_after = get_call(&db.pool, &b.id).await.unwrap().unwrap();
        let c_after = get_call(&db.pool, &c.id).await.unwrap().unwrap();
        assert_eq!(a_after.status, "recording", "recording left for reconcile");
        assert_eq!(b_after.status, "failed");
        assert_eq!(c_after.status, "ready");
    }

    // [B28.2] Кандидаты авто-восстановления: failed без reason (sweep/краш),
    // но НЕ настоящие фейлы пайплайна (mark_call_failed с reason).
    #[tokio::test]
    async fn list_interrupted_failed_calls_excludes_real_failures() {
        let db = fresh_db().await;
        // a: прерван (sweep: processing → failed без reason).
        let a = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &a.id, 5.0).await.unwrap();
        sweep_stale_calls(&db.pool).await.unwrap();
        // b: настоящий фейл пайплайна — с reason.
        let b = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &b.id, 5.0).await.unwrap();
        fail_recording_with_reason(&db.pool, &b.id, Some("stt_failed"))
            .await
            .unwrap();
        // c: ready — не кандидат.
        let c = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &c.id, 5.0).await.unwrap();
        mark_call_ready(&db.pool, &c.id).await.unwrap();

        let ids = list_interrupted_failed_calls(&db.pool).await.unwrap();
        assert_eq!(ids, vec![a.id.clone()], "только прерванный без reason");
    }

    #[tokio::test]
    async fn list_orphan_recording_ids_returns_only_recording() {
        let db = fresh_db().await;
        let a = insert_recording(&db.pool, "managed").await.unwrap(); // recording
        let b = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &b.id, 5.0).await.unwrap(); // processing
        let ids = list_orphan_recording_ids(&db.pool).await.unwrap();
        assert_eq!(ids, vec![a.id], "only the recording-status orphan");
    }
}
