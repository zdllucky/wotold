//! Confirmation-flow привязки спикеров: список, ensure/prune, owner-привязка,
//! confirm/unbind. Suggestion-слой (`insert_speaker_suggestions`,
//! `auto_bind_high_confidence_speakers`, `set_call_speaker_suggestion`) живёт
//! в соседнем `suggestions.rs` — вынесен там при TD-43, когда этот файл упёрся
//! в лимит когезии 800 строк.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::AppError;

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
    /// [V7] RFC3339 timestamp если speaker был привязан автоматически
    /// (suggestion_score >= threshold). NULL = ручное подтверждение.
    /// UI рендерит «↩ отменить» баннер первые N секунд после открытия.
    pub auto_bound_at: Option<String>,
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
            cs.suggestion_score, cs.suggestion_source, cs.confirmed,
            cs.auto_bound_at
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
            auto_bound_at: r.get("auto_bound_at"),
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

/// [P-fix9] Удалить устаревшие анонимные строки call_speakers, которых больше
/// нет в текущей расшифровке. Реконсиляция при reprocess: после re-STT набор
/// speaker-тегов меняется, а add-only `ensure_call_speakers_present` оставлял
/// фантомные теги (например speaker:3 от прошлого mic-diar-ON прогона) → UI
/// показывал спикера без сэмпла, кнопка плеера мертва.
///
/// Сохраняем `owner` и **confirmed** строки (не теряем подтверждённые юзером
/// привязки). Удаляем только `confirmed=0` теги, отсутствующие в `present_tags`.
/// Пустой `present_tags` → удалить все non-owner unconfirmed (solo-звонок).
/// Возвращает число удалённых строк.
pub async fn prune_call_speakers_not_in(
    pool: &SqlitePool,
    call_id: &str,
    present_tags: &[String],
) -> Result<u64, AppError> {
    let mut sql = String::from(
        "DELETE FROM call_speakers \
         WHERE call_id = ? AND speaker_tag != 'owner' AND confirmed = 0",
    );
    if !present_tags.is_empty() {
        let placeholders = vec!["?"; present_tags.len()].join(",");
        sql.push_str(&format!(" AND speaker_tag NOT IN ({placeholders})"));
    }
    let mut q = sqlx::query(&sql).bind(call_id);
    for t in present_tags {
        q = q.bind(t);
    }
    let res = q.execute(pool).await.map_err(AppError::from)?;
    Ok(res.rows_affected())
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
    // [TD-16] Оба SELECT'а — ВНУТРИ транзакции. Раньше они читались с `pool`
    // до `begin()`: конкурентный `set_call_speaker_cluster` мог перезаписать
    // cluster между чтением и INSERT'ом в voice_samples, и в биометрию
    // контакта уходил устаревший embedding.
    let mut tx = pool.begin().await?;

    // Контакт должен существовать. FK contacts(id) уже это гарантирует, но
    // вернём явный AppError а не SQL-ошибку для UX.
    let contact_row: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, attributes, COALESCE(NULL, '') FROM contacts WHERE id = ?1")
            .bind(contact_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((_, attributes_json, _)) = contact_row else {
        return Err(AppError::NotFound(format!("contact {contact_id}")));
    };

    // Перед UPDATE достаём cluster_embedding и call_id для возможного
    // voice_sample insert.
    let speaker_meta: Option<(String, Option<Vec<u8>>, Option<f64>)> = sqlx::query_as(
        "SELECT call_id, cluster_embedding, suggestion_score
         FROM call_speakers WHERE id = ?1",
    )
    .bind(call_speaker_id)
    .fetch_optional(&mut *tx)
    .await?;

    // [TD-16] `auto_bound_at = NULL`: подтверждение руками перестаёт быть
    // авто-привязкой. Без сброса UI продолжал показывать баннер «↩ отменить»
    // для того, что пользователь выбрал сам.
    let updated = sqlx::query(
        "UPDATE call_speakers
         SET contact_id = ?1, confirmed = 1, auto_bound_at = NULL
         WHERE id = ?2",
    )
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
                // [B3.8] N-cap rotation: оставляем top-N по quality DESC.
                crate::db::voice_samples::evict_old_voice_samples(&mut *tx, contact_id).await?;
            }
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Откатить привязку спикера: contact_id = NULL, confirmed = 0. Suggestion
/// остаётся как был — пользователь может изменить решение позже.
/// [V7] auto_bound_at тоже очищается — после undo'а это снова pending speaker
/// без auto-bound provenance (если юзер вручную подтвердит позже —
/// auto_bound_at останется NULL, что и нужно).
pub async fn unbind_call_speaker(pool: &SqlitePool, call_speaker_id: &str) -> Result<(), AppError> {
    let updated = sqlx::query(
        "UPDATE call_speakers
         SET contact_id = NULL,
             confirmed = 0,
             auto_bound_at = NULL
         WHERE id = ?1",
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::calls::insert_recording;
    use crate::db::test_support::{fresh_db, insert_contact_row, insert_speaker_row};

    // ============================================================
    // M3.5 (#26) speaker confirmation flow
    // ============================================================

    #[tokio::test]
    async fn prune_removes_absent_unconfirmed_keeps_owner_and_confirmed() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let owner = insert_contact_row(&db.pool, "Me").await;
        auto_bind_owner_speaker(&db.pool, &call.id, &owner, "owner")
            .await
            .unwrap();
        insert_speaker_row(&db.pool, &call.id, "speaker:0", None).await; // present
        insert_speaker_row(&db.pool, &call.id, "speaker:3", None).await; // stale → prune
        let s9 = insert_speaker_row(&db.pool, &call.id, "speaker:9", None).await; // absent but confirmed
        confirm_call_speaker(&db.pool, &s9, &owner).await.unwrap();

        let pruned = prune_call_speakers_not_in(&db.pool, &call.id, &["speaker:0".to_string()])
            .await
            .unwrap();
        assert_eq!(pruned, 1, "только speaker:3 (absent + unconfirmed)");

        let tags: Vec<String> = list_call_speakers(&db.pool, &call.id)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.speaker_tag)
            .collect();
        assert!(tags.contains(&"owner".to_string()));
        assert!(tags.contains(&"speaker:0".to_string()));
        assert!(
            tags.contains(&"speaker:9".to_string()),
            "confirmed сохраняется"
        );
        assert!(!tags.contains(&"speaker:3".to_string()), "stale удалён");
    }

    #[tokio::test]
    async fn prune_empty_present_removes_all_unconfirmed_nonowner() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let owner = insert_contact_row(&db.pool, "Me").await;
        auto_bind_owner_speaker(&db.pool, &call.id, &owner, "owner")
            .await
            .unwrap();
        insert_speaker_row(&db.pool, &call.id, "speaker:0", None).await;
        insert_speaker_row(&db.pool, &call.id, "speaker:1", None).await;
        let n = prune_call_speakers_not_in(&db.pool, &call.id, &[])
            .await
            .unwrap();
        assert_eq!(n, 2);
        let tags: Vec<String> = list_call_speakers(&db.pool, &call.id)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.speaker_tag)
            .collect();
        assert_eq!(tags, vec!["owner".to_string()]);
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
    async fn confirm_call_speaker_clears_auto_bound_at() {
        // [TD-16] Ручное подтверждение перестаёт быть авто-привязкой, иначе UI
        // показывает баннер «↩ отменить» для того, что выбрал сам юзер.
        // Комплементарен `unbind_clears_auto_bound_at`.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_row(&db.pool, "Alice").await;
        let sid = insert_speaker_row(&db.pool, &call.id, "S1", None).await;

        // Имитируем авто-привязку: проставляем auto_bound_at.
        sqlx::query("UPDATE call_speakers SET auto_bound_at = ?1 WHERE id = ?2")
            .bind("2026-07-24T00:00:00Z")
            .bind(&sid)
            .execute(&db.pool)
            .await
            .unwrap();

        confirm_call_speaker(&db.pool, &sid, &alice).await.unwrap();

        let auto_bound: Option<String> =
            sqlx::query_scalar("SELECT auto_bound_at FROM call_speakers WHERE id = ?1")
                .bind(&sid)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert!(
            auto_bound.is_none(),
            "после ручного confirm auto_bound_at обязан быть NULL, получили {auto_bound:?}"
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
