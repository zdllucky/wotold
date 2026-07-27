use sqlx::SqlitePool;

use crate::AppError;

/// [B3.1] Persist извлечённого pipeline'ом cluster embedding в call_speakers.
/// Сохраняем little-endian f32 blob (см. embedding_to_bytes в embeddings.rs).
///
/// [Phase 3 R9] Раньше эта функция также делала voice_samples backfill
/// (consent-check + DELETE+INSERT + eviction) — теперь это side-effect вынесен
/// в `pipeline::voice_backfill::maybe_backfill_voice_sample`, который вызывается
/// из `run_cluster_pipeline` ПОСЛЕ persist'а cluster'а. Это:
/// - Позволяет тестировать `set_call_speaker_cluster` без contacts/consent.
/// - Делает pipeline-flow явным (UPDATE cluster → THEN backfill if eligible).
/// - Не меняет публичную сигнатуру и идемпотентность.
pub async fn set_call_speaker_cluster(
    pool: &SqlitePool,
    call_id: &str,
    speaker_tag: &str,
    embedding_blob: &[u8],
) -> Result<(), AppError> {
    let res = sqlx::query(
        "UPDATE call_speakers
         SET cluster_embedding = ?1
         WHERE call_id = ?2 AND speaker_tag = ?3",
    )
    .bind(embedding_blob)
    .bind(call_id)
    .bind(speaker_tag)
    .execute(pool)
    .await?;
    // [TD-43] Строки `(call_id, speaker_tag)` может не быть — тогда UPDATE
    // молча no-op и спикер остаётся без биометрии. Тихая потеря запрещена
    // (инженерное правило 3): деградация обязана быть видимой.
    if res.rows_affected() == 0 {
        log::warn!(
            "set_call_speaker_cluster: нет строки call_speakers для call {call_id} \
             speaker {speaker_tag} — cluster embedding потерян, \
             спикер останется без биометрии"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::calls::insert_recording;
    use crate::db::test_support::{fresh_db, insert_speaker_row};

    #[tokio::test]
    async fn set_cluster_on_missing_row_does_not_error() {
        // [TD-43] UPDATE без совпадающей строки — no-op. Контракт остаётся
        // мягким (вызывающий `run_cluster_pipeline` логирует и продолжает),
        // но потеря теперь видна в warn. Близнец —
        // `suggestions::set_suggestion_on_missing_row_does_not_error`.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();

        let res =
            set_call_speaker_cluster(&db.pool, &call.id, "speaker:нет-такого", &[1u8, 2, 3, 4])
                .await;
        assert!(res.is_ok(), "контракт: мягкая деградация, не Err");

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM call_speakers WHERE call_id = ?1")
                .bind(&call.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "UPDATE не должен создавать строку");
    }

    #[tokio::test]
    async fn set_cluster_on_existing_row_writes_blob() {
        // Позитивная половина: без неё warn-ветка «прошла» бы тест даже при
        // всегда-нулевом rows_affected.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        insert_speaker_row(&db.pool, &call.id, "speaker:1", None).await;

        let blob = vec![9u8, 8, 7, 6];
        set_call_speaker_cluster(&db.pool, &call.id, "speaker:1", &blob)
            .await
            .unwrap();

        let stored: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT cluster_embedding FROM call_speakers WHERE call_id = ?1 AND speaker_tag = ?2",
        )
        .bind(&call.id)
        .bind("speaker:1")
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(stored.as_deref(), Some(blob.as_slice()));
    }
}
