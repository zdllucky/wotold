//! Admin / lifecycle commands — full wipe + updater.

use tauri::{AppHandle, State};

use crate::{secrets, state::AppState, updater::AvailableUpdate, AppError};

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

    // 4. device-id — пусть на следующий запуск получит новый.
    let device_file = state.app_data_dir.join("device.json");
    if device_file.exists() {
        let _ = tokio::fs::remove_file(&device_file).await;
    }

    // 5. Keychain — удаляем все BYO ключи и session-токен. Ошибки не fail —
    // ключа могло не быть.
    let _ = secrets::delete_key(secrets::ByoProvider::Soniox);
    let _ = secrets::delete_key(secrets::ByoProvider::Gladia);
    let _ = secrets::delete_key(secrets::ByoProvider::Anthropic);
    let _ = secrets::clear_account_session();

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
