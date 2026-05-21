//! [S2/S3] Tauri commands для управления call-activity probe.
//!
//! Frontend дергает их когда юзер двигает toggle на SettingsPage. Сам
//! controller живёт в `AppState::call_detect` (см. `audio/call_detect.rs`).

use tauri::{AppHandle, State};

use crate::{state::AppState, AppError};

/// Включить probe. `cooldown_min` приходит из настроек (3/5/10/15).
/// Идемпотентно — повторный вызов обновляет cooldown_min.
#[tauri::command]
pub async fn enable_call_detect(
    app: AppHandle,
    state: State<'_, AppState>,
    cooldown_min: u32,
) -> Result<(), AppError> {
    let cooldown_min = cooldown_min.clamp(1, 60) as u64;
    state.call_detect.enable(app, cooldown_min).await
}

/// Выключить probe. Идемпотентно.
#[tauri::command]
pub async fn disable_call_detect(state: State<'_, AppState>) -> Result<(), AppError> {
    state.call_detect.disable().await
}

#[tauri::command]
pub async fn is_call_detect_enabled(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(state.call_detect.is_enabled().await)
}
