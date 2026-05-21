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
    /// [B16]: причина если recap LLM упал. Звонок остаётся 'ready' (транскрипт
    /// есть), но UI знает что саммари нужно пересоздать.
    pub recap_failed_reason: Option<String>,
    /// [V6.2] Pipeline progress fields для async-states UI. NULL когда звонок
    /// recording / ready / failed / шаг не начался — UI рендерит ProgressRail
    /// только при `status='processing' && pipeline_step IS NOT NULL`.
    pub pipeline_step: Option<i64>,
    pub pipeline_pct: Option<i64>,
    pub pipeline_eta_sec: Option<i64>,
    pub upload_bytes: Option<i64>,
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
        recap_failed_reason: None,
        pipeline_step: None,
        pipeline_pct: None,
        pipeline_eta_sec: None,
        upload_bytes: None,
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

/// [B16]: записать причину recap-fail. Звонок остаётся 'ready' (транскрипт
/// сохранён), но UI знает что саммари недоступно. None → очистить
/// (например после успешного regenerate).
pub async fn set_recap_failed_reason(
    pool: &SqlitePool,
    call_id: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE calls SET recap_failed_reason = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(reason)
        .bind(&now)
        .bind(call_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Перевести запись в финальный статус `ready` после успешного pipeline'а.
/// [V6.2] Заодно очищаем pipeline_* поля — звонок больше не "в обработке",
/// UI не должен рендерить ProgressRail.
pub async fn mark_call_ready(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE calls
         SET status = 'ready',
             pipeline_step = NULL,
             pipeline_pct = NULL,
             pipeline_eta_sec = NULL,
             upload_bytes = NULL,
             updated_at = ?1
         WHERE id = ?2",
    )
    .bind(&now)
    .bind(call_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// [V6.2] Обновить pipeline_step / pct / eta / upload_bytes. Pipeline вызывает
/// перед каждым меняющимся шагом — UI получает live tick через `call:progress`
/// event (см. pipeline::emit_progress). Без транзакции: одна строка, одна
/// колонка, идемпотент при concurrent writers (последний выигрывает).
pub async fn set_call_progress(
    pool: &SqlitePool,
    call_id: &str,
    step: u8,
    pct: u8,
    eta_sec: Option<i64>,
    upload_bytes: Option<i64>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE calls
         SET pipeline_step = ?1,
             pipeline_pct = ?2,
             pipeline_eta_sec = ?3,
             upload_bytes = ?4,
             updated_at = ?5
         WHERE id = ?6",
    )
    .bind(step as i64)
    .bind(pct as i64)
    .bind(eta_sec)
    .bind(upload_bytes)
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

/// [B17 V4.0] Persist LLM-generated call title. Called from recap pipeline
/// после успешной генерации JSON. Frontend reads через get_call → renders
/// в header вместо fallback "Звонок · 20 мая". Empty/blank title не
/// перезаписывает существующий.
pub async fn set_call_title(pool: &SqlitePool, call_id: &str, title: &str) -> Result<(), AppError> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE calls
         SET title = ?1,
             updated_at = ?2
         WHERE id = ?3",
    )
    .bind(trimmed)
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
    // [V6.2] Очищаем pipeline_* — звонок больше не processing, UI должен
    // показывать error variant, а не ProgressRail.
    sqlx::query(
        "UPDATE calls
         SET status = 'failed',
             ended_at = ?2,
             failed_reason = ?3,
             pipeline_step = NULL,
             pipeline_pct = NULL,
             pipeline_eta_sec = NULL,
             upload_bytes = NULL,
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
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, pipeline_step, pipeline_pct, pipeline_eta_sec, upload_bytes, created_at, updated_at
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
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, pipeline_step, pipeline_pct, pipeline_eta_sec, upload_bytes, created_at, updated_at
         FROM calls
         ORDER BY started_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// C5 (#41) cascade delete: удаляет calls row + связанные строки
/// (action_items, call_speakers по CASCADE FK; voice_samples с source_call=id
/// удаляются явно — FK с ON DELETE SET NULL логически некорректен здесь).
/// Audio-файлы на диске чистит вызывающий — DB слой не знает path.
pub async fn delete_call_and_samples(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    // C3: voice_samples.source_call ссылается на этот call — очистим эмбеддинги.
    sqlx::query("DELETE FROM voice_samples WHERE source_call = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    // action_items + call_speakers идут по ON DELETE CASCADE (см. 0001_initial.sql).
    sqlx::query("DELETE FROM calls WHERE id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
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

    #[tokio::test]
    async fn delete_call_removes_row_and_voice_samples() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();

        // Создаём контакт + voice_sample привязанный к этому звонку.
        let owner = crate::db::ensure_owner_contact(&db.pool).await.unwrap();
        sqlx::query(
            "INSERT INTO voice_samples (id, contact_id, embedding, source_call, quality, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("vs-1")
        .bind(&owner.id)
        .bind(vec![0u8; 4])
        .bind(&call.id)
        .bind(0.9)
        .bind("2026-05-20T00:00:00Z")
        .execute(&db.pool)
        .await
        .unwrap();

        delete_call_and_samples(&db.pool, &call.id).await.unwrap();

        assert!(get_call(&db.pool, &call.id).await.unwrap().is_none());
        let vs_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM voice_samples WHERE source_call = ?1")
                .bind(&call.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(vs_count, 0);
    }

    #[tokio::test]
    async fn delete_call_handles_missing_id_silently() {
        let db = fresh_db().await;
        // Не должен паниковать при несуществующем id (idempotent semantics).
        delete_call_and_samples(&db.pool, "ghost-id").await.unwrap();
    }

    #[tokio::test]
    async fn delete_call_cascades_action_items_and_speakers() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let owner = crate::db::ensure_owner_contact(&db.pool).await.unwrap();

        // Seed action_items для call.
        sqlx::query(
            "INSERT INTO action_items (id, call_id, text, owner_contact_id)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind("ai-1")
        .bind(&call.id)
        .bind("buy milk")
        .bind(&owner.id)
        .execute(&db.pool)
        .await
        .unwrap();

        // Seed call_speakers.
        sqlx::query(
            "INSERT INTO call_speakers (id, call_id, speaker_tag, contact_id, confirmed)
             VALUES (?1, ?2, ?3, ?4, 0)",
        )
        .bind("cs-1")
        .bind(&call.id)
        .bind("S1")
        .bind(&owner.id)
        .execute(&db.pool)
        .await
        .unwrap();

        delete_call_and_samples(&db.pool, &call.id).await.unwrap();

        let ai_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM action_items WHERE call_id = ?1")
                .bind(&call.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(ai_count, 0, "action_items должны быть cascade-deleted");

        let cs_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM call_speakers WHERE call_id = ?1")
                .bind(&call.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(cs_count, 0, "call_speakers должны быть cascade-deleted");
    }

    // ============================================================
    // [V6.2] pipeline progress
    // ============================================================

    #[tokio::test]
    async fn set_call_progress_persists_step_pct_eta_upload() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &call.id, 12.5).await.unwrap();

        set_call_progress(&db.pool, &call.id, 2, 64, Some(25), Some(1_048_576))
            .await
            .unwrap();

        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.pipeline_step, Some(2));
        assert_eq!(after.pipeline_pct, Some(64));
        assert_eq!(after.pipeline_eta_sec, Some(25));
        assert_eq!(after.upload_bytes, Some(1_048_576));
    }

    #[tokio::test]
    async fn mark_call_ready_clears_pipeline_progress() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &call.id, 5.0).await.unwrap();
        set_call_progress(&db.pool, &call.id, 5, 100, None, None)
            .await
            .unwrap();

        mark_call_ready(&db.pool, &call.id).await.unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "ready");
        assert!(after.pipeline_step.is_none(), "step должен очиститься");
        assert!(after.pipeline_pct.is_none());
        assert!(after.pipeline_eta_sec.is_none());
        assert!(after.upload_bytes.is_none());
    }

    #[tokio::test]
    async fn fail_recording_clears_pipeline_progress() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        set_call_progress(&db.pool, &call.id, 3, 50, Some(10), Some(2048))
            .await
            .unwrap();
        fail_recording_with_reason(&db.pool, &call.id, Some("STT down"))
            .await
            .unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "failed");
        assert!(after.pipeline_step.is_none());
        assert!(after.pipeline_pct.is_none());
    }
}
