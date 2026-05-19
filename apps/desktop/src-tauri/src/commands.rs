use tauri::State;

use crate::{db::OwnerContact, state::AppState, AppError};

#[tauri::command]
pub fn get_device_id(state: State<'_, AppState>) -> String {
    state.device_id.to_string()
}

#[tauri::command]
pub async fn get_owner_contact(
    state: State<'_, AppState>,
) -> Result<OwnerContact, AppError> {
    crate::db::ensure_owner_contact(&state.db).await
}
