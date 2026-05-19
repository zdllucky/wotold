use tauri::{AppHandle, State};

use crate::{db::OwnerContact, state::AppState, updater::AvailableUpdate, AppError};

#[tauri::command]
pub fn get_device_id(state: State<'_, AppState>) -> String {
    state.device_id.to_string()
}

#[tauri::command]
pub async fn get_owner_contact(state: State<'_, AppState>) -> Result<OwnerContact, AppError> {
    crate::db::ensure_owner_contact(&state.db).await
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
