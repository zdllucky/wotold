//! [W2 / TD-07] Пауза и возобновление записи.
//!
//! [TD-41] Выделено из `calls/lifecycle.rs` (1423 строки при лимите 800,
//! правило 8) по доменной границе, вместе с тестами. Пауза — отдельная фича
//! со своим инвариантом: `paused_total_ms` копится, `paused_at` очищается,
//! обе операции идемпотентны. Логика не менялась.

use sqlx::SqlitePool;

use crate::AppError;

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

#[cfg(test)]
mod tests {
    // Соседние функции звонка приходят через фасад `db::calls`:
    // тесты домена всё равно строят строку через `insert_recording`.
    use crate::db::calls::*;
    use crate::db::test_support::fresh_db;
    use sqlx::SqlitePool;

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

    /// [TD-32] Отодвинуть `paused_at` в прошлое на `ms`, чтобы `resume_call`
    /// увидел нужную длительность паузы БЕЗ настоящего ожидания. Тесты про
    /// накопление паузы измеряли её реальным `sleep` — полсекунды настенного
    /// времени и флаки на нагруженном раннере, при том что проверяется
    /// арифметика, а не часы.
    async fn backdate_pause(pool: &SqlitePool, call_id: &str, ms: i64) {
        let earlier = chrono::Utc::now() - chrono::Duration::milliseconds(ms);
        sqlx::query("UPDATE calls SET paused_at = ?1 WHERE id = ?2")
            .bind(earlier.to_rfc3339())
            .bind(call_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn resume_clears_timestamp_and_accumulates_total() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        pause_call(&db.pool, &call.id).await.unwrap();
        backdate_pause(&db.pool, &call.id, 150).await;
        resume_call(&db.pool, &call.id).await.unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert!(after.paused_at.is_none(), "paused_at должен очиститься");
        // Время задано явно, поэтому запас нужен только на округление.
        assert!(
            after.paused_total_ms >= 150,
            "paused_total_ms должен накопить ~150ms, got {}",
            after.paused_total_ms
        );
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
            backdate_pause(&db.pool, &call.id, 80).await;
            resume_call(&db.pool, &call.id).await.unwrap();
        }

        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert!(after.paused_at.is_none(), "финально не на паузе");
        // 3 × 80ms = 240ms ровно — время задано, jitter'а больше нет.
        assert!(
            after.paused_total_ms >= 240,
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
        backdate_pause(&db.pool, &call.id, 120).await;

        let finished = finish_recording(&db.pool, &call.id, 30.0).await.unwrap();
        assert_eq!(finished.status, "processing");
        assert!(
            finished.paused_at.is_none(),
            "lingering paused_at должен схлопнуться"
        );
        assert!(
            finished.paused_total_ms >= 120,
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
