use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use crate::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Call {
    pub id: String,
    pub title: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_sec: Option<i64>,
    pub status: String,
    pub provider: Option<String>,
    pub path_label: String,
    pub lang_detected: Option<String>,
    /// M2.7 (#23): UX-readable причина при status=failed.
    pub failed_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Вставить запись о новой записи в статусе `recording`. path_label = managed|byo.
/// Возвращает созданную строку.
pub async fn insert_recording(pool: &SqlitePool, path_label: &str) -> Result<Call, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
         VALUES (?1, ?2, 'recording', ?3, ?2, ?2)",
    )
    .bind(&id)
    .bind(&now)
    .bind(path_label)
    .execute(pool)
    .await?;

    Ok(Call {
        id,
        title: None,
        started_at: now.clone(),
        ended_at: None,
        duration_sec: None,
        status: "recording".into(),
        provider: None,
        path_label: path_label.into(),
        lang_detected: None,
        failed_reason: None,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// Перевести запись из recording → processing с фактической длительностью.
/// processing — потому что после остановки записи дальше идёт STT → matching → recap.
/// Финальный статус ready проставит recap pipeline (#28).
pub async fn finish_recording(
    pool: &SqlitePool,
    call_id: &str,
    duration_sec: f64,
) -> Result<Call, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let duration_secs_i64 = duration_sec.round() as i64;

    sqlx::query(
        "UPDATE calls
         SET status = 'processing',
             ended_at = ?2,
             duration_sec = ?3,
             updated_at = ?2
         WHERE id = ?1",
    )
    .bind(call_id)
    .bind(&now)
    .bind(duration_secs_i64)
    .execute(pool)
    .await?;

    get_call(pool, call_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("call {call_id} disappeared")))
}

/// Перевести запись в финальный статус `ready` после успешного pipeline'а.
pub async fn mark_call_ready(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE calls SET status = 'ready', updated_at = ?1 WHERE id = ?2")
        .bind(&now)
        .bind(call_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Проставить `lang_detected` и `provider` (последний фактически использованный)
/// после транскрипции. Статус остаётся `processing` — финальный `ready` ставит
/// `mark_call_ready` после всех артефактов.
pub async fn set_call_meta(
    pool: &SqlitePool,
    call_id: &str,
    lang_detected: Option<&str>,
    provider: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE calls
         SET lang_detected = ?1,
             provider = ?2,
             updated_at = ?3
         WHERE id = ?4",
    )
    .bind(lang_detected)
    .bind(provider)
    .bind(&now)
    .bind(call_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Stale-sweep: при старте приложения все `recording` и `processing` row'ы
/// помечаются `failed`. Это означает что в прошлой сессии запись или
/// пайплайн были прерваны (краш, force-quit, потеря питания). Возвращает
/// количество затронутых строк — пригодится для лога.
pub async fn sweep_stale_calls(pool: &SqlitePool) -> Result<u64, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE calls
         SET status = 'failed',
             ended_at = COALESCE(ended_at, ?1),
             updated_at = ?1
         WHERE status IN ('recording', 'processing')",
    )
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Пометить запись как failed (sidecar сломался, тайм-аут и т.п.).
/// Старая сигнатура — без причины. Новый код использует `fail_recording_with_reason`.
pub async fn fail_recording(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    fail_recording_with_reason(pool, call_id, None).await
}

/// M2.7 (#23): пометить failed с UX-readable причиной для отображения в UI.
/// `reason` коротко: «STT недоступен», «Quota исчерпана», «Auth — проверь ключи».
pub async fn fail_recording_with_reason(
    pool: &SqlitePool,
    call_id: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE calls
         SET status = 'failed',
             ended_at = ?2,
             failed_reason = ?3,
             updated_at = ?2
         WHERE id = ?1",
    )
    .bind(call_id)
    .bind(&now)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_call(pool: &SqlitePool, call_id: &str) -> Result<Option<Call>, AppError> {
    let row: Option<Call> = sqlx::query_as(
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, created_at, updated_at
         FROM calls WHERE id = ?1",
    )
    .bind(call_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Все звонки от свежих к старым. FTS-поиск по транскриптам/рекапу
/// подключится в #30 follow-up когда они начнут писаться (#22, #28).
pub async fn list_calls(pool: &SqlitePool) -> Result<Vec<Call>, AppError> {
    let rows: Vec<Call> = sqlx::query_as(
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, created_at, updated_at
         FROM calls
         ORDER BY started_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    #[tokio::test]
    async fn insert_recording_creates_call_in_recording_status() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        assert_eq!(call.status, "recording");
        assert_eq!(call.path_label, "managed");
        assert!(call.duration_sec.is_none());
        assert!(call.ended_at.is_none());
    }

    #[tokio::test]
    async fn finish_recording_transitions_to_processing_with_duration() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "byo").await.unwrap();
        let finished = finish_recording(&db.pool, &call.id, 123.49).await.unwrap();
        assert_eq!(finished.status, "processing");
        assert_eq!(finished.duration_sec, Some(123));
        assert!(finished.ended_at.is_some());
    }

    #[tokio::test]
    async fn mark_call_ready_sets_ready_status() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &call.id, 10.0).await.unwrap();
        mark_call_ready(&db.pool, &call.id).await.unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "ready");
    }

    #[tokio::test]
    async fn fail_recording_sets_failed_and_ended_at() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        fail_recording(&db.pool, &call.id).await.unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "failed");
        assert!(after.ended_at.is_some());
        assert!(after.failed_reason.is_none());
    }

    #[tokio::test]
    async fn fail_recording_with_reason_persists_failed_reason() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        fail_recording_with_reason(&db.pool, &call.id, Some("STT недоступен"))
            .await
            .unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "failed");
        assert_eq!(after.failed_reason.as_deref(), Some("STT недоступен"));
    }

    #[tokio::test]
    async fn set_call_meta_writes_lang_and_provider() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        set_call_meta(&db.pool, &call.id, Some("ru"), "soniox")
            .await
            .unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.provider.as_deref(), Some("soniox"));
        assert_eq!(after.lang_detected.as_deref(), Some("ru"));
    }

    #[tokio::test]
    async fn sweep_stale_calls_marks_recording_and_processing_failed() {
        let db = fresh_db().await;
        let a = insert_recording(&db.pool, "managed").await.unwrap();
        let b = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &b.id, 5.0).await.unwrap();
        let c = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &c.id, 5.0).await.unwrap();
        mark_call_ready(&db.pool, &c.id).await.unwrap();

        let affected = sweep_stale_calls(&db.pool).await.unwrap();
        assert_eq!(
            affected, 2,
            "a recording + b processing → failed; c ready unchanged"
        );

        let a_after = get_call(&db.pool, &a.id).await.unwrap().unwrap();
        let b_after = get_call(&db.pool, &b.id).await.unwrap().unwrap();
        let c_after = get_call(&db.pool, &c.id).await.unwrap().unwrap();
        assert_eq!(a_after.status, "failed");
        assert_eq!(b_after.status, "failed");
        assert_eq!(c_after.status, "ready");
    }

    #[tokio::test]
    async fn list_calls_orders_by_started_desc() {
        let db = fresh_db().await;
        let first = insert_recording(&db.pool, "managed").await.unwrap();
        // Гарантируем разный started_at (rfc3339 секундная гранулярность).
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let second = insert_recording(&db.pool, "managed").await.unwrap();
        let list = list_calls(&db.pool).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, second.id, "newest first");
        assert_eq!(list[1].id, first.id);
    }

    #[tokio::test]
    async fn get_call_returns_none_for_missing() {
        let db = fresh_db().await;
        assert!(get_call(&db.pool, "no-such-id").await.unwrap().is_none());
    }
}
