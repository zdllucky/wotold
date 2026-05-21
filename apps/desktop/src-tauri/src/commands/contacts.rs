//! Commands over contacts (CRUD + owner-contact rename).

use tauri::State;

use crate::{
    db::{Contact, ContactInput, OwnerContact},
    state::AppState,
    AppError,
};

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
pub async fn update_contact(
    state: State<'_, AppState>,
    id: String,
    input: ContactInput,
) -> Result<Contact, AppError> {
    crate::db::update_contact(&state.db, &id, input).await
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
