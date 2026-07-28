//! Admin / lifecycle commands — full wipe + updater.

use tauri::{AppHandle, State};

use crate::{state::AppState, updater::AvailableUpdate, AppError};

/// [B16 audit P2 / GDPR Art. 17]: полный wipe всех пользовательских данных.
/// Удаляет:
///   - все записи звонков (`calls/` директория)
///   - всю БД (`app.db`)
///   - все BYO API-ключи и session-токены из Keychain
///   - device-id (`device.txt`) — следующий запуск сгенерирует новый
/// Onboarding-флаг и настройки тоже исчезают вместе с БД — при следующем
/// запуске юзер увидит онбординг с нуля. Не трогает logs (отдельная
/// директория ~/Library/Logs/...).
#[tauri::command]
pub async fn wipe_all_data(state: State<'_, AppState>) -> Result<(), AppError> {
    // 1. Закрыть pool — иначе rm на app.db даст 'database is locked'.
    state.db.close().await;

    // 2. Удалить calls/ recursively.
    if let Err(e) = state.store.remove_all_calls().await {
        log::warn!("wipe: {e}");
    }

    // 3. Удалить БД (app.db, WAL, SHM).
    for fname in ["app.db", "app.db-wal", "app.db-shm"] {
        let p = state.app_data_dir.join(fname);
        if p.exists() {
            let _ = tokio::fs::remove_file(&p).await;
        }
    }

    // Keychain: cloud/BYO-ключи и auth-session удалены (local-only). Будущие
    // секреты внешних интеграций (secrets::* generic-seam) должны вычищаться
    // здесь по мере их появления.

    log::warn!("wipe_all_data: пользователь стёр всё. Перезапуск приложения требуется.");
    Ok(())
}

/// Неблокирующая проверка обновления (M11.4). UI вызывает при старте,
/// показывает ненавязчивый промпт если результат — Some.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<AvailableUpdate>, AppError> {
    crate::updater::check(&app).await
}

/// По согласию пользователя — скачать, установить, перезапустить.
#[tauri::command]
pub async fn apply_update(app: AppHandle) -> Result<(), AppError> {
    crate::updater::apply(&app).await
}

/// Диагностика: сводка падений чанков за `days` суток (по умолчанию 7).
///
/// Экрана для неё нет и не планируется — это ответ на вопрос «часто ли у меня
/// вообще падают чанки и на чём», который иначе требует гриппинга ротируемых
/// логов. Данные локальные, наружу не уходят (как и `summary_generation_log`).
#[tauri::command]
pub async fn telemetry_chunk_failures(
    state: State<'_, AppState>,
    days: Option<i64>,
) -> Result<serde_json::Value, AppError> {
    let days = days.unwrap_or(7).clamp(1, 365);
    let s = crate::db::telemetry::chunk_failure_stats(&state.db, days).await?;
    Ok(serde_json::json!({
        "days": days,
        "failures": s.failures,
        "distinct_chunks": s.distinct_chunks,
        "chunks_total": s.chunks_total,
        "failed_pct": s.failed_pct(),
        "by_preset": s.by_preset,
        "by_reason": s.by_reason,
    }))
}
