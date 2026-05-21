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
    sqlx::query(
        "UPDATE call_speakers
         SET cluster_embedding = ?1
         WHERE call_id = ?2 AND speaker_tag = ?3",
    )
    .bind(embedding_blob)
    .bind(call_id)
    .bind(speaker_tag)
    .execute(pool)
    .await?;
    Ok(())
}
