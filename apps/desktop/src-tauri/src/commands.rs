use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    audio::macos as audio_macos,
    audio::permissions::{self, PermissionsStatus},
    db::{ActionItem, Call, CallSpeakerView, Contact, ContactInput, OwnerContact, VoiceSampleView},
    secrets::{self, ByoProvider, ByoStatus},
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
pub async fn get_call(state: State<'_, AppState>, id: String) -> Result<Option<Call>, AppError> {
    crate::db::get_call(&state.db, &id).await
}

/// C5 (#41) cascade delete. Удаляет calls row, связанные action_items/call_speakers
/// (по CASCADE), voice_samples с source_call=id, и audio-файлы на диске.
#[tauri::command]
pub async fn delete_call(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    crate::db::delete_call_and_samples(&state.db, &id).await?;

    let call_dir = state.app_data_dir.join("calls").join(&id);
    if call_dir.exists() {
        if let Err(e) = tokio::fs::remove_dir_all(&call_dir).await {
            // Audio удалили частично или не было — БД уже консистентна, логируем но не fail.
            log::warn!("delete_call: rm {} failed: {e}", call_dir.display());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn list_call_action_items(
    state: State<'_, AppState>,
    call_id: String,
) -> Result<Vec<ActionItem>, AppError> {
    crate::db::list_action_items(&state.db, &call_id).await
}

#[tauri::command]
pub async fn read_call_artifact(
    state: State<'_, AppState>,
    call_id: String,
    kind: String,
) -> Result<Option<String>, AppError> {
    let filename = match kind.as_str() {
        "recap" => "recap.md",
        "transcript" => "transcript.md",
        other => return Err(AppError::Other(format!("unknown artifact kind: {other}"))),
    };
    let path = state
        .app_data_dir
        .join("calls")
        .join(&call_id)
        .join(filename);
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::Other(format!("read {filename}: {e}"))),
    }
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
pub async fn update_contact(
    state: State<'_, AppState>,
    id: String,
    input: ContactInput,
) -> Result<Contact, AppError> {
    crate::db::update_contact(&state.db, &id, input).await
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
    let pool = state.db.clone();
    let device_id = state.device_id.clone();
    let app_data_dir = state.app_data_dir.clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let ctx = crate::pipeline::PipelineCtx {
            call_id: call_id.clone(),
            call_dir: app_data_dir.join("calls").join(&call_id),
            mic_path,
            system_path,
            device_id,
        };
        // [B5]: передаём AppHandle чтобы pipeline emit 'pipeline:finished'.
        if let Err(e) = crate::pipeline::run(&pool, ctx, Some(&app_handle)).await {
            log::error!("pipeline {call_id} error: {e}");
        }
    });

    Ok(call)
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

/// M4.5 паспорта: пересоздать рекап + action_items без повторной транскрипции.
/// Ошибки LLM пробрасываются (UI показывает toast / error), в отличие от
/// pipeline::run где рекап silent-skip при ошибке (транскрипт важнее).
#[tauri::command]
pub async fn regenerate_recap(state: State<'_, AppState>, call_id: String) -> Result<(), AppError> {
    crate::pipeline::regenerate_recap(&state.db, &state.app_data_dir, &state.device_id, &call_id)
        .await
}
