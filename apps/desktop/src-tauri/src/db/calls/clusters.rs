use sqlx::SqlitePool;

use crate::AppError;

/// [B3.1] Persist извлечённого pipeline'ом cluster embedding в call_speakers.
/// Сохраняем little-endian f32 blob (см. embedding_to_bytes в embeddings.rs).
///
/// [B3.7 backfill] Если speaker УЖЕ confirmed и контакт opt-in
/// (consent_voice='true'), идемпотентно upsert'им voice_sample. Это
/// покрывает кейс reprocess'а: юзер подтвердил speaker'а ДО того как
/// модель посчитала cluster (в dev/StubEmbedder режиме). При повторном
/// прогоне pipeline'а с реальной моделью — sample сохраняется ретроактивно.
///
/// Идемпотентность: DELETE existing voice_samples WHERE
/// (contact_id, source_call=call_id) → INSERT новый. Гарантирует ровно
/// один sample per (contact_id, call_id), даже после многократных
/// reprocess'ов.
pub async fn set_call_speaker_cluster(
    pool: &SqlitePool,
    call_id: &str,
    speaker_tag: &str,
    embedding_blob: &[u8],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE call_speakers
         SET cluster_embedding = ?1
         WHERE call_id = ?2 AND speaker_tag = ?3",
    )
    .bind(embedding_blob)
    .bind(call_id)
    .bind(speaker_tag)
    .execute(&mut *tx)
    .await?;

    // Backfill: проверяем confirmed + consent_voice → upsert voice_sample.
    let row = sqlx::query(
        "SELECT cs.contact_id, cs.confirmed, cs.suggestion_score, c.attributes
         FROM call_speakers cs
         LEFT JOIN contacts c ON c.id = cs.contact_id
         WHERE cs.call_id = ?1 AND cs.speaker_tag = ?2",
    )
    .bind(call_id)
    .bind(speaker_tag)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(row) = row {
        use sqlx::Row;
        let contact_id: Option<String> = row.try_get("contact_id")?;
        let confirmed: bool = row.try_get::<i64, _>("confirmed")? != 0;
        let suggestion_score: Option<f64> = row.try_get("suggestion_score")?;
        let attrs_json: Option<String> = row.try_get("attributes")?;

        if confirmed {
            if let Some(cid) = contact_id {
                let consent = attrs_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                    .and_then(|v| {
                        v.get("consent_voice")
                            .and_then(|c| c.as_str())
                            .map(|s| s == "true")
                    })
                    .unwrap_or(false);
                if consent && !embedding_blob.is_empty() {
                    // Idempotent upsert: cleanup any prior sample для этой пары.
                    sqlx::query(
                        "DELETE FROM voice_samples
                         WHERE contact_id = ?1 AND source_call = ?2",
                    )
                    .bind(&cid)
                    .bind(call_id)
                    .execute(&mut *tx)
                    .await?;
                    let voice_sample_id = uuid::Uuid::new_v4().to_string();
                    let now = chrono::Utc::now().to_rfc3339();
                    let quality = suggestion_score.unwrap_or(1.0);
                    sqlx::query(
                        "INSERT INTO voice_samples
                           (id, contact_id, embedding, source_call, quality, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )
                    .bind(&voice_sample_id)
                    .bind(&cid)
                    .bind(embedding_blob)
                    .bind(call_id)
                    .bind(quality)
                    .bind(&now)
                    .execute(&mut *tx)
                    .await?;
                    log::info!(
                        "voice_sample backfilled (reprocess): contact={cid} call={call_id} quality={quality:.3}"
                    );
                    // [B3.8] N-cap rotation после INSERT.
                    crate::db::voice_samples::evict_old_voice_samples(&mut *tx, &cid).await?;
                }
            }
        }
    }

    tx.commit().await?;
    Ok(())
}
