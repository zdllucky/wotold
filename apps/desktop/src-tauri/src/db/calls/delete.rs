//! [C5 (#41)] Каскадное удаление звонка вместе с производными строками.
//!
//! [TD-41] Выделено из `calls/lifecycle.rs` (1423 строки при лимите 800,
//! правило 8) вместе с тестами. Отдельный модуль ещё и потому, что удаление —
//! security-review триггер (остаточные семплы, `voice_samples.source_call`),
//! и его тесты должны читаться рядом с ним. Логика не менялась.

use sqlx::SqlitePool;

use crate::AppError;

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
    // call_chunks (0013), action_items + call_speakers (0001) удаляются по
    // ON DELETE CASCADE (foreign_keys=ON при init pool) — отдельный DELETE не нужен.
    sqlx::query("DELETE FROM calls WHERE id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    // Соседние функции звонка приходят через фасад `db::calls`:
    // тесты домена всё равно строят строку через `insert_recording`.
    use crate::db::calls::*;
    use crate::db::test_support::fresh_db;

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
}
