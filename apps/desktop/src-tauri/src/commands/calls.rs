//! Commands for read/delete/export over calls + reading artifacts.

use tauri::State;

use crate::{
    call_id::CallId,
    call_store::{ArtifactKind, AudioKind},
    db::{ActionItem, Call},
    services::export::compose_call_markdown,
    state::AppState,
    AppError,
};

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
    // [TD-05] id из webview: до валидации он не имеет права стать путём.
    // `remove_call_dir("..")` раньше сносил весь app_data_dir вместе с БД.
    let call_id = CallId::parse(&id)?;
    crate::db::delete_call_and_samples(&state.db, &id).await?;
    if let Err(e) = state.store.remove_call_dir(&call_id).await {
        // Audio удалили частично или не было — БД уже консистентна, логируем но не fail.
        log::warn!("delete_call: {e}");
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

/// [M14 T-11] V2 structured summary blocks — decisions list for UI rendering.
/// Возвращает rows ordered by `order_idx ASC`. Пустой если schema_version=1
/// (legacy) либо если LLM не вернул decisions.
#[tauri::command]
pub async fn list_call_decisions(
    state: State<'_, AppState>,
    call_id: String,
) -> Result<Vec<crate::db::decisions::DecisionRow>, AppError> {
    crate::db::decisions::list_decisions(&state.db, &call_id).await
}

/// [M14 T-11] V2 structured summary blocks — open questions list для UI.
#[tauri::command]
pub async fn list_call_open_questions(
    state: State<'_, AppState>,
    call_id: String,
) -> Result<Vec<crate::db::open_questions::OpenQuestionRow>, AppError> {
    crate::db::open_questions::list_open_questions(&state.db, &call_id).await
}

#[tauri::command]
pub async fn read_call_artifact(
    state: State<'_, AppState>,
    call_id: String,
    kind: String,
) -> Result<Option<String>, AppError> {
    let kind = ArtifactKind::from_str(&kind)
        .ok_or_else(|| AppError::Other(format!("unknown artifact kind: {kind}")))?;
    let call_id = CallId::parse(&call_id)?;
    state.store.read_artifact(&call_id, kind).await
}

/// Экспорт звонка в одиночный markdown-файл по выбранному пользователем
/// пути. Композирует metadata header (title + дата + длительность +
/// провайдер) + recap.md + transcript.md (если есть). Если оба артефакта
/// отсутствуют — Err. dest_path берётся из save-dialog'а на frontend'е,
/// валидируется здесь (must end in `.md`, must be writable).
#[tauri::command]
pub async fn export_call_markdown(
    state: State<'_, AppState>,
    call_id: String,
    dest_path: String,
) -> Result<(), AppError> {
    use std::path::Path;
    let dest = Path::new(&dest_path);
    // [Sec] Sanity: расширение .md — иначе юзер может случайно перезаписать
    // важный файл. Не строгая валидация — просто guard от опечаток.
    if dest.extension().and_then(|e| e.to_str()) != Some("md") {
        return Err(AppError::Other(
            "Файл должен иметь расширение .md".to_string(),
        ));
    }
    let call = crate::db::get_call(&state.db, &call_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("call {call_id}")))?;
    let call_id = CallId::parse(&call_id)?;

    let recap = state
        .store
        .read_artifact(&call_id, ArtifactKind::Recap)
        .await?;
    let transcript = state
        .store
        .read_artifact(&call_id, ArtifactKind::Transcript)
        .await?;

    let md = compose_call_markdown(&call, recap.as_deref(), transcript.as_deref())?;

    tokio::fs::write(&dest, md)
        .await
        .map_err(|e| AppError::Other(format!("write {dest_path}: {e}")))?;
    Ok(())
}

/// [B16 UX P0] Возвращает абсолютный путь к WAV-файлу звонка для аудиоплеера.
/// Frontend использует convertFileSrc(path) → asset:// URL → `<audio src>`.
/// kind: 'mic' | 'system'. Если файл не существует — Err.
#[tauri::command]
pub async fn get_call_audio_path(
    state: State<'_, AppState>,
    call_id: String,
    kind: String,
) -> Result<String, AppError> {
    let audio_kind = AudioKind::from_str(&kind)
        .ok_or_else(|| AppError::Other(format!("unknown audio kind: {kind}")))?;
    let call_id = CallId::parse(&call_id)?;
    let path = state.store.audio_path(&call_id, audio_kind);
    if path.exists() {
        return Ok(path.to_string_lossy().to_string());
    }

    // [M13 fix] Root WAV нет — chunked-запись пишет chunk 0 в chunks/0/, а root
    // создаётся только merge'ем. Если pipeline упал ДО merge (напр. модель не
    // найдена), плеер раньше показывал «audio not найден» хотя всё аудио лежит
    // в chunks/{idx}/. Склеиваем chunks→root on-demand, чтобы плеер всегда имел
    // полную дорожку. Fallback — chunks/0/ (частичное аудио лучше пустого).
    let chunks_dir = state.store.chunks_dir(&call_id);
    if chunks_dir.exists() {
        let cd = chunks_dir.clone();
        let call_dir = state.store.call_dir(&call_id);
        let _ = tokio::task::spawn_blocking(move || {
            crate::pipeline::audio_merger::merge_both_tracks(&cd, &call_dir);
        })
        .await;
        if path.exists() {
            return Ok(path.to_string_lossy().to_string());
        }
        let chunk0 = state
            .store
            .chunk_dir(&call_id, 0)
            .join(audio_kind.filename());
        if chunk0.exists() {
            return Ok(chunk0.to_string_lossy().to_string());
        }
    }

    Err(AppError::Other(format!(
        "audio file {} не найден для звонка {call_id}",
        audio_kind.filename()
    )))
}
