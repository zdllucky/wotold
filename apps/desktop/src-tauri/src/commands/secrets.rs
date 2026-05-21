//! Commands for BYO API keys + account session + device id.

use serde::Serialize;
use tauri::State;

use crate::{
    secrets::{self, ByoProvider, ByoStatus},
    state::AppState,
    AppError,
};

#[tauri::command]
pub fn get_device_id(state: State<'_, AppState>) -> String {
    state.device_id.to_string()
}

/// #47: записать BYO API key в системный keychain. Никогда не логируется и
/// не возвращается обратно. Пустая строка = удаление.
#[tauri::command]
pub fn set_byo_key(provider: ByoProvider, value: String) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        secrets::delete_key(provider)
    } else {
        secrets::set_key(provider, trimmed)
    }
}

#[tauri::command]
pub fn delete_byo_key(provider: ByoProvider) -> Result<(), AppError> {
    secrets::delete_key(provider)
}

/// Возвращает per-provider status (has key / empty), без раскрытия значений.
#[tauri::command]
pub fn list_byo_status() -> Result<Vec<ByoStatus>, AppError> {
    secrets::status_all()
}

// =================== Account session (#38, M10.2) ===================

#[derive(Debug, Clone, Serialize)]
pub struct AccountSessionStatus {
    pub present: bool,
}

#[tauri::command]
pub fn get_account_session_status() -> Result<AccountSessionStatus, AppError> {
    Ok(AccountSessionStatus {
        present: secrets::has_account_session()?,
    })
}

#[tauri::command]
pub fn set_account_session(token: String) -> Result<(), AppError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        secrets::clear_account_session()
    } else {
        secrets::set_account_session(trimmed)
    }
}

#[tauri::command]
pub fn clear_account_session() -> Result<(), AppError> {
    secrets::clear_account_session()
}

/// Возвращает session token для встраивания в Authorization Bearer при запросах
/// на прокси (например GET /v1/auth/me). Возвращает только когда фронт инициирует
/// запрос — токен живёт в JS памяти не дольше HTTP-вызова.
#[tauri::command]
pub fn read_account_session_token() -> Result<Option<String>, AppError> {
    secrets::read_account_session()
}
