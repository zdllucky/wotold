//! [TD-43] Suggestion-слой привязки спикеров: запись предложений от
//! matching-пайплайна и авто-привязка высокоуверенных совпадений.
//!
//! Выделен из `calls/speakers.rs` — тот перевалил за лимит когезии в 800
//! строк, и добавить в него даже двухстрочный фикс стало нельзя (инженерное
//! правило 8). Граница модуля проходит по домену: здесь всё, что пишет
//! `suggestion_*` и читает их для auto-bind; в `speakers.rs` остаётся
//! confirmation-flow (confirm/unbind/list/prune) и owner-привязка.

use sqlx::SqlitePool;

use crate::AppError;

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
    //
    // [TD-16] `AND confirmed = 0` — та же клауза, что в
    // `prune_call_speakers_not_in`. Без неё повторный прогон сносил строки с
    // `confirmed = 1` вместе с contact_id, cluster_embedding и auto_bound_at,
    // то есть уничтожал подтверждённую пользователем привязку (священное
    // действие по R2). Сейчас функция не подключена к пайплайну
    // (`identify_speakers` никем не зовётся), так что это предохранитель на
    // момент wire-up #26, а не активный баг.
    sqlx::query(
        "DELETE FROM call_speakers
         WHERE call_id = ?1 AND speaker_tag != 'owner' AND confirmed = 0",
    )
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

/// [V7] Авто-привязка спикеров с высокой уверенностью (suggestion_score >=
/// `threshold`). Выполняется после matching pipeline, ТОЛЬКО когда юзер
/// явно включил toggle в Settings.
///
/// Guardrails (R2 паспорта — opt-in is the only legitimate path):
///   1. Speaker НЕ должен быть уже привязан (confirmed=0 AND contact_id NULL)
///   2. speaker_tag != 'owner' (owner всегда привязан тривиально)
///   3. suggestion_score >= threshold (0.90 | 0.95 | 0.98)
///   4. Контакт-кандидат имеет consent_voice='true'
///   5. Контакт имеет ≥ 2 voice_samples (один sample = слабая база, может
///      ошибаться при больном голосе или родственниках)
///
/// Возвращает количество авто-привязанных speaker'ов — pipeline эмитит
/// event `call:auto_bound` с этим числом, UI рендерит «↩ отменить» баннер.
pub async fn auto_bind_high_confidence_speakers(
    pool: &SqlitePool,
    call_id: &str,
    threshold: f64,
) -> Result<u64, AppError> {
    // Минимум 2 sample'а на контакт — отдельный CTE-сабквери внутри UPDATE
    // (SQLite ≥3.8.3 поддерживает correlated subquery в SET, проверено в
    // sqlx 0.8 + bundled libsqlite ≥3.40). consent_voice проверяется через
    // json_extract атрибутов — формат '{"consent_voice":"true"}' (см.
    // confirm_call_speaker).
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE call_speakers
         SET contact_id = suggestion_contact_id,
             confirmed = 1,
             auto_bound_at = ?1
         WHERE call_id = ?2
           AND speaker_tag != 'owner'
           AND confirmed = 0
           AND contact_id IS NULL
           AND suggestion_contact_id IS NOT NULL
           AND suggestion_score IS NOT NULL
           AND suggestion_score >= ?3
           AND EXISTS (
             SELECT 1 FROM contacts c
             WHERE c.id = call_speakers.suggestion_contact_id
               AND json_extract(COALESCE(c.attributes, '{}'), '$.consent_voice') = 'true'
           )
           AND (
             SELECT COUNT(*) FROM voice_samples vs
             WHERE vs.contact_id = call_speakers.suggestion_contact_id
           ) >= 2",
    )
    .bind(&now)
    .bind(call_id)
    .bind(threshold)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// [B3.4] Persist suggestion (contact_id, score, source) к существующему
/// call_speaker — заменяет старую при перезапуске matching pipeline.
pub async fn set_call_speaker_suggestion(
    pool: &SqlitePool,
    call_id: &str,
    speaker_tag: &str,
    suggestion_contact_id: Option<&str>,
    suggestion_score: Option<f64>,
    suggestion_source: Option<&str>,
) -> Result<(), AppError> {
    let res = sqlx::query(
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
    // [TD-43] Строки `(call_id, speaker_tag)` может не быть — тогда UPDATE
    // молча no-op и подсказка теряется. Близнец `set_call_speaker_cluster`
    // болел тем же (правило 2: twin parity). Тихая деградация запрещена
    // правилом 3 — пусть хотя бы видна в логе.
    if res.rows_affected() == 0 {
        log::warn!(
            "set_call_speaker_suggestion: нет строки call_speakers для call \
             {call_id} speaker {speaker_tag} — подсказка контакта потеряна"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::calls::{
        confirm_call_speaker, insert_recording, list_call_speakers, unbind_call_speaker,
    };
    use crate::db::test_support::{fresh_db, insert_contact_row, insert_speaker_row};

    /// Helper: создать контакт с opt-in consent + N voice samples.
    async fn contact_with_samples(
        pool: &sqlx::SqlitePool,
        name: &str,
        sample_count: usize,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES (?1, ?2, 0, '{\"consent_voice\":\"true\"}', ?3, ?3)",
        )
        .bind(&id)
        .bind(name)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
        for i in 0..sample_count {
            sqlx::query(
                "INSERT INTO voice_samples
                   (id, contact_id, embedding, source_call, quality, created_at)
                 VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            )
            .bind(format!("vs-{name}-{i}"))
            .bind(&id)
            .bind(vec![0u8; 4])
            .bind(0.9)
            .bind(&now)
            .execute(pool)
            .await
            .unwrap();
        }
        id
    }

    async fn insert_speaker_with_suggestion(
        pool: &sqlx::SqlitePool,
        call_id: &str,
        speaker_tag: &str,
        suggestion_contact_id: &str,
        score: f64,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO call_speakers
               (id, call_id, speaker_tag, suggestion_contact_id, suggestion_score, suggestion_source, confirmed)
             VALUES (?1, ?2, ?3, ?4, ?5, 'embedding', 0)",
        )
        .bind(&id)
        .bind(call_id)
        .bind(speaker_tag)
        .bind(suggestion_contact_id)
        .bind(score)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    // ============================================================
    // [TD-16] Подтверждённое пользователем неприкосновенно
    // ============================================================

    #[tokio::test]
    async fn insert_speaker_suggestions_preserves_confirmed_non_owner() {
        // Регрессия TD-16: DELETE сносил строки с confirmed=1 вместе с
        // contact_id/cluster_embedding/auto_bound_at. Owner-строка была
        // защищена отдельно (тест ниже), а подтверждённая пользователем
        // привязка обычного спикера — нет. Это мина под R2 на момент, когда
        // identify_speakers подключат к пайплайну (#26).
        use crate::merge_signals::{MergedSuggestion, SuggestionSource};
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_row(&db.pool, "Alice").await;
        let bob = insert_contact_row(&db.pool, "Bob").await;

        // Пользователь вручную подтвердил S1 → Alice.
        let s1 = insert_speaker_row(&db.pool, &call.id, "S1", None).await;
        confirm_call_speaker(&db.pool, &s1, &alice).await.unwrap();

        // Повторный прогон identify_speakers предлагает S2 → Bob.
        let suggestions = vec![MergedSuggestion {
            speaker_tag: "S2".into(),
            contact_id: bob.clone(),
            display_name: "Bob".into(),
            score: 0.9,
            source: SuggestionSource::Embedding,
        }];
        insert_speaker_suggestions(&db.pool, &call.id, &suggestions)
            .await
            .unwrap();

        let speakers = list_call_speakers(&db.pool, &call.id).await.unwrap();
        let s1_row = speakers
            .iter()
            .find(|s| s.speaker_tag == "S1")
            .expect("подтверждённая привязка S1 обязана пережить повторный прогон");
        assert!(s1_row.confirmed, "confirmed-флаг не должен сбрасываться");
        assert_eq!(
            s1_row.contact_id.as_deref(),
            Some(alice.as_str()),
            "выбор пользователя не должен теряться"
        );
        assert!(
            speakers.iter().any(|s| s.speaker_tag == "S2"),
            "новое предложение при этом добавляется"
        );
    }

    #[tokio::test]
    async fn insert_speaker_suggestions_preserves_owner_row() {
        // M3.7: повторный run identify_speakers НЕ должен сносить owner binding.
        use crate::merge_signals::{MergedSuggestion, SuggestionSource};
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let owner = crate::db::ensure_owner_contact(&db.pool).await.unwrap();
        let alice = insert_contact_row(&db.pool, "Alice").await;

        crate::db::calls::auto_bind_owner_speaker(&db.pool, &call.id, &owner.id, "owner")
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

    // ============================================================
    // [V7] auto_bind_high_confidence_speakers — opt-in auto bind
    // ============================================================

    #[tokio::test]
    async fn auto_bind_binds_high_score_speaker_with_consent_and_samples() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = contact_with_samples(&db.pool, "Alice", 2).await;
        let sid = insert_speaker_with_suggestion(&db.pool, &call.id, "S1", &alice, 0.97).await;

        let n = auto_bind_high_confidence_speakers(&db.pool, &call.id, 0.95)
            .await
            .unwrap();
        assert_eq!(n, 1);

        let speakers = list_call_speakers(&db.pool, &call.id).await.unwrap();
        let s1 = speakers.iter().find(|s| s.id == sid).unwrap();
        assert!(s1.confirmed);
        assert_eq!(s1.contact_id.as_deref(), Some(alice.as_str()));
        assert!(s1.auto_bound_at.is_some(), "auto_bound_at должен быть set");
    }

    #[tokio::test]
    async fn auto_bind_skips_when_score_below_threshold() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = contact_with_samples(&db.pool, "Alice", 3).await;
        insert_speaker_with_suggestion(&db.pool, &call.id, "S1", &alice, 0.93).await;

        let n = auto_bind_high_confidence_speakers(&db.pool, &call.id, 0.95)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn auto_bind_skips_when_only_one_sample() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        // 1 sample — недостаточно для confident match.
        let alice = contact_with_samples(&db.pool, "Alice", 1).await;
        insert_speaker_with_suggestion(&db.pool, &call.id, "S1", &alice, 0.99).await;

        let n = auto_bind_high_confidence_speakers(&db.pool, &call.id, 0.95)
            .await
            .unwrap();
        assert_eq!(n, 0, "consenting но <2 samples — не привязываем");
    }

    #[tokio::test]
    async fn auto_bind_skips_when_no_consent() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        // Контакт без consent_voice.
        let bob = insert_contact_row(&db.pool, "Bob").await;
        sqlx::query(
            "INSERT INTO voice_samples
               (id, contact_id, embedding, source_call, quality, created_at)
             VALUES ('vs-x', ?1, ?2, NULL, 0.9, ?3)",
        )
        .bind(&bob)
        .bind(vec![0u8; 4])
        .bind("2026-05-20T00:00:00Z")
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO voice_samples
               (id, contact_id, embedding, source_call, quality, created_at)
             VALUES ('vs-y', ?1, ?2, NULL, 0.9, ?3)",
        )
        .bind(&bob)
        .bind(vec![0u8; 4])
        .bind("2026-05-20T00:00:00Z")
        .execute(&db.pool)
        .await
        .unwrap();
        insert_speaker_with_suggestion(&db.pool, &call.id, "S1", &bob, 0.99).await;

        let n = auto_bind_high_confidence_speakers(&db.pool, &call.id, 0.95)
            .await
            .unwrap();
        assert_eq!(n, 0, "без consent_voice='true' — никакой авто-привязки");
    }

    #[tokio::test]
    async fn auto_bind_skips_already_confirmed_speaker() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = contact_with_samples(&db.pool, "Alice", 2).await;
        let bob = contact_with_samples(&db.pool, "Bob", 2).await;
        let sid = insert_speaker_with_suggestion(&db.pool, &call.id, "S1", &alice, 0.99).await;
        // Юзер УЖЕ вручную привязал к Bob — auto-bind не должен перезаписать.
        confirm_call_speaker(&db.pool, &sid, &bob).await.unwrap();

        let n = auto_bind_high_confidence_speakers(&db.pool, &call.id, 0.95)
            .await
            .unwrap();
        assert_eq!(n, 0, "existing binding не перезаписывается");

        let speakers = list_call_speakers(&db.pool, &call.id).await.unwrap();
        let s1 = speakers.iter().find(|s| s.id == sid).unwrap();
        assert_eq!(s1.contact_id.as_deref(), Some(bob.as_str()));
        assert!(s1.auto_bound_at.is_none());
    }

    #[tokio::test]
    async fn auto_bind_skips_owner_tag() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = contact_with_samples(&db.pool, "Alice", 2).await;
        // owner row с suggestion на Alice — must not auto-rebind owner.
        sqlx::query(
            "INSERT INTO call_speakers
               (id, call_id, speaker_tag, suggestion_contact_id, suggestion_score, suggestion_source, confirmed)
             VALUES ('owner-row', ?1, 'owner', ?2, 0.99, 'embedding', 0)",
        )
        .bind(&call.id)
        .bind(&alice)
        .execute(&db.pool)
        .await
        .unwrap();

        let n = auto_bind_high_confidence_speakers(&db.pool, &call.id, 0.95)
            .await
            .unwrap();
        assert_eq!(n, 0, "owner tag всегда исключён из auto-bind");
    }

    #[tokio::test]
    async fn unbind_clears_auto_bound_at() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = contact_with_samples(&db.pool, "Alice", 2).await;
        let sid = insert_speaker_with_suggestion(&db.pool, &call.id, "S1", &alice, 0.99).await;
        auto_bind_high_confidence_speakers(&db.pool, &call.id, 0.95)
            .await
            .unwrap();

        unbind_call_speaker(&db.pool, &sid).await.unwrap();
        let speakers = list_call_speakers(&db.pool, &call.id).await.unwrap();
        let s1 = speakers.iter().find(|s| s.id == sid).unwrap();
        assert!(!s1.confirmed);
        assert!(s1.contact_id.is_none());
        assert!(
            s1.auto_bound_at.is_none(),
            "auto_bound_at очищается на undo"
        );
    }

    // ============================================================
    // [TD-43] Молчаливый no-op при отсутствующей строке
    // ============================================================

    #[tokio::test]
    async fn set_suggestion_on_missing_row_does_not_error() {
        // Регрессия TD-43: UPDATE без строки возвращал Ok и молчал. Теперь
        // контракт сохранён (Ok — вызывающий пайплайн деградирует мягко), но
        // потеря видна в логе. Тест фиксирует, что no-op не превратился в
        // Err и не запаниковал.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_row(&db.pool, "Alice").await;

        let res = set_call_speaker_suggestion(
            &db.pool,
            &call.id,
            "S-нет-такого",
            Some(&alice),
            Some(0.9),
            Some("embedding"),
        )
        .await;
        assert!(res.is_ok(), "контракт: мягкая деградация, не Err");

        let rows = list_call_speakers(&db.pool, &call.id).await.unwrap();
        assert!(rows.is_empty(), "ничего не должно быть создано");
    }

    #[tokio::test]
    async fn set_suggestion_on_existing_row_writes_all_fields() {
        // Позитивная половина: на существующей строке UPDATE реально пишет.
        // Без неё warn-ветка могла бы «пройти» тест на всегда-нулевом
        // rows_affected.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        let alice = insert_contact_row(&db.pool, "Alice").await;
        insert_speaker_row(&db.pool, &call.id, "S1", None).await;

        set_call_speaker_suggestion(
            &db.pool,
            &call.id,
            "S1",
            Some(&alice),
            Some(0.91),
            Some("llm"),
        )
        .await
        .unwrap();

        let rows = list_call_speakers(&db.pool, &call.id).await.unwrap();
        let s1 = rows.iter().find(|s| s.speaker_tag == "S1").unwrap();
        assert_eq!(s1.suggestion_contact_id.as_deref(), Some(alice.as_str()));
        assert_eq!(s1.suggestion_score, Some(0.91));
    }
}
