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
    /// [W2] RFC3339 timestamp когда юзер нажал pause. NULL означает что запись
    /// сейчас не на паузе (recording или уже завершена). Используется только
    /// для recording rows; при finish_recording проставленный paused_at
    /// автоматически сворачивается в paused_total_ms.
    pub paused_at: Option<String>,
    /// [W2] Накопленная длительность пауз в миллисекундах. Pipeline и UI
    /// вычитают это значение из (ended_at - started_at), чтобы получить
    /// фактическое время записи аудио.
    pub paused_total_ms: i64,
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
        paused_at: None,
        paused_total_ms: 0,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// [W2] Pause активную запись. SETs `paused_at = now()` если поле было NULL.
/// Идемпотентно: если запись уже на паузе — log warning и Ok(()) (frontend мог
/// дважды нажать кнопку или гонка между hotkey и UI button).
pub async fn pause_call(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    // Условный UPDATE: меняем только если paused_at IS NULL, иначе noop.
    let res = sqlx::query(
        "UPDATE calls
         SET paused_at = ?2,
             updated_at = ?2
         WHERE id = ?1 AND paused_at IS NULL",
    )
    .bind(call_id)
    .bind(&now)
    .execute(pool)
    .await?;

    if res.rows_affected() == 0 {
        // Проверим, существует ли вообще такая запись — если нет, это явная
        // ошибка вызывающего; если есть — уже paused, idempotent noop.
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM calls WHERE id = ?1")
            .bind(call_id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Err(AppError::Other(format!(
                "pause_call: call {call_id} not found"
            )));
        }
        log::warn!("pause_call: call {call_id} already paused, idempotent noop");
    }
    Ok(())
}

/// [W2] Resume записи с паузы. Если paused_at IS NOT NULL — вычисляем
/// (now - paused_at) в мс, добавляем к paused_total_ms, очищаем paused_at.
/// Если запись не на паузе — Ok(()) без изменений (idempotent).
pub async fn resume_call(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    let row: Option<(Option<String>, i64)> =
        sqlx::query_as("SELECT paused_at, paused_total_ms FROM calls WHERE id = ?1")
            .bind(call_id)
            .fetch_optional(pool)
            .await?;

    let (paused_at, paused_total_ms) = match row {
        Some(r) => r,
        None => {
            return Err(AppError::Other(format!(
                "resume_call: call {call_id} not found"
            )));
        }
    };

    let Some(paused_at_str) = paused_at else {
        // Уже resumed — noop, не ошибка.
        log::debug!("resume_call: call {call_id} not paused, noop");
        return Ok(());
    };

    let paused_at_dt = chrono::DateTime::parse_from_rfc3339(&paused_at_str)
        .map_err(|e| AppError::Other(format!("paused_at parse failed: {e}")))?;
    let now = chrono::Utc::now();
    let elapsed_ms = (now - paused_at_dt.with_timezone(&chrono::Utc))
        .num_milliseconds()
        .max(0);
    let new_total = paused_total_ms.saturating_add(elapsed_ms);
    let now_str = now.to_rfc3339();

    sqlx::query(
        "UPDATE calls
         SET paused_at = NULL,
             paused_total_ms = ?2,
             updated_at = ?3
         WHERE id = ?1",
    )
    .bind(call_id)
    .bind(new_total)
    .bind(&now_str)
    .execute(pool)
    .await?;
    Ok(())
}

/// Перевести запись из recording → processing с фактической длительностью.
/// processing — потому что после остановки записи дальше идёт STT → matching → recap.
/// Финальный статус ready проставит recap pipeline (#28).
///
/// [W2] `duration_sec` уже учитывает накопленные паузы — caller (audio sidecar)
/// возвращает реальное время аудио. Если user забыл нажать resume и сразу
/// нажал stop, мы сворачиваем lingering paused_at в paused_total_ms и очищаем
/// поле паузы (resume-then-stop семантика).
pub async fn finish_recording(
    pool: &SqlitePool,
    call_id: &str,
    duration_sec: f64,
) -> Result<Call, AppError> {
    // [W2] Если был забытый pause — выполняем неявный resume сейчас, чтобы
    // paused_total_ms остался согласованным и не торчал paused_at у завершённой
    // записи. resume_call идемпотентен для non-paused.
    resume_call(pool, call_id).await?;

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
///
/// [M13 review fix] Также sweep'аем `call_chunks` в `processing` — без этого
/// crash во время chunk_runner оставлял бы row застрявшим, и
/// `chunk_assembly::load_chunked_transcripts` молча skip'ал бы его (filter
/// status='done'), что приводило бы к silent data loss для chunk'а в
/// финальном transcript'е.
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
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, pipeline_step, pipeline_pct, pipeline_eta_sec, upload_bytes, paused_at, paused_total_ms, created_at, updated_at
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
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, pipeline_step, pipeline_pct, pipeline_eta_sec, upload_bytes, paused_at, paused_total_ms, created_at, updated_at
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

    // ============================================================
    // [W2] pause / resume
    // ============================================================

    #[tokio::test]
    async fn insert_recording_initializes_pause_fields_to_zero() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        assert!(call.paused_at.is_none());
        assert_eq!(call.paused_total_ms, 0);
        // Reload — поля действительно записаны через DEFAULT.
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert!(after.paused_at.is_none());
        assert_eq!(after.paused_total_ms, 0);
    }

    #[tokio::test]
    async fn pause_call_sets_timestamp() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        pause_call(&db.pool, &call.id).await.unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert!(after.paused_at.is_some(), "paused_at должен быть выставлен");
        assert_eq!(after.paused_total_ms, 0);
        // Парсится как валидный rfc3339.
        let parsed = chrono::DateTime::parse_from_rfc3339(after.paused_at.as_deref().unwrap());
        assert!(parsed.is_ok());
    }

    #[tokio::test]
    async fn pause_already_paused_is_idempotent() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        pause_call(&db.pool, &call.id).await.unwrap();
        let first = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        let first_paused_at = first.paused_at.clone();
        assert!(first_paused_at.is_some());

        // Второй вызов — Ok без error, paused_at не должен перезаписаться
        // (это бы исказило накопленное время паузы).
        pause_call(&db.pool, &call.id).await.unwrap();
        let second = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(second.paused_at, first_paused_at);
    }

    #[tokio::test]
    async fn pause_unknown_call_returns_error() {
        let db = fresh_db().await;
        let res = pause_call(&db.pool, "nonexistent-id").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn resume_clears_timestamp_and_accumulates_total() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        pause_call(&db.pool, &call.id).await.unwrap();
        // Симулируем реальную паузу.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        resume_call(&db.pool, &call.id).await.unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert!(after.paused_at.is_none(), "paused_at должен очиститься");
        // ≥ 100ms запас, в worst case CI medium может быть 120-130ms.
        assert!(
            after.paused_total_ms >= 100,
            "paused_total_ms должен накопить ~150ms, got {}",
            after.paused_total_ms
        );
        // Sanity — но не больше секунды (мы спали только 150ms).
        assert!(after.paused_total_ms < 1000);
    }

    #[tokio::test]
    async fn resume_when_not_paused_is_noop() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        // Never paused → resume Ok, без изменений.
        resume_call(&db.pool, &call.id).await.unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert!(after.paused_at.is_none());
        assert_eq!(after.paused_total_ms, 0);
    }

    #[tokio::test]
    async fn multiple_pause_resume_cycles_accumulate_correctly() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();

        for _ in 0..3 {
            pause_call(&db.pool, &call.id).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            resume_call(&db.pool, &call.id).await.unwrap();
            // Короткий «живой» период между паузами.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert!(after.paused_at.is_none(), "финально не на паузе");
        // 3 × ~80ms ≈ 240ms. С запасом на CI jitter: ≥ 180ms.
        assert!(
            after.paused_total_ms >= 180,
            "expected ~240ms accumulated, got {}",
            after.paused_total_ms
        );
        assert!(after.paused_total_ms < 1500);
    }

    #[tokio::test]
    async fn finish_recording_with_lingering_paused_at_folds_pause_into_total() {
        // Сценарий: user нажал pause и забыл нажать resume, потом stop.
        // Ожидание: finish_recording неявно сворачивает pending pause
        // в paused_total_ms, paused_at очищается, статус идёт processing.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        pause_call(&db.pool, &call.id).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;

        let finished = finish_recording(&db.pool, &call.id, 30.0).await.unwrap();
        assert_eq!(finished.status, "processing");
        assert!(
            finished.paused_at.is_none(),
            "lingering paused_at должен схлопнуться"
        );
        assert!(
            finished.paused_total_ms >= 80,
            "pending pause должна быть учтена, got {}",
            finished.paused_total_ms
        );
        assert!(finished.ended_at.is_some());
        assert_eq!(finished.duration_sec, Some(30));
    }

    #[tokio::test]
    async fn finish_recording_without_pause_keeps_total_zero() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let finished = finish_recording(&db.pool, &call.id, 5.0).await.unwrap();
        assert_eq!(finished.paused_total_ms, 0);
        assert!(finished.paused_at.is_none());
    }
}
