use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    audio::macos as audio_macos,
    db::{Call, Contact, ContactInput, OwnerContact},
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
pub async fn list_calls(state: State<'_, AppState>) -> Result<Vec<Call>, AppError> {
    crate::db::list_calls(&state.db).await
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

#[derive(Debug, Clone, Serialize)]
pub struct RecordingState {
    pub call_id: String,
    pub started_at: String,
}

#[tauri::command]
pub async fn get_recording_state(
    state: State<'_, AppState>,
) -> Result<Option<RecordingState>, AppError> {
    let guard = state.recording.lock().await;
    Ok(guard.as_ref().map(|s| RecordingState {
        call_id: s.call_id.clone(),
        started_at: s.started_at.to_rfc3339(),
    }))
}

#[tauri::command]
pub async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<Call, AppError> {
    let mut guard = state.recording.lock().await;
    if guard.is_some() {
        return Err(AppError::Other("recording already in progress".into()));
    }

    // M2.3: path_label фиксирует путь доставки на момент создания звонка.
    // По умолчанию managed; переключаемое значение из settings подключим
    // в #20/#21 (когда провайдеры реально начнут вызываться).
    let path_label = "managed";
    let call = crate::db::insert_recording(&state.db, path_label).await?;
    let call_dir = state.app_data_dir.join("calls").join(&call.id);
    let mic_path = call_dir.join("mic.wav");
    let system_path = call_dir.join("system.wav");

    match audio_macos::start(&app, call.id.clone(), mic_path, system_path).await {
        Ok(session) => {
            *guard = Some(session);
            Ok(call)
        }
        Err(e) => {
            // Откат: помечаем call как failed чтобы он не висел в "recording" навсегда.
            let _ = crate::db::fail_recording(&state.db, &call.id).await;
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<Call, AppError> {
    let session = {
        let mut guard = state.recording.lock().await;
        guard
            .take()
            .ok_or_else(|| AppError::Other("not recording".into()))?
    };

    let call_id = session.call_id.clone();
    match audio_macos::stop(session).await {
        Ok(result) => crate::db::finish_recording(&state.db, &call_id, result.duration_sec).await,
        Err(e) => {
            let _ = crate::db::fail_recording(&state.db, &call_id).await;
            Err(e)
        }
    }
}
