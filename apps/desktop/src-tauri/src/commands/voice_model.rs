//! Commands for voice embedder model (download / status / delete / info).
//!
//! [B3.7c] Voice embedder model management — runtime download.
//! Используется Settings → Распознавание голоса. См. voice_model.rs.

use tauri::{AppHandle, State};

use crate::{state::AppState, AppError};

#[tauri::command]
pub async fn voice_model_status(
    state: State<'_, AppState>,
) -> Result<crate::voice_model::ModelStatus, AppError> {
    Ok(crate::voice_model::check_status(&state.app_data_dir).await)
}

#[tauri::command]
pub async fn voice_model_download(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), AppError> {
    crate::voice_model::download(&state.app_data_dir, &app).await?;
    Ok(())
}

#[tauri::command]
pub async fn voice_model_delete(state: State<'_, AppState>) -> Result<(), AppError> {
    crate::voice_model::delete(&state.app_data_dir).await
}

#[tauri::command]
pub fn voice_model_info() -> serde_json::Value {
    serde_json::json!({
        "url": crate::voice_model::MODEL_URL,
        "sha256": crate::voice_model::MODEL_SHA256,
        "size_hint": crate::voice_model::MODEL_SIZE_HINT,
        // [B3.7] При сборке без `voice-onnx` feature модель скачать можно,
        // но pipeline её не использует (только подмена `OnnxEmbedder`
        // вместо `StubEmbedder` происходит под фичей). Frontend показывает
        // об этом honest badge "feature не включена в сборке".
        "feature_enabled": cfg!(feature = "voice-onnx"),
    })
}
