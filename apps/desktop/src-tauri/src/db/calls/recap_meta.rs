//! [B16 / P5.1 / M14] Метаданные саммари звонка: причина провала рекапа и
//! bulk-запись полей v2.
//!
//! [TD-41] Выделено из `calls/lifecycle.rs` (1423 строки при лимите 800,
//! правило 8) вместе с тестами. Общий инвариант этих функций — banner в UI
//! («почему рекапа нет» + каким движком пробовали) должен быть согласован,
//! поэтому reason и engine пишутся одним UPDATE. Логика не менялась.

use sqlx::SqlitePool;

use crate::AppError;

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

/// [P5.1] Atomic UPDATE `recap_failed_reason` + `summary_engine` для
/// предотвращения mismatch race при engine-switching между attempts.
///
/// Без этого помощника `summary_engine` персистится только в
/// `persist_recap_from_json` (success path), а failure path обновляет
/// только reason. Result: после Cloud → Local switch banner показывает
/// stale "ОБЛАКО" badge с "Локальная модель не успела…" текстом.
///
/// `engine_label` semantics:
/// - `Some(label)` → overwrite `summary_engine` (current attempt engine).
/// - `None` → leave `summary_engine` unchanged (caller не знает engine).
pub async fn set_recap_failure(
    pool: &SqlitePool,
    call_id: &str,
    reason: Option<&str>,
    engine_label: Option<&str>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    match engine_label {
        Some(engine) => {
            sqlx::query(
                "UPDATE calls
                 SET recap_failed_reason = ?1, summary_engine = ?2, updated_at = ?3
                 WHERE id = ?4",
            )
            .bind(reason)
            .bind(engine)
            .bind(&now)
            .bind(call_id)
            .execute(pool)
            .await?;
        }
        None => {
            // Fallback на reason-only — caller не resolved engine label.
            set_recap_failed_reason(pool, call_id, reason).await?;
        }
    }
    Ok(())
}

/// [M14 T-02] Bulk-update M14 summary metadata fields в `calls` row.
/// Single UPDATE — атомарно. Все поля nullable; передавайте Some только
/// для того, что меняете (None = не трогать).
///
/// Используется в `pipeline::recap::persist_summary_v2` после успешного
/// cloud v2 generate'а.
#[allow(dead_code)] // [M14 T-02] Wired в recap.rs в Step 4 этого slice'а.
pub struct SummaryMetadata<'a> {
    pub engine: &'a str,
    pub schema_version: u8,
    pub call_type: Option<&'a str>,
    pub call_type_confidence: Option<f32>,
    pub pipeline_mode: &'a str,
    pub generation_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub type_specific_block_json: Option<&'a str>,
}

#[allow(dead_code)] // [M14 T-02] Production caller в recap.rs Step 4.
pub async fn set_summary_metadata(
    pool: &SqlitePool,
    call_id: &str,
    meta: SummaryMetadata<'_>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE calls
         SET summary_engine = ?1,
             summary_schema_version = ?2,
             call_type = ?3,
             call_type_confidence = ?4,
             summary_pipeline_mode = ?5,
             summary_generation_ms = ?6,
             summary_input_tokens = ?7,
             summary_output_tokens = ?8,
             summary_type_specific_block = ?9,
             updated_at = ?10
         WHERE id = ?11",
    )
    .bind(meta.engine)
    .bind(meta.schema_version as i64)
    .bind(meta.call_type)
    .bind(meta.call_type_confidence)
    .bind(meta.pipeline_mode)
    .bind(meta.generation_ms)
    .bind(meta.input_tokens)
    .bind(meta.output_tokens)
    .bind(meta.type_specific_block_json)
    .bind(&now)
    .bind(call_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    // Соседние функции звонка приходят через фасад `db::calls`:
    // тесты домена всё равно строят строку через `insert_recording`.
    use crate::db::calls::*;
    use crate::db::test_support::fresh_db;

    // [P5.1] `set_recap_failure` атомарно UPDATE'ит reason + engine_label,
    // предотвращая banner mismatch при engine-switching между attempts.

    #[tokio::test]
    async fn set_recap_failure_updates_both_fields() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        // Initial success persist через прямой SQL чтобы не depend on recap.rs.
        sqlx::query("UPDATE calls SET summary_engine = 'cloud-managed' WHERE id = ?1")
            .bind(&call.id)
            .execute(&db.pool)
            .await
            .unwrap();

        // Engine switch: следующая attempt fails на local engine.
        set_recap_failure(
            &db.pool,
            &call.id,
            Some("local_llm_timeout"),
            Some("local-qwen-7b"),
        )
        .await
        .unwrap();

        let (reason, engine): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT recap_failed_reason, summary_engine FROM calls WHERE id = ?1")
                .bind(&call.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(reason.as_deref(), Some("local_llm_timeout"));
        assert_eq!(engine.as_deref(), Some("local-qwen-7b"));
    }

    #[tokio::test]
    async fn set_recap_failure_with_none_engine_leaves_engine_unchanged() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        sqlx::query("UPDATE calls SET summary_engine = 'cloud-managed' WHERE id = ?1")
            .bind(&call.id)
            .execute(&db.pool)
            .await
            .unwrap();

        set_recap_failure(&db.pool, &call.id, Some("oops"), None)
            .await
            .unwrap();

        let (reason, engine): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT recap_failed_reason, summary_engine FROM calls WHERE id = ?1")
                .bind(&call.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(reason.as_deref(), Some("oops"));
        // Engine unchanged.
        assert_eq!(engine.as_deref(), Some("cloud-managed"));
    }

    #[tokio::test]
    async fn set_recap_failure_clears_reason_on_none() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();
        set_recap_failure(
            &db.pool,
            &call.id,
            Some("first fail"),
            Some("local-qwen-3b"),
        )
        .await
        .unwrap();
        set_recap_failure(&db.pool, &call.id, None, Some("local-qwen-3b"))
            .await
            .unwrap();
        let reason: Option<String> =
            sqlx::query_scalar("SELECT recap_failed_reason FROM calls WHERE id = ?1")
                .bind(&call.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert!(reason.is_none());
    }
}
