//! Commands for speaker confirmation flow + voice samples management.

use tauri::State;

use crate::{
    db::{CallSpeakerView, VoiceSampleView},
    state::AppState,
    AppError,
};

// ============================================================
// M3.5 (#26) speaker confirmation flow
// ============================================================

/// Спикеры звонка + текущая привязка + suggestion. UI рисует на основе этого.
#[tauri::command]
pub async fn list_call_speakers(
    state: State<'_, AppState>,
    call_id: String,
) -> Result<Vec<CallSpeakerView>, AppError> {
    crate::db::list_call_speakers(&state.db, &call_id).await
}

/// R2 паспорта: финальная привязка спикер↔контакт ТОЛЬКО через явное действие
/// пользователя. Используется UI confirmation flow.
#[tauri::command]
pub async fn confirm_call_speaker(
    state: State<'_, AppState>,
    call_speaker_id: String,
    contact_id: String,
) -> Result<(), AppError> {
    crate::db::confirm_call_speaker(&state.db, &call_speaker_id, &contact_id).await
}

/// Откатить ранее подтверждённую привязку (юзер передумал).
#[tauri::command]
pub async fn unbind_call_speaker(
    state: State<'_, AppState>,
    call_speaker_id: String,
) -> Result<(), AppError> {
    crate::db::unbind_call_speaker(&state.db, &call_speaker_id).await
}

// ============================================================
// M3.6 / M7.4 (#45) voice samples view + manual delete (C3)
// ============================================================

#[tauri::command]
pub async fn list_voice_samples(
    state: State<'_, AppState>,
    contact_id: String,
) -> Result<Vec<VoiceSampleView>, AppError> {
    crate::db::list_voice_samples(&state.db, &contact_id).await
}

/// Manual delete одного семпла (C3 паспорта). Используется когда пользователь
/// ошибочно подтвердил спикера или хочет очистить устаревший биометрический
/// слепок.
#[tauri::command]
pub async fn delete_voice_sample(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    crate::db::delete_voice_sample(&state.db, &id).await
}
