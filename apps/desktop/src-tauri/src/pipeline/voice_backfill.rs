//! [Phase 3 R9] Voice sample backfill после persist'а cluster embedding'а.
//!
//! Раньше `db::set_call_speaker_cluster` после UPDATE'а делал ещё side-effect
//! — читал contact attributes, проверял `consent_voice`, удалял старый sample,
//! INSERT'ил новый, эвиктил past-cap. Логика была спрятана внутри SQL-helper'а
//! что мешало:
//! - Тестировать `set_call_speaker_cluster` изолированно — каждый тест должен
//!   был думать про consent/contacts/voice_samples.
//! - Решать в pipeline когда backfill уместен (например при reprocess'е после
//!   ручного confirm'а), а когда — нет.
//!
//! Теперь:
//! - `db::set_call_speaker_cluster` — только UPDATE call_speakers.cluster_embedding.
//! - `maybe_backfill_voice_sample` — отдельная функция, вызывается из
//!   `run_cluster_pipeline` ПОСЛЕ persist'а cluster'а.
//!
//! Идемпотентность: DELETE existing voice_sample для (contact_id, source_call=call_id)
//! → INSERT новый. Гарантирует ровно один sample per (contact_id, call_id),
//! даже после многократных reprocess'ов.

use sqlx::SqlitePool;

use crate::pipeline::voice_sample_picker::best_sample_segment;
use crate::AppError;

/// Если speaker УЖЕ confirmed и контакт opt-in (consent_voice='true'), то
/// идемпотентно upsert'ит voice_sample. Покрывает reprocess case: юзер
/// подтвердил speaker'а ДО того как модель посчитала cluster (Stub в dev),
/// при повторном run'е с реальной моделью — sample сохраняется retroactively.
///
/// Возвращает:
/// - `Ok(true)` — sample был upsert'нут (consent + confirmed + non-empty blob).
/// - `Ok(false)` — backfill пропущен (нет consent / не confirmed / no contact /
///   пустой embedding). Это НЕ ошибка — нормальный flow.
/// - `Err(_)` — SQL / serde ошибка. Caller'у решать (pipeline логирует warning).
///
/// [P4] `raw_stt_json` — содержимое merged `raw_stt.json` artifact (caller
/// читает с диска once per pipeline run). `None` либо malformed → INSERT
/// без slice metadata (graceful, не блокирует backfill).
pub async fn maybe_backfill_voice_sample(
    pool: &SqlitePool,
    call_id: &str,
    speaker_tag: &str,
    embedding_blob: &[u8],
    raw_stt_json: Option<&str>,
) -> Result<bool, AppError> {
    if embedding_blob.is_empty() {
        return Ok(false);
    }

    // Берём confirm / contact_id / score / consent одним JOIN'ом.
    let row = sqlx::query(
        "SELECT cs.contact_id, cs.confirmed, cs.suggestion_score, c.attributes
         FROM call_speakers cs
         LEFT JOIN contacts c ON c.id = cs.contact_id
         WHERE cs.call_id = ?1 AND cs.speaker_tag = ?2",
    )
    .bind(call_id)
    .bind(speaker_tag)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(false);
    };

    use sqlx::Row;
    let contact_id: Option<String> = row.try_get("contact_id")?;
    let confirmed: bool = row.try_get::<i64, _>("confirmed")? != 0;
    let suggestion_score: Option<f64> = row.try_get("suggestion_score")?;
    let attrs_json: Option<String> = row.try_get("attributes")?;

    if !confirmed {
        return Ok(false);
    }
    let Some(cid) = contact_id else {
        return Ok(false);
    };

    let consent = attrs_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| {
            v.get("consent_voice")
                .and_then(|c| c.as_str())
                .map(|s| s == "true")
        })
        .unwrap_or(false);

    if !consent {
        return Ok(false);
    }

    // Атомарный upsert + N-cap eviction в одной транзакции.
    let mut tx = pool.begin().await?;
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

    // [P4] Best-segment slice metadata — для playback short audio fragment.
    // None если raw_stt_json missing/malformed либо нет segments ≥ 1.5 sec
    // для speaker_tag. Legacy fallback: INSERT с NULL'ями (UI выключает play).
    let slice = raw_stt_json
        .and_then(|json| best_sample_segment(json, speaker_tag, crate::pipeline::merge::OWNER_TAG));
    let (start_sec, end_sec, track_kind) = match slice {
        Some((s, e, t)) => (Some(s), Some(e), Some(t.as_str().to_string())),
        None => (None, None, None),
    };

    sqlx::query(
        "INSERT INTO voice_samples
           (id, contact_id, embedding, source_call, quality, created_at,
            start_sec, end_sec, track_kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&voice_sample_id)
    .bind(&cid)
    .bind(embedding_blob)
    .bind(call_id)
    .bind(quality)
    .bind(&now)
    .bind(start_sec)
    .bind(end_sec)
    .bind(&track_kind)
    .execute(&mut *tx)
    .await?;

    crate::db::voice_samples::evict_old_voice_samples(&mut *tx, &cid).await?;
    tx.commit().await?;

    log::info!(
        "voice_sample backfilled: contact={cid} call={call_id} tag={speaker_tag} quality={quality:.3}"
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::insert_recording;
    use crate::db::test_support::fresh_db;

    async fn insert_contact_with_consent(pool: &SqlitePool, name: &str, consent: bool) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let attrs = if consent {
            "{\"consent_voice\":\"true\"}"
        } else {
            "{}"
        };
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?4, ?4)",
        )
        .bind(&id)
        .bind(name)
        .bind(attrs)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn insert_confirmed_speaker(
        pool: &SqlitePool,
        call_id: &str,
        tag: &str,
        contact_id: &str,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO call_speakers
               (id, call_id, speaker_tag, contact_id, suggestion_score, confirmed)
             VALUES (?1, ?2, ?3, ?4, 0.92, 1)",
        )
        .bind(&id)
        .bind(call_id)
        .bind(tag)
        .bind(contact_id)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn empty_blob_skips_backfill() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_with_consent(&db.pool, "Alice", true).await;
        insert_confirmed_speaker(&db.pool, &call.id, "S1", &alice).await;

        let inserted = maybe_backfill_voice_sample(&db.pool, &call.id, "S1", &[], None)
            .await
            .unwrap();
        assert!(!inserted);
        let samples: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM voice_samples")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(samples, 0, "пустой blob — ничего не upsert'ится");
    }

    #[tokio::test]
    async fn unknown_speaker_returns_false() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let inserted =
            maybe_backfill_voice_sample(&db.pool, &call.id, "ghost", &[1, 2, 3, 4], None)
                .await
                .unwrap();
        assert!(!inserted);
    }

    #[tokio::test]
    async fn unconfirmed_speaker_skips() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_with_consent(&db.pool, "Alice", true).await;
        // confirmed=0 — backfill не уместен.
        sqlx::query(
            "INSERT INTO call_speakers (id, call_id, speaker_tag, contact_id, confirmed)
             VALUES (?1, ?2, 'S1', ?3, 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&call.id)
        .bind(&alice)
        .execute(&db.pool)
        .await
        .unwrap();

        let inserted = maybe_backfill_voice_sample(&db.pool, &call.id, "S1", &[1, 2, 3, 4], None)
            .await
            .unwrap();
        assert!(!inserted);
    }

    #[tokio::test]
    async fn no_consent_skips() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let bob = insert_contact_with_consent(&db.pool, "Bob", false).await;
        insert_confirmed_speaker(&db.pool, &call.id, "S1", &bob).await;

        let inserted = maybe_backfill_voice_sample(&db.pool, &call.id, "S1", &[1, 2, 3, 4], None)
            .await
            .unwrap();
        assert!(!inserted);
    }

    #[tokio::test]
    async fn confirmed_with_consent_inserts_voice_sample() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_with_consent(&db.pool, "Alice", true).await;
        insert_confirmed_speaker(&db.pool, &call.id, "S1", &alice).await;

        let inserted = maybe_backfill_voice_sample(&db.pool, &call.id, "S1", &[1, 2, 3, 4], None)
            .await
            .unwrap();
        assert!(inserted);
        let samples: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM voice_samples WHERE contact_id = ?1")
                .bind(&alice)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(samples, 1);
    }

    // [P4] Slice metadata persisted при наличии rawStt + ≥1.5s segment.
    #[tokio::test]
    async fn persists_slice_metadata_when_raw_stt_provided() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_with_consent(&db.pool, "Alice", true).await;
        insert_confirmed_speaker(&db.pool, &call.id, "owner", &alice).await;

        let raw_stt = serde_json::json!({
            "merged": [
                { "speakerTag": "owner", "start": 0.0, "end": 2.0, "text": "hi" },
                { "speakerTag": "owner", "start": 3.0, "end": 8.0, "text": "longer one" },
            ]
        })
        .to_string();

        let inserted =
            maybe_backfill_voice_sample(&db.pool, &call.id, "owner", &[1, 2, 3, 4], Some(&raw_stt))
                .await
                .unwrap();
        assert!(inserted);

        let (start, end, track): (Option<f64>, Option<f64>, Option<String>) = sqlx::query_as(
            "SELECT start_sec, end_sec, track_kind FROM voice_samples WHERE contact_id = ?1",
        )
        .bind(&alice)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(start, Some(3.0));
        assert_eq!(end, Some(8.0));
        assert_eq!(track.as_deref(), Some("mic"));
    }

    // [P4] None rawStt → INSERT с NULL'ями (legacy-compat fallback).
    #[tokio::test]
    async fn null_slice_metadata_when_no_raw_stt() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_with_consent(&db.pool, "Alice", true).await;
        insert_confirmed_speaker(&db.pool, &call.id, "S1", &alice).await;

        let inserted = maybe_backfill_voice_sample(&db.pool, &call.id, "S1", &[1, 2, 3, 4], None)
            .await
            .unwrap();
        assert!(inserted);

        let (start, end, track): (Option<f64>, Option<f64>, Option<String>) = sqlx::query_as(
            "SELECT start_sec, end_sec, track_kind FROM voice_samples WHERE contact_id = ?1",
        )
        .bind(&alice)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(start.is_none() && end.is_none() && track.is_none());
    }

    #[tokio::test]
    async fn idempotent_on_repeat_call() {
        // Reprocess: backfill вызывается дважды, остаётся ровно 1 sample.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_with_consent(&db.pool, "Alice", true).await;
        insert_confirmed_speaker(&db.pool, &call.id, "S1", &alice).await;

        maybe_backfill_voice_sample(&db.pool, &call.id, "S1", &[1, 2, 3, 4], None)
            .await
            .unwrap();
        maybe_backfill_voice_sample(&db.pool, &call.id, "S1", &[5, 6, 7, 8], None)
            .await
            .unwrap();

        let samples: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM voice_samples WHERE contact_id = ?1 AND source_call = ?2",
        )
        .bind(&alice)
        .bind(&call.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(samples, 1, "повторный backfill не плодит дубли");
    }
}
