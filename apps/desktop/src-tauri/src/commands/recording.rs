//! Commands for start/stop recording + audio permissions.

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    audio::macos as audio_macos,
    audio::permissions::{self, PermissionsStatus},
    db::Call,
    services::pipeline_runner::PipelineRunner,
    state::AppState,
    AppError,
};

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

    // [B16 audit P1]: pre-check разрешений перед попыткой start. Раньше
    // sidecar просто молча fail-stop'ал — юзер видел загадочный «calls failed»
    // через 1-2 секунды. Теперь возвращаем clear AppError → frontend
    // покажет 'Нет разрешения на ...' и направит в Настройки.
    let perms = permissions::check(&app).await?;
    if perms.microphone != "granted" {
        return Err(AppError::Other(
            "Нет разрешения на микрофон. Открой Настройки → Разрешения.".into(),
        ));
    }
    if perms.screen_recording != "granted" {
        return Err(AppError::Other(
            "Нет разрешения на запись экрана (для системного аудио). Открой Настройки → Разрешения.".into(),
        ));
    }

    // M2.3: path_label фиксирует путь доставки на момент создания звонка.
    // По умолчанию managed; переключаемое значение из settings подключим
    // в #20/#21 (когда провайдеры реально начнут вызываться).
    let path_label = "managed";
    let call = crate::db::insert_recording(&state.db, path_label).await?;
    let mic_path = state.store.mic_path(&call.id);
    let system_path = state.store.system_path(&call.id);

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
pub async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<Call, AppError> {
    let session = {
        let mut guard = state.recording.lock().await;
        guard
            .take()
            .ok_or_else(|| AppError::Other("not recording".into()))?
    };

    let call_id = session.call_id.clone();
    let mic_path = session.mic_path.clone();
    let system_path = session.system_path.clone();
    let result = audio_macos::stop(session).await;

    let call = match result {
        Ok(r) => crate::db::finish_recording(&state.db, &call_id, r.duration_sec).await?,
        Err(e) => {
            let _ = crate::db::fail_recording(&state.db, &call_id).await;
            return Err(e);
        }
    };

    // M2.4-2.5: транскрипция в фоне. Возвращаем клиенту calls row сразу
    // (status=processing), статус подтянется через list_calls когда pipeline
    // финишнет (status → ready или failed).
    PipelineRunner::spawn_initial(
        state.db.clone(),
        state.store.clone(),
        state.device_id.clone(),
        app,
        state.pipeline_tasks.clone(),
        call_id,
        mic_path,
        system_path,
    )
    .await;

    Ok(call)
}

#[tauri::command]
pub async fn get_audio_permissions(app: AppHandle) -> Result<PermissionsStatus, AppError> {
    permissions::check(&app).await
}

#[tauri::command]
pub async fn request_audio_permissions(
    app: AppHandle,
    target: String,
) -> Result<PermissionsStatus, AppError> {
    permissions::request(&app, &target).await
}

#[tauri::command]
pub fn open_system_privacy_pane(pane: String) -> Result<(), AppError> {
    let url = match pane.as_str() {
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        "screen_recording" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        _ => return Err(AppError::Other(format!("unknown pane: {pane}"))),
    };

    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| AppError::Other(format!("open failed: {e}")))?;
    Ok(())
}
