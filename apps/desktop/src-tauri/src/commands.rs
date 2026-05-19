use tauri::{AppHandle, State};

use crate::{
    db::{Contact, ContactInput, OwnerContact},
    state::AppState,
    updater::AvailableUpdate,
    AppError,
};

#[tauri::command]
pub fn get_device_id(state: State<'_, AppState>) -> String {
    state.device_id.to_string()
}

#[tauri::command]
pub async fn get_owner_contact(state: State<'_, AppState>) -> Result<OwnerContact, AppError> {
    crate::db::ensure_owner_contact(&state.db).await
}

#[tauri::command]
pub async fn list_contacts(state: State<'_, AppState>) -> Result<Vec<Contact>, AppError> {
    crate::db::list_contacts(&state.db).await
}

#[tauri::command]
pub async fn create_contact(
    state: State<'_, AppState>,
    input: ContactInput,
) -> Result<Contact, AppError> {
    crate::db::create_contact(&state.db, input).await
}

#[tauri::command]
pub async fn delete_contact(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    crate::db::delete_contact(&state.db, &id).await
}

#[tauri::command]
pub async fn rename_owner_contact(
    state: State<'_, AppState>,
    new_name: String,
) -> Result<OwnerContact, AppError> {
    crate::db::rename_owner_contact(&state.db, &new_name).await
}

#[tauri::command]
pub async fn get_setting(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<String>, AppError> {
    crate::db::get_setting(&state.db, &key).await
}

#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), AppError> {
    crate::db::set_setting(&state.db, &key, &value).await
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
