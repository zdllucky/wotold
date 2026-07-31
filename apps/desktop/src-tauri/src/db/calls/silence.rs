//! [T5/R15] Точка реза тихого хвоста — `calls.silence_trim_ms`.
//!
//! Пишется один раз на стопе, и только при авто-стопе по тишине; читается
//! пайплайном, который по ней подрезает корневые WAV и выбрасывает сегменты
//! транскрипта за резом. `None` — запись остановлена руками, аудио не трогаем
//! (решение владельца).
//!
//! Отдельным модулем, а не полем в [`crate::db::Call`]: значение нужно ровно
//! двум местам (стоп и пайплайн), а `Call` тащится в каждый список звонков и в
//! webview — расширять его ради внутренней координации не за что.

use sqlx::SqlitePool;

use crate::AppError;

/// Запомнить точку реза. Значение — смещение от начала записи в мс, то есть
/// длина, которую следует оставить.
pub async fn set_silence_trim_ms(
    pool: &SqlitePool,
    call_id: &str,
    trim_at_ms: u64,
) -> Result<(), AppError> {
    sqlx::query("UPDATE calls SET silence_trim_ms = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(trim_at_ms as i64)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(call_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Прочитать точку реза. `None` — резать не надо (ручной стоп, старая запись
/// до 0025, либо звонка уже нет).
///
/// Отрицательные и нулевые значения отбрасываются: SQLite не enforce'ит CHECK
/// на этой колонке, а нулевой рез означал бы «оставить пустую дорожку» —
/// подрезка на это отвечает ошибкой, и доводить до неё незачем.
pub async fn get_silence_trim_ms(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Option<u64>, AppError> {
    let raw: Option<Option<i64>> =
        sqlx::query_scalar("SELECT silence_trim_ms FROM calls WHERE id = ?1")
            .bind(call_id)
            .fetch_optional(pool)
            .await?;
    Ok(raw.flatten().filter(|ms| *ms > 0).map(|ms| ms as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::calls::insert_recording;
    use crate::db::test_support::fresh_db;

    #[tokio::test]
    async fn manual_stop_leaves_no_trim_point() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "local").await.unwrap();
        assert_eq!(
            get_silence_trim_ms(&db.pool, &call.id).await.unwrap(),
            None,
            "по умолчанию колонка пуста — аудио не трогаем"
        );
    }

    #[tokio::test]
    async fn roundtrips_trim_point() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "local").await.unwrap();
        set_silence_trim_ms(&db.pool, &call.id, 1_505_000)
            .await
            .unwrap();
        assert_eq!(
            get_silence_trim_ms(&db.pool, &call.id).await.unwrap(),
            Some(1_505_000)
        );
    }

    #[tokio::test]
    async fn overwrite_wins_last_value() {
        // Переобработка звонка не пишет сюда, но восстановление после краша
        // могло бы — значение должно быть последним, а не накопленным.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "local").await.unwrap();
        set_silence_trim_ms(&db.pool, &call.id, 1_000)
            .await
            .unwrap();
        set_silence_trim_ms(&db.pool, &call.id, 2_000)
            .await
            .unwrap();
        assert_eq!(
            get_silence_trim_ms(&db.pool, &call.id).await.unwrap(),
            Some(2_000)
        );
    }

    #[tokio::test]
    async fn garbage_values_read_as_no_trim() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "local").await.unwrap();
        for bad in [0i64, -1, -999] {
            sqlx::query("UPDATE calls SET silence_trim_ms = ?1 WHERE id = ?2")
                .bind(bad)
                .bind(&call.id)
                .execute(&db.pool)
                .await
                .unwrap();
            assert_eq!(
                get_silence_trim_ms(&db.pool, &call.id).await.unwrap(),
                None,
                "{bad}: нулевой и отрицательный рез — не рез"
            );
        }
    }

    #[tokio::test]
    async fn unknown_call_reads_as_none_not_error() {
        let db = fresh_db().await;
        assert_eq!(get_silence_trim_ms(&db.pool, "ghost").await.unwrap(), None);
    }
}
