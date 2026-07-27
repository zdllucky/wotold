//! [TD-42] Постраничная выборка звонков и счётчики.
//!
//! Выделено из `calls/lifecycle.rs` вместе с тестами: жизненный цикл строки
//! звонка и то, как её показывают списками, — разные поводы открыть файл, а
//! `lifecycle.rs` от этих добавок снова перевалил за лимит 800 (правило 8).

use sqlx::SqlitePool;

use crate::AppError;

use super::lifecycle::Call;

/// [TD-42] Колонки строки звонка — один список на все выборки, чтобы
/// пагинированный запрос не разъехался с полным по набору полей.
pub(super) const CALL_COLUMNS: &str = "id, title, started_at, ended_at, duration_sec, status, provider, path_label, lang_detected, failed_reason, recap_failed_reason, pipeline_step, pipeline_pct, pipeline_eta_sec, upload_bytes, paused_at, paused_total_ms, call_type, call_type_confidence, summary_schema_version, summary_engine, summary_pipeline_mode, created_at, updated_at";

/// [TD-42] Страница звонков от свежих к старым.
///
/// Полный `list_calls` остаётся для экранов, которым действительно нужна вся
/// история (инбокс с фасетами, агрегация по контактам). А рельса «Недавние»
/// брала все строки на КАЖДОЕ событие пайплайна и выбрасывала всё после
/// пятидесятой — вот для неё это.
pub async fn list_calls_page(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> Result<Vec<Call>, AppError> {
    // Отрицательный limit в SQLite означает «без ограничения» — ровно то, от
    // чего задача и уводит. Клэмпим у границы, а не доверяем вызывающему.
    let limit = limit.clamp(0, 1000);
    let offset = offset.max(0);
    let rows: Vec<Call> = sqlx::query_as(&format!(
        "SELECT {CALL_COLUMNS}
         FROM calls
         ORDER BY started_at DESC
         LIMIT ?1 OFFSET ?2"
    ))
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Call::with_processing_via).collect())
}

/// [TD-42] Сколько всего звонков. Нужен вместе с `list_calls_page`: рельса
/// показывает счётчик, а страница его больше не даёт.
pub async fn count_calls(pool: &SqlitePool) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM calls")
        .fetch_one(pool)
        .await?)
}

/// [TD-42] Id готовых звонков. Bulk-реген раньше тянул все строки целиком и
/// отфильтровывал `status != 'ready'` в Rust — фильтр просился в WHERE, а
/// остальные 23 колонки ему не нужны вовсе.
pub async fn list_ready_call_ids(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM calls WHERE status = 'ready' ORDER BY started_at DESC")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

#[cfg(test)]
mod tests {
    use crate::db::calls::*;
    use crate::db::test_support::fresh_db;
    // [TD-42] Пагинация и счётчик
    // ============================================================

    /// Фиксированные `started_at` вместо ожидания смены секунды (правило 6).
    async fn seed_calls_with_times(pool: &sqlx::SqlitePool, times: &[&str]) -> Vec<String> {
        let mut ids = Vec::new();
        for t in times {
            let call = insert_recording(pool, "local").await.unwrap();
            sqlx::query("UPDATE calls SET started_at = ?1 WHERE id = ?2")
                .bind(t)
                .bind(&call.id)
                .execute(pool)
                .await
                .unwrap();
            ids.push(call.id);
        }
        ids
    }

    #[tokio::test]
    async fn page_returns_window_in_same_order_as_full_list() {
        let db = fresh_db().await;
        let ids = seed_calls_with_times(
            &db.pool,
            &[
                "2026-01-01T00:00:00Z",
                "2026-01-02T00:00:00Z",
                "2026-01-03T00:00:00Z",
            ],
        )
        .await;
        // Свежие первыми: id[2], id[1], id[0].
        let page = list_calls_page(&db.pool, 2, 0).await.unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].id, ids[2]);
        assert_eq!(page[1].id, ids[1]);

        let next = list_calls_page(&db.pool, 2, 2).await.unwrap();
        assert_eq!(next.len(), 1, "хвост короче страницы");
        assert_eq!(next[0].id, ids[0]);

        // Порядок совпадает с полной выборкой — экраны не должны расходиться.
        let full = list_calls(&db.pool).await.unwrap();
        let full_ids: Vec<_> = full.iter().map(|c| c.id.as_str()).collect();
        let paged_ids: Vec<_> = page
            .iter()
            .chain(next.iter())
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(full_ids, paged_ids);
    }

    #[tokio::test]
    async fn page_clamps_negative_limit_instead_of_returning_everything() {
        // В SQLite отрицательный LIMIT означает «без ограничения» — то есть
        // ровно то, от чего задача уводит. Клэмп обязан это ловить.
        let db = fresh_db().await;
        seed_calls_with_times(&db.pool, &["2026-01-01T00:00:00Z", "2026-01-02T00:00:00Z"]).await;
        assert!(list_calls_page(&db.pool, -1, 0).await.unwrap().is_empty());
        assert_eq!(list_calls_page(&db.pool, 1, -5).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn page_derives_processing_via_like_full_list() {
        // Страница проходит через ту же деривацию, что и полный список —
        // иначе рельса «Недавние» показывала бы другой бейдж, чем инбокс.
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "local").await.unwrap();
        finish_recording(&db.pool, &call.id, 10.0).await.unwrap();
        let page = list_calls_page(&db.pool, 10, 0).await.unwrap();
        let full = list_calls(&db.pool).await.unwrap();
        assert_eq!(page[0].processing_via, full[0].processing_via);
    }

    #[tokio::test]
    async fn count_counts_all_statuses() {
        let db = fresh_db().await;
        assert_eq!(count_calls(&db.pool).await.unwrap(), 0);
        let a = insert_recording(&db.pool, "local").await.unwrap();
        insert_recording(&db.pool, "local").await.unwrap();
        finish_recording(&db.pool, &a.id, 5.0).await.unwrap();
        mark_call_ready(&db.pool, &a.id).await.unwrap();
        assert_eq!(
            count_calls(&db.pool).await.unwrap(),
            2,
            "счётчик рельсы считает всё, а не только готовые"
        );
    }

    #[tokio::test]
    async fn ready_ids_filter_in_sql_not_in_rust() {
        let db = fresh_db().await;
        let ready = insert_recording(&db.pool, "local").await.unwrap();
        let failed = insert_recording(&db.pool, "local").await.unwrap();
        finish_recording(&db.pool, &ready.id, 5.0).await.unwrap();
        mark_call_ready(&db.pool, &ready.id).await.unwrap();
        fail_recording_with_reason(&db.pool, &failed.id, Some("boom"))
            .await
            .unwrap();

        let ids = list_ready_call_ids(&db.pool).await.unwrap();
        assert_eq!(ids, vec![ready.id]);
    }
}
