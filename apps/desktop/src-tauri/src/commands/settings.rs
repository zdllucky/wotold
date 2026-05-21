//! Commands for KV-style app settings.

use tauri::State;

use crate::{state::AppState, AppError};

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
