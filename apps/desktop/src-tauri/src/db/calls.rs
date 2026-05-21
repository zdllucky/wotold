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
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, created_at, updated_at
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
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, created_at, updated_at
         FROM calls
         ORDER BY started_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// M3.4 (#25): сохранить предложенные привязки спикеров (suggestion_*).
/// confirmed=0 — пользователь подтверждает через UI (M3.5 / #26).
/// Перезаписываем предложения при повторном run (idempotent через DELETE+INSERT).
pub async fn insert_speaker_suggestions(
    pool: &SqlitePool,
    call_id: &str,
    suggestions: &[crate::merge_signals::MergedSuggestion],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    // M3.7: НЕ удаляем owner row (он создан через auto_bind_owner_speaker и
    // тривиально привязан к пользователю — identify_speakers не работает с
    // owner_tag, см. identify.rs).
    sqlx::query("DELETE FROM call_speakers WHERE call_id = ?1 AND speaker_tag != 'owner'")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    for s in suggestions {
        let source = match s.source {
            crate::merge_signals::SuggestionSource::Embedding => "embedding",
            crate::merge_signals::SuggestionSource::Llm => "llm",
            crate::merge_signals::SuggestionSource::Both => "both",
        };
        sqlx::query(
            "INSERT INTO call_speakers (id, call_id, speaker_tag, suggestion_contact_id, suggestion_score, suggestion_source, confirmed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(call_id)
        .bind(&s.speaker_tag)
        .bind(&s.contact_id)
        .bind(s.score)
        .bind(source)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// M3.5 (#26): представление call_speaker для UI — speaker_tag + текущая
/// привязка (contact_id, confirmed) + suggestion если есть. Для рендера
/// confirmation flow в CallDetailPage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSpeakerView {
    pub id: String,
    pub call_id: String,
    pub speaker_tag: String,
    pub contact_id: Option<String>,
    pub contact_display_name: Option<String>,
    pub suggestion_contact_id: Option<String>,
    pub suggestion_contact_display_name: Option<String>,
    pub suggestion_score: Option<f64>,
    pub suggestion_source: Option<String>,
    pub confirmed: bool,
}

/// Возвращает спикеров звонка с join'ом display_name по contact_id +
/// suggestion_contact_id (LEFT JOIN — отсутствующий контакт = NULL).
pub async fn list_call_speakers(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Vec<CallSpeakerView>, AppError> {
    let rows = sqlx::query(
        "SELECT
            cs.id, cs.call_id, cs.speaker_tag, cs.contact_id,
            c1.display_name AS contact_display_name,
            cs.suggestion_contact_id,
            c2.display_name AS suggestion_contact_display_name,
            cs.suggestion_score, cs.suggestion_source, cs.confirmed
         FROM call_speakers cs
         LEFT JOIN contacts c1 ON c1.id = cs.contact_id
         LEFT JOIN contacts c2 ON c2.id = cs.suggestion_contact_id
         WHERE cs.call_id = ?1
         ORDER BY cs.speaker_tag",
    )
    .bind(call_id)
    .fetch_all(pool)
    .await?;

    use sqlx::Row;
    Ok(rows
        .into_iter()
        .map(|r| CallSpeakerView {
            id: r.get("id"),
            call_id: r.get("call_id"),
            speaker_tag: r.get("speaker_tag"),
            contact_id: r.get("contact_id"),
            contact_display_name: r.get("contact_display_name"),
            suggestion_contact_id: r.get("suggestion_contact_id"),
            suggestion_contact_display_name: r.get("suggestion_contact_display_name"),
            suggestion_score: r.get("suggestion_score"),
            suggestion_source: r.get("suggestion_source"),
            confirmed: r.get::<i64, _>("confirmed") == 1,
        })
        .collect())
}

/// [B11] M7.4: добавить placeholder rows в `call_speakers` для каждого спикера
/// из транскрипта (если их там ещё нет). Это делает всех спикеров видимыми
/// в `SpeakersSection`, в т.ч. анонимных без suggestion от identify_speakers.
/// Идемпотент: для существующих speaker_tag ничего не делает.
pub async fn ensure_call_speakers_present(
    pool: &SqlitePool,
    call_id: &str,
    speaker_tags: &[String],
) -> Result<(), AppError> {
    if speaker_tags.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    for tag in speaker_tags {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM call_speakers WHERE call_id = ?1 AND speaker_tag = ?2 LIMIT 1",
        )
        .bind(call_id)
        .bind(tag)
        .fetch_optional(&mut *tx)
        .await?;
        if exists.is_some() {
            continue;
        }
        sqlx::query(
            "INSERT INTO call_speakers (id, call_id, speaker_tag, confirmed)
             VALUES (?1, ?2, ?3, 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(call_id)
        .bind(tag)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// M3.7 паспорта: mic-дорожка по определению принадлежит владельцу устройства,
/// никакой биометрии не требуется. Pipeline вызывает этот метод после
/// merge_tracks чтобы сразу записать speaker_tag="owner" confirmed=1 с
/// привязкой к owner контакту. Идемпотент: DELETE+INSERT.
///
/// Это НЕ нарушает R2 (никакой автопривязки) — owner это сам пользователь,
/// привязка к собственному контакту тривиальна и не требует confirm.
pub async fn auto_bind_owner_speaker(
    pool: &SqlitePool,
    call_id: &str,
    owner_contact_id: &str,
    owner_tag: &str,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM call_speakers WHERE call_id = ?1 AND speaker_tag = ?2")
        .bind(call_id)
        .bind(owner_tag)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO call_speakers (id, call_id, speaker_tag, contact_id, confirmed)
         VALUES (?1, ?2, ?3, ?4, 1)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(call_id)
    .bind(owner_tag)
    .bind(owner_contact_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// M3.5 (#26): R2 паспорта — финальная привязка спикер↔контакт ТОЛЬКО через
/// явное подтверждение пользователя. Этот метод вызывается из Tauri-команды,
/// никогда из pipeline. confirmed = 1.
///
/// [B3.5] Если у contact есть `consent_voice='true'` И у call_speaker есть
/// cluster_embedding — INSERT в voice_samples (source_call=call_id,
/// embedding=cluster, quality=suggestion_score|1.0). Это автоматически
/// накапливает образцы голоса для будущего matching между звонками.
pub async fn confirm_call_speaker(
    pool: &SqlitePool,
    call_speaker_id: &str,
    contact_id: &str,
) -> Result<(), AppError> {
    // Контакт должен существовать. FK contacts(id) уже это гарантирует, но
    // вернём явный AppError а не SQL-ошибку для UX.
    let contact_row: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, attributes, COALESCE(NULL, '') FROM contacts WHERE id = ?1")
            .bind(contact_id)
            .fetch_optional(pool)
            .await?;
    let Some((_, attributes_json, _)) = contact_row else {
        return Err(AppError::Other(format!("contact {contact_id} not found")));
    };

    // Перед UPDATE достаём cluster_embedding и call_id для возможного
    // voice_sample insert.
    let speaker_meta: Option<(String, Option<Vec<u8>>, Option<f64>)> = sqlx::query_as(
        "SELECT call_id, cluster_embedding, suggestion_score
         FROM call_speakers WHERE id = ?1",
    )
    .bind(call_speaker_id)
    .fetch_optional(pool)
    .await?;

    let mut tx = pool.begin().await?;
    let updated =
        sqlx::query("UPDATE call_speakers SET contact_id = ?1, confirmed = 1 WHERE id = ?2")
            .bind(contact_id)
            .bind(call_speaker_id)
            .execute(&mut *tx)
            .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::Other(format!(
            "call_speaker {call_speaker_id} not found"
        )));
    }

    // C2 (#40): копируем cluster в voice_samples только если контакт opt-in
    // дал consent_voice='true'.
    let consent_voice = serde_json::from_str::<serde_json::Value>(&attributes_json)
        .ok()
        .and_then(|v| {
            v.get("consent_voice")
                .and_then(|c| c.as_str())
                .map(|s| s == "true")
        })
        .unwrap_or(false);

    if consent_voice {
        if let Some((call_id, Some(embedding_blob), score)) = speaker_meta {
            // [B3.5] embedding должен быть non-empty BLOB (256 × 4 = 1024 байта
            // для current EMBEDDING_DIM). Принимаем any BLOB → matching сам
            // обработает invalid размер через safe cosine_similarity = 0.0.
            if !embedding_blob.is_empty() {
                let voice_sample_id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();
                let quality = score.unwrap_or(1.0);
                sqlx::query(
                    "INSERT INTO voice_samples
                       (id, contact_id, embedding, source_call, quality, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .bind(&voice_sample_id)
                .bind(contact_id)
                .bind(&embedding_blob)
                .bind(&call_id)
                .bind(quality)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                log::info!(
                    "voice_sample saved: contact={contact_id} call={call_id} quality={quality:.3}"
                );
            }
        }
    }

    tx.commit().await?;
    Ok(())
}

/// [B3.1] Persist извлечённого pipeline'ом cluster embedding в call_speakers.
/// Безопасно для NULL — embedding=None очистит поле. Сохраняем little-endian
/// f32 blob (см. embedding_to_bytes в embeddings.rs).
#[allow(dead_code)] // wired в B3.3
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

/// [B3.4] Persist suggestion (contact_id, score, source) к существующему
/// call_speaker — заменяет старую при перезапуске matching pipeline.
#[allow(dead_code)] // wired в B3.4
pub async fn set_call_speaker_suggestion(
    pool: &SqlitePool,
    call_id: &str,
    speaker_tag: &str,
    suggestion_contact_id: Option<&str>,
    suggestion_score: Option<f64>,
    suggestion_source: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE call_speakers
         SET suggestion_contact_id = ?1,
             suggestion_score = ?2,
             suggestion_source = ?3
         WHERE call_id = ?4 AND speaker_tag = ?5",
    )
    .bind(suggestion_contact_id)
    .bind(suggestion_score)
    .bind(suggestion_source)
    .bind(call_id)
    .bind(speaker_tag)
    .execute(pool)
    .await?;
    Ok(())
}

/// Откатить привязку спикера: contact_id = NULL, confirmed = 0. Suggestion
/// остаётся как был — пользователь может изменить решение позже.
pub async fn unbind_call_speaker(pool: &SqlitePool, call_speaker_id: &str) -> Result<(), AppError> {
    let updated =
        sqlx::query("UPDATE call_speakers SET contact_id = NULL, confirmed = 0 WHERE id = ?1")
            .bind(call_speaker_id)
            .execute(pool)
            .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::Other(format!(
            "call_speaker {call_speaker_id} not found"
        )));
    }
    Ok(())
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
    // M3.5 (#26) speaker confirmation flow
    // ============================================================

    async fn insert_speaker_row(
        pool: &sqlx::SqlitePool,
        call_id: &str,
        speaker_tag: &str,
        suggestion_contact_id: Option<&str>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO call_speakers (id, call_id, speaker_tag, suggestion_contact_id, suggestion_score, suggestion_source, confirmed)
             VALUES (?1, ?2, ?3, ?4, 0.85, 'embedding', 0)",
        )
        .bind(&id)
        .bind(call_id)
        .bind(speaker_tag)
        .bind(suggestion_contact_id)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn insert_contact_row(pool: &sqlx::SqlitePool, name: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES (?1, ?2, 0, '{}', ?3, ?3)",
        )
        .bind(&id)
        .bind(name)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn list_call_speakers_joins_contact_names() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_row(&db.pool, "Alice").await;
        let bob = insert_contact_row(&db.pool, "Bob").await;
        insert_speaker_row(&db.pool, &call.id, "S1", Some(&alice)).await;
        let s2_id = insert_speaker_row(&db.pool, &call.id, "S2", Some(&bob)).await;

        // Confirm S2 → contact_display_name заполняется через c1 join.
        confirm_call_speaker(&db.pool, &s2_id, &bob).await.unwrap();

        let speakers = list_call_speakers(&db.pool, &call.id).await.unwrap();
        assert_eq!(speakers.len(), 2);
        let s1 = speakers.iter().find(|s| s.speaker_tag == "S1").unwrap();
        let s2 = speakers.iter().find(|s| s.speaker_tag == "S2").unwrap();
        assert_eq!(s1.suggestion_contact_display_name.as_deref(), Some("Alice"));
        assert_eq!(s1.contact_id, None);
        assert!(!s1.confirmed);
        assert_eq!(s2.contact_display_name.as_deref(), Some("Bob"));
        assert!(s2.confirmed);
    }

    #[tokio::test]
    async fn confirm_then_unbind_clears_binding_but_keeps_suggestion() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_row(&db.pool, "Alice").await;
        let sid = insert_speaker_row(&db.pool, &call.id, "S1", Some(&alice)).await;

        confirm_call_speaker(&db.pool, &sid, &alice).await.unwrap();
        let after_confirm = list_call_speakers(&db.pool, &call.id)
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert!(after_confirm.confirmed);
        assert_eq!(after_confirm.contact_id.as_deref(), Some(alice.as_str()));

        unbind_call_speaker(&db.pool, &sid).await.unwrap();
        let after_unbind = list_call_speakers(&db.pool, &call.id)
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert!(!after_unbind.confirmed);
        assert_eq!(after_unbind.contact_id, None);
        // suggestion остаётся — юзер может передумать.
        assert_eq!(
            after_unbind.suggestion_contact_id.as_deref(),
            Some(alice.as_str())
        );
    }

    #[tokio::test]
    async fn confirm_unknown_contact_errors() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let sid = insert_speaker_row(&db.pool, &call.id, "S1", None).await;

        let err = confirm_call_speaker(&db.pool, &sid, "ghost-contact").await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn confirm_unknown_speaker_errors() {
        let db = fresh_db().await;
        let alice = insert_contact_row(&db.pool, "Alice").await;
        let err = confirm_call_speaker(&db.pool, "ghost-speaker", &alice).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn auto_bind_owner_speaker_writes_confirmed_row() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let owner = crate::db::ensure_owner_contact(&db.pool).await.unwrap();

        auto_bind_owner_speaker(&db.pool, &call.id, &owner.id, "owner")
            .await
            .unwrap();

        let speakers = list_call_speakers(&db.pool, &call.id).await.unwrap();
        assert_eq!(speakers.len(), 1);
        assert_eq!(speakers[0].speaker_tag, "owner");
        assert!(speakers[0].confirmed);
        assert_eq!(speakers[0].contact_id.as_deref(), Some(owner.id.as_str()));
    }

    #[tokio::test]
    async fn insert_speaker_suggestions_preserves_owner_row() {
        // M3.7: повторный run identify_speakers НЕ должен сносить owner binding.
        use crate::merge_signals::{MergedSuggestion, SuggestionSource};
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let owner = crate::db::ensure_owner_contact(&db.pool).await.unwrap();
        let alice = insert_contact_row(&db.pool, "Alice").await;

        auto_bind_owner_speaker(&db.pool, &call.id, &owner.id, "owner")
            .await
            .unwrap();

        let suggestions = vec![MergedSuggestion {
            speaker_tag: "S1".into(),
            contact_id: alice.clone(),
            display_name: "Alice".into(),
            score: 0.85,
            source: SuggestionSource::Embedding,
        }];
        insert_speaker_suggestions(&db.pool, &call.id, &suggestions)
            .await
            .unwrap();

        let speakers = list_call_speakers(&db.pool, &call.id).await.unwrap();
        assert_eq!(speakers.len(), 2);
        assert!(
            speakers
                .iter()
                .any(|s| s.speaker_tag == "owner" && s.confirmed),
            "owner row должен пережить insert_speaker_suggestions"
        );
        assert!(
            speakers.iter().any(
                |s| s.speaker_tag == "S1" && s.suggestion_contact_id.as_deref() == Some(&alice)
            ),
            "новый suggestion должен быть записан"
        );
    }

    #[tokio::test]
    async fn auto_bind_owner_speaker_is_idempotent() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let owner = crate::db::ensure_owner_contact(&db.pool).await.unwrap();

        auto_bind_owner_speaker(&db.pool, &call.id, &owner.id, "owner")
            .await
            .unwrap();
        auto_bind_owner_speaker(&db.pool, &call.id, &owner.id, "owner")
            .await
            .unwrap();
        let speakers = list_call_speakers(&db.pool, &call.id).await.unwrap();
        assert_eq!(speakers.len(), 1, "повторный вызов не создаёт дубль");
    }
}
