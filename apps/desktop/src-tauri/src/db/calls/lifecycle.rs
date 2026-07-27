use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use crate::AppError;

use super::pause::resume_call;

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
    /// [V6.2] Pipeline progress fields для async-states UI. NULL когда звонок
    /// recording / ready / failed / шаг не начался — UI рендерит ProgressRail
    /// только при `status='processing' && pipeline_step IS NOT NULL`.
    pub pipeline_step: Option<i64>,
    pub pipeline_pct: Option<i64>,
    pub pipeline_eta_sec: Option<i64>,
    pub upload_bytes: Option<i64>,
    /// [W2] RFC3339 timestamp когда юзер нажал pause. NULL означает что запись
    /// сейчас не на паузе (recording или уже завершена). Используется только
    /// для recording rows; при finish_recording проставленный paused_at
    /// автоматически сворачивается в paused_total_ms.
    pub paused_at: Option<String>,
    /// [W2] Накопленная длительность пауз в миллисекундах. Pipeline и UI
    /// вычитают это значение из (ended_at - started_at), чтобы получить
    /// фактическое время записи аудио.
    pub paused_total_ms: i64,
    /// [M14 T-02] Тип звонка из 9 enum'ов CallSummaryV2 (sales_discovery,
    /// sales_demo, etc). NULL для legacy schema_version=1 + при call_type=other
    /// LLM. UI рендерит CallTypeBadge только когда non-null + confidence ≥ 0.5.
    pub call_type: Option<String>,
    pub call_type_confidence: Option<f64>,
    /// [M14 T-02] 1 = legacy markdown-only recap; 2 = full CallSummaryV2 с
    /// decisions/open_questions/evidence. Default 1 (migration).
    pub summary_schema_version: Option<i64>,
    /// [M14 T-02] cloud-managed | local-qwen-{1.5b|3b|7b}. NULL для legacy.
    pub summary_engine: Option<String>,
    /// [M14 T-02 / F1] one_shot | refine_chain. (map_reduce/hierarchical
    /// в БД не встречаются — режим раньше хардкодился в one_shot.)
    pub summary_pipeline_mode: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// [P-fix3] Производное (не DB-колонка): движок обработки для UI —
    /// `local` | `cloud_managed` | `cloud_byo` | null. Фронт использует его для
    /// EngineChip + гейта кнопки «Распознать заново» (force-re-STT доступен
    /// только для local). Вычисляется в `with_processing_via` после fetch;
    /// `#[sqlx(default)]` чтобы query_as не требовал колонку.
    #[sqlx(default)]
    pub processing_via: Option<String>,
}

impl Call {
    /// [P-fix3] Вычислить `processing_via` из имеющихся полей. Local-движок →
    /// `local` (по summary_engine `local-*` или provider `local`); BYO-ключи →
    /// `cloud_byo` (path_label `byo`); иначе облачный provider → `cloud_managed`.
    pub fn with_processing_via(mut self) -> Self {
        let eng = self.summary_engine.as_deref().unwrap_or("");
        let prov = self.provider.as_deref().unwrap_or("");
        self.processing_via = if eng.starts_with("local") || prov == "local" {
            Some("local".to_string())
        } else if self.path_label == "byo" {
            Some("cloud_byo".to_string())
        } else if !prov.is_empty() {
            Some("cloud_managed".to_string())
        } else {
            None
        };
        self
    }
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
        pipeline_step: None,
        pipeline_pct: None,
        pipeline_eta_sec: None,
        upload_bytes: None,
        paused_at: None,
        paused_total_ms: 0,
        call_type: None,
        call_type_confidence: None,
        // [M14 T-02] Default 1 (legacy markdown) пока persist_summary_v2 не
        // переведёт row в schema_version=2.
        summary_schema_version: Some(1),
        summary_engine: None,
        summary_pipeline_mode: None,
        created_at: now.clone(),
        updated_at: now,
        processing_via: None,
    })
}

/// Перевести запись из recording → processing с фактической длительностью.
/// processing — потому что после остановки записи дальше идёт STT → matching → recap.
/// Финальный статус ready проставит recap pipeline (#28).
///
/// [W2] `duration_sec` уже учитывает накопленные паузы — caller (audio sidecar)
/// возвращает реальное время аудио. Если user забыл нажать resume и сразу
/// нажал stop, мы сворачиваем lingering paused_at в paused_total_ms и очищаем
/// поле паузы (resume-then-stop семантика).
pub async fn finish_recording(
    pool: &SqlitePool,
    call_id: &str,
    duration_sec: f64,
) -> Result<Call, AppError> {
    // [W2] Если был забытый pause — выполняем неявный resume сейчас, чтобы
    // paused_total_ms остался согласованным и не торчал paused_at у завершённой
    // записи. resume_call идемпотентен для non-paused.
    resume_call(pool, call_id).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let duration_secs_i64 = duration_sec.round() as i64;

    // [TD-17] FSM-гейт по образцу `db::chunks::mark_chunk_*`: легален только
    // переход `recording → processing`. Раньше `WHERE id = ?1` позволял
    // отставшему stop-flow утащить уже `ready`/`failed` звонок обратно в
    // обработку. 0 строк — настоящая ошибка (функция обязана вернуть Call),
    // и без гейта следующий get_call падал с невнятным «disappeared».
    let updated = sqlx::query(
        "UPDATE calls
         SET status = 'processing',
             ended_at = ?2,
             duration_sec = ?3,
             updated_at = ?2
         WHERE id = ?1 AND status = 'recording'",
    )
    .bind(call_id)
    .bind(&now)
    .bind(duration_secs_i64)
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::Other(format!(
            "finish_recording: звонок {call_id} не в статусе 'recording'"
        )));
    }

    get_call(pool, call_id)
        .await?
        .ok_or_else(|| AppError::Other(format!("call {call_id} disappeared")))
}

/// [P5.2] Live duration update во время recording (на каждый `audio:rotated`
/// event sidecar'а, ~раз в 10 мин). Overwrite OK — sidecar duration_sec
/// monotonic. До этого `duration_sec` писалось только на `finish_recording`,
/// что давало stale "1:56" на HomePage для 30+ мин записей.
pub async fn update_call_duration(
    pool: &SqlitePool,
    call_id: &str,
    duration_sec: f64,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE calls SET duration_sec = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(duration_sec.round() as i64)
        .bind(&now)
        .bind(call_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Перевести запись в финальный статус `ready` после успешного pipeline'а.
/// [V6.2] Заодно очищаем pipeline_* поля — звонок больше не "в обработке",
/// UI не должен рендерить ProgressRail.
pub async fn mark_call_ready(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE calls
         SET status = 'ready',
             -- [M13 fix] Успешный pipeline очищает stale failed_reason (иначе
             -- recovered/reprocessed звонок показывал бы ready + старую ошибку).
             failed_reason = NULL,
             pipeline_step = NULL,
             pipeline_pct = NULL,
             pipeline_eta_sec = NULL,
             upload_bytes = NULL,
             updated_at = ?1
         WHERE id = ?2",
    )
    .bind(&now)
    .bind(call_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// [V6.2] Обновить pipeline_step / pct / eta / upload_bytes. Pipeline вызывает
/// перед каждым меняющимся шагом — UI получает live tick через `call:progress`
/// event (см. pipeline::emit_progress). Без транзакции: одна строка, одна
/// колонка, идемпотент при concurrent writers (последний выигрывает).
pub async fn set_call_progress(
    pool: &SqlitePool,
    call_id: &str,
    step: u8,
    pct: u8,
    eta_sec: Option<i64>,
    upload_bytes: Option<i64>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE calls
         SET pipeline_step = ?1,
             pipeline_pct = ?2,
             pipeline_eta_sec = ?3,
             upload_bytes = ?4,
             updated_at = ?5
         WHERE id = ?6",
    )
    .bind(step as i64)
    .bind(pct as i64)
    .bind(eta_sec)
    .bind(upload_bytes)
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
    // [V6.2] Очищаем pipeline_* — звонок больше не processing, UI должен
    // показывать error variant, а не ProgressRail.
    // [TD-17] Гейт: провалить можно только активный звонок. Раньше
    // `WHERE id = ?1` позволял перевести уже `ready` звонок в `failed` и
    // заодно перетереть его `ended_at`.
    //
    // 0 строк — **warn, а не Err**: функция живёт в error-path, четыре
    // callsite'а зовут её через `let _ =`, а два через `?`
    // (pipeline_runner, pipeline::run). Превращать «не смогли записать
    // провал» в новую ошибку поверх исходной нельзя — потеряется первичная
    // причина. Так же требует и формулировка аудита («с warn-логом»).
    let updated = sqlx::query(
        "UPDATE calls
         SET status = 'failed',
             ended_at = ?2,
             failed_reason = ?3,
             pipeline_step = NULL,
             pipeline_pct = NULL,
             pipeline_eta_sec = NULL,
             upload_bytes = NULL,
             updated_at = ?2
         WHERE id = ?1 AND status IN ('recording', 'processing')",
    )
    .bind(call_id)
    .bind(&now)
    .bind(reason)
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        log::warn!(
            "fail_recording: звонок {call_id} не в активном статусе — \
             нелегальный переход проигнорирован (reason: {reason:?})"
        );
    }
    Ok(())
}

pub async fn get_call(pool: &SqlitePool, call_id: &str) -> Result<Option<Call>, AppError> {
    let row: Option<Call> = sqlx::query_as(
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, pipeline_step, pipeline_pct, pipeline_eta_sec, upload_bytes, paused_at, paused_total_ms, call_type, call_type_confidence, summary_schema_version, summary_engine, summary_pipeline_mode, created_at, updated_at
         FROM calls WHERE id = ?1",
    )
    .bind(call_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Call::with_processing_via))
}

/// Все звонки от свежих к старым. FTS-поиск по транскриптам/рекапу
/// подключится в #30 follow-up когда они начнут писаться (#22, #28).
pub async fn list_calls(pool: &SqlitePool) -> Result<Vec<Call>, AppError> {
    let rows: Vec<Call> = sqlx::query_as(
        "SELECT id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, pipeline_step, pipeline_pct, pipeline_eta_sec, upload_bytes, paused_at, paused_total_ms, call_type, call_type_confidence, summary_schema_version, summary_engine, summary_pipeline_mode, created_at, updated_at
         FROM calls
         ORDER BY started_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Call::with_processing_via).collect())
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

    // ============================================================
    // [TD-17] FSM-гейты: нелегальные переходы не проходят
    // ============================================================

    #[tokio::test]
    async fn finish_recording_rejects_non_recording_status() {
        // Регрессия: `WHERE id = ?1` позволял отставшему stop-flow утащить уже
        // готовый звонок обратно в processing.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &call.id, 10.0).await.unwrap();
        mark_call_ready(&db.pool, &call.id).await.unwrap();

        let err = finish_recording(&db.pool, &call.id, 10.0)
            .await
            .expect_err("ready → processing нелегален");
        assert!(
            format!("{err}").contains("не в статусе 'recording'"),
            "внятная ошибка вместо «disappeared», получили: {err}"
        );

        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "ready", "статус не должен был измениться");
    }

    #[tokio::test]
    async fn fail_recording_on_ready_call_is_warn_noop() {
        // Гейт для fail — warn, а не Err: функция живёт в error-path, и новая
        // ошибка поверх исходной потеряла бы первичную причину.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &call.id, 10.0).await.unwrap();
        mark_call_ready(&db.pool, &call.id).await.unwrap();
        let before = get_call(&db.pool, &call.id).await.unwrap().unwrap();

        fail_recording_with_reason(&db.pool, &call.id, Some("поздний фейл"))
            .await
            .expect("нелегальный переход не должен возвращать Err");

        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "ready", "готовый звонок не помечается failed");
        assert!(after.failed_reason.is_none(), "reason не записывается");
        assert_eq!(
            after.ended_at, before.ended_at,
            "ended_at не перетирается — раньше это происходило безусловно"
        );
    }

    #[tokio::test]
    async fn fail_recording_allowed_from_recording_and_processing() {
        // Легальные переходы обязаны продолжать работать.
        let db = fresh_db().await;

        let a = insert_recording(&db.pool, "managed").await.unwrap();
        fail_recording_with_reason(&db.pool, &a.id, Some("из recording"))
            .await
            .unwrap();
        assert_eq!(
            get_call(&db.pool, &a.id).await.unwrap().unwrap().status,
            "failed"
        );

        let b = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &b.id, 5.0).await.unwrap();
        fail_recording_with_reason(&db.pool, &b.id, Some("из processing"))
            .await
            .unwrap();
        assert_eq!(
            get_call(&db.pool, &b.id).await.unwrap().unwrap().status,
            "failed"
        );
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
    async fn list_calls_orders_by_started_desc() {
        let db = fresh_db().await;
        let first = insert_recording(&db.pool, "managed").await.unwrap();
        let second = insert_recording(&db.pool, "managed").await.unwrap();
        // [TD-32] Раньше здесь спали 1100 мс — ждали, пока сменится секунда в
        // rfc3339. Полторы секунды настенного времени на один assert, и на
        // нагруженном раннере это всё равно не гарантия. Порядок задаём
        // явно: тест про сортировку, а не про часы.
        sqlx::query("UPDATE calls SET started_at = ?1 WHERE id = ?2")
            .bind("2020-01-01T00:00:00Z")
            .bind(&first.id)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE calls SET started_at = ?1 WHERE id = ?2")
            .bind("2026-07-27T00:00:00Z")
            .bind(&second.id)
            .execute(&db.pool)
            .await
            .unwrap();
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
    async fn get_call_derives_processing_via() {
        let db = fresh_db().await;
        // local-движок (summary_engine local-*) → "local" (важно для гейта
        // кнопки force-re-STT на фронте).
        let local = insert_recording(&db.pool, "managed").await.unwrap();
        sqlx::query(
            "UPDATE calls SET provider='local', summary_engine='local-qwen-3b' WHERE id=?1",
        )
        .bind(&local.id)
        .execute(&db.pool)
        .await
        .unwrap();
        let got = get_call(&db.pool, &local.id).await.unwrap().unwrap();
        assert_eq!(got.processing_via.as_deref(), Some("local"));

        // BYO-ключи → "cloud_byo".
        let byo = insert_recording(&db.pool, "byo").await.unwrap();
        sqlx::query("UPDATE calls SET provider='soniox' WHERE id=?1")
            .bind(&byo.id)
            .execute(&db.pool)
            .await
            .unwrap();
        let got = get_call(&db.pool, &byo.id).await.unwrap().unwrap();
        assert_eq!(got.processing_via.as_deref(), Some("cloud_byo"));

        // Облачный managed → "cloud_managed"; list_calls тоже деривит.
        let cloud = insert_recording(&db.pool, "managed").await.unwrap();
        sqlx::query("UPDATE calls SET provider='gladia' WHERE id=?1")
            .bind(&cloud.id)
            .execute(&db.pool)
            .await
            .unwrap();
        let list = list_calls(&db.pool).await.unwrap();
        let row = list.iter().find(|c| c.id == cloud.id).unwrap();
        assert_eq!(row.processing_via.as_deref(), Some("cloud_managed"));
    }

    // ============================================================
    // [V6.2] pipeline progress
    // ============================================================

    #[tokio::test]
    async fn set_call_progress_persists_step_pct_eta_upload() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &call.id, 12.5).await.unwrap();

        set_call_progress(&db.pool, &call.id, 2, 64, Some(25), Some(1_048_576))
            .await
            .unwrap();

        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.pipeline_step, Some(2));
        assert_eq!(after.pipeline_pct, Some(64));
        assert_eq!(after.pipeline_eta_sec, Some(25));
        assert_eq!(after.upload_bytes, Some(1_048_576));
    }

    #[tokio::test]
    async fn mark_call_ready_clears_pipeline_progress() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        finish_recording(&db.pool, &call.id, 5.0).await.unwrap();
        set_call_progress(&db.pool, &call.id, 5, 100, None, None)
            .await
            .unwrap();

        mark_call_ready(&db.pool, &call.id).await.unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "ready");
        assert!(after.pipeline_step.is_none(), "step должен очиститься");
        assert!(after.pipeline_pct.is_none());
        assert!(after.pipeline_eta_sec.is_none());
        assert!(after.upload_bytes.is_none());
    }

    #[tokio::test]
    async fn fail_recording_clears_pipeline_progress() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        set_call_progress(&db.pool, &call.id, 3, 50, Some(10), Some(2048))
            .await
            .unwrap();
        fail_recording_with_reason(&db.pool, &call.id, Some("STT down"))
            .await
            .unwrap();
        let after = get_call(&db.pool, &call.id).await.unwrap().unwrap();
        assert_eq!(after.status, "failed");
        assert!(after.pipeline_step.is_none());
        assert!(after.pipeline_pct.is_none());
    }

    // [P5.2] `update_call_duration` overwrites duration_sec на каждом
    // sidecar `rotated` event. До этого fix'а единственным writer'ом был
    // `finish_recording` → UI показывал stale значение во время recording.
    #[tokio::test]
    async fn update_call_duration_overwrites() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        update_call_duration(&db.pool, &call.id, 600.4)
            .await
            .unwrap();
        let dur: Option<i64> = sqlx::query_scalar("SELECT duration_sec FROM calls WHERE id = ?1")
            .bind(&call.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(dur, Some(600)); // .4 → round → 600

        // Sidecar шлёт monotonic increasing → overwrite OK.
        update_call_duration(&db.pool, &call.id, 1200.7)
            .await
            .unwrap();
        let dur: Option<i64> = sqlx::query_scalar("SELECT duration_sec FROM calls WHERE id = ?1")
            .bind(&call.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(dur, Some(1201)); // .7 → round → 1201
    }
}
