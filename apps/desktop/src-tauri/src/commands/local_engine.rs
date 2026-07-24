//! [M12.4.2] Tauri commands для local-engine model catalog + preset.
//!
//! Frontend (Settings UI, M12.5) дёргает:
//!
//! - `local_engine_model_status(id)` — per-model snapshot
//! - `local_engine_model_download(id)` — start download (idempotent)
//! - `local_engine_model_delete(id)` — disk cleanup
//! - `local_engine_get_active_preset()` — current setting (`null` до выбора)
//! - `local_engine_set_active_preset(preset)` — atomic swap + сообщает какие
//!   модели надо доскачать (фронт сам зовёт `model_download`)
//! - `local_engine_list_catalog()` — для UI рендера list-modal «Освободить место»
//!
//! Все ошибки → `AppError` маппятся на string в bindings (см. error.rs). На
//! не-macOS платформах модуль не компилируется (R9) — commands отдают
//! `unimplemented` через cfg-gate.

#![cfg(target_os = "macos")]

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    local_engine::{
        hw_probe::{self, HwReport},
        models::{self, ModelKind, ModelStatus, MODEL_CATALOG},
        preset::{LocalEnginePreset, PresetSpec, SETTING_ACTIVE_PRESET},
    },
    state::AppState,
    AppError,
};

/// Settings KV ключ для cached `HwReport` JSON (PRD §M12.7.1).
const SETTING_HW_REPORT: &str = "local_engine.hw_report";

#[derive(Serialize)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub kind: ModelKind,
    pub display_name: &'static str,
    pub size_bytes: u64,
    pub license_url: &'static str,
}

#[tauri::command]
pub fn local_engine_list_catalog() -> Vec<CatalogEntry> {
    MODEL_CATALOG
        .iter()
        .map(|m| CatalogEntry {
            id: m.id.as_str(),
            kind: m.kind,
            display_name: m.display_name,
            size_bytes: m.size_bytes,
            license_url: m.license_url,
        })
        .collect()
}

#[tauri::command]
pub async fn local_engine_model_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<ModelStatus, AppError> {
    // Fast path: file-existence only. SHA256 verified only at download time; runtime checks are size-only [TD-10].
    models::check_status_fast(&state.app_data_dir, &id).await
}

/// [M12.4.4-bis] Сводная таблица для Storage management UI:
/// каталог + статус на диске + last_used_at + badge активности.
#[derive(Serialize)]
pub struct StorageRow {
    pub id: &'static str,
    pub kind: ModelKind,
    pub display_name: &'static str,
    pub size_bytes: u64,
    pub status: ModelStatus,
    pub last_used_at: Option<String>,
    /// `true` если модель входит в текущий active preset.
    pub is_active: bool,
}

#[tauri::command]
pub async fn local_engine_storage_list(
    state: State<'_, AppState>,
) -> Result<Vec<StorageRow>, AppError> {
    let usage = models::list_usage(&state.db).await?;
    let active_preset = crate::db::get_setting(&state.db, SETTING_ACTIVE_PRESET)
        .await?
        .as_deref()
        .and_then(LocalEnginePreset::from_str);
    let active_ids: [Option<&'static str>; 2] = active_preset
        .map(|p| {
            [
                Some(p.whisper_model_id().as_str()),
                Some(p.llm_model_id().as_str()),
            ]
        })
        .unwrap_or([None, None]);

    let mut rows = Vec::with_capacity(MODEL_CATALOG.len());
    for entry in MODEL_CATALOG.iter() {
        // Fast path: file-existence only, no SHA256. A same-size swap after install is NOT detected at runtime [TD-10]; corruption
        // lazily before the model is actually used (check_status in STT/LLM init).
        let status = models::check_status_fast(&state.app_data_dir, entry.id.as_str()).await?;
        rows.push(StorageRow {
            id: entry.id.as_str(),
            kind: entry.kind,
            display_name: entry.display_name,
            size_bytes: entry.size_bytes,
            status,
            last_used_at: usage.get(entry.id.as_str()).cloned(),
            is_active: active_ids.iter().any(|a| *a == Some(entry.id.as_str())),
        });
    }
    Ok(rows)
}

#[tauri::command]
pub async fn local_engine_model_download(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<(), AppError> {
    models::download(&state.app_data_dir, &id, Some(&app)).await?;
    Ok(())
}

#[tauri::command]
pub async fn local_engine_model_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    models::delete(&state.app_data_dir, &id).await
}

#[tauri::command]
pub async fn local_engine_get_active_preset(
    state: State<'_, AppState>,
) -> Result<Option<PresetSpec>, AppError> {
    let raw = crate::db::get_setting(&state.db, SETTING_ACTIVE_PRESET).await?;
    Ok(raw
        .as_deref()
        .and_then(LocalEnginePreset::from_str)
        .map(PresetSpec::from))
}

#[tauri::command]
pub async fn local_engine_set_active_preset(
    state: State<'_, AppState>,
    preset: String,
) -> Result<PresetSpec, AppError> {
    let parsed = LocalEnginePreset::from_str(&preset)
        .ok_or_else(|| AppError::Other(format!("unknown preset: {preset}")))?;
    crate::db::set_setting(&state.db, SETTING_ACTIVE_PRESET, parsed.as_str()).await?;
    Ok(PresetSpec::from(parsed))
}

/// [M12.7] Hardware probe + cache. Первый вызов делает реальный probe и
/// сохраняет JSON в settings; последующие отдают из кэша (UI hint: «обновить»
/// доступен через `force=true`).
#[tauri::command]
pub async fn local_engine_hw_probe(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<HwReport, AppError> {
    if !force.unwrap_or(false) {
        if let Some(json) = crate::db::get_setting(&state.db, SETTING_HW_REPORT).await? {
            if let Ok(report) = serde_json::from_str::<HwReport>(&json) {
                return Ok(report);
            }
        }
    }
    // probe_hardware() spawns sysctl subprocesses (std::process::Command) —
    // must run on a blocking thread to avoid starving the async executor.
    let report = tokio::task::spawn_blocking(hw_probe::probe_hardware)
        .await
        .map_err(|e| AppError::Other(format!("hw_probe join: {e}")))?;
    let json = serde_json::to_string(&report)
        .map_err(|e| AppError::Other(format!("hw_report serialize: {e}")))?;
    crate::db::set_setting(&state.db, SETTING_HW_REPORT, &json).await?;
    Ok(report)
}

/// [B2] Текущее состояние тумблера «держать модель активной».
#[tauri::command]
pub async fn local_engine_get_keep_resident(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(crate::pipeline::keep_resident_enabled(&state.db).await)
}

/// [B2] Переключить резидентный режим. Пишет настройку И применяет сразу:
/// `on` → поднять resident `llama-server` (модель в RAM всю сессию),
/// `off` → погасить. Старт best-effort: если движок не local либо модель не
/// скачана — сервер не поднимется, но настройка сохранится и применится позже
/// (на старте / смене движка).
#[tauri::command]
pub async fn local_engine_set_keep_resident(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), AppError> {
    crate::db::set_setting(
        &state.db,
        crate::pipeline::SETTING_KEEP_RESIDENT,
        if enabled { "1" } else { "0" },
    )
    .await?;
    if enabled {
        let app_data_dir = state.app_data_dir.clone();
        crate::pipeline::start_resident_server(&app, &state.db, &app_data_dir).await;
    } else {
        crate::pipeline::stop_resident_server(&app).await;
    }
    Ok(())
}

/// [recap-rich Phase 3] G-Eval dev-харнесс: судья по 4 осям (coherence/
/// faithfulness/relevance/conciseness) оценивает recap.md звонка против его
/// transcript.md через локальную модель. Логирует + возвращает баллы. Для
/// объективного сравнения «стало ли информативнее» до/после prompt-правок.
#[derive(Serialize)]
pub struct RecapEvalDto {
    pub coherence: u8,
    pub faithfulness: u8,
    pub relevance: u8,
    pub conciseness: u8,
    pub average: f32,
    pub justification: String,
}

#[tauri::command]
pub async fn local_engine_eval_recap(
    app: AppHandle,
    state: State<'_, AppState>,
    call_id: String,
) -> Result<RecapEvalDto, AppError> {
    // [TD-05] Раньше здесь был ручной `app_data_dir.join("calls").join(&call_id)`
    // мимо CallStore — то есть чтение произвольного recap.md/transcript.md в ФС
    // по `call_id = "../../.."`. Теперь id валидируется, а путь строит store.
    let parsed_id = crate::call_id::CallId::parse(&call_id)?;
    let recap = state
        .store
        .read_artifact(&parsed_id, crate::call_store::ArtifactKind::Recap)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("recap.md для звонка {call_id}")))?;
    let transcript = state
        .store
        .read_artifact(&parsed_id, crate::call_store::ArtifactKind::Transcript)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("transcript.md для звонка {call_id}")))?;
    let call = crate::db::get_call(&state.db, &call_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("call {call_id}")))?;
    let s = crate::pipeline::settings::PipelineSettings::load(&state.db).await?;
    let (provider, _preset) =
        crate::pipeline::build_local_llm_provider(&state.db, &state.app_data_dir, &app, &s).await?;
    // [Q] call_id → LLM-очередь.
    let provider = provider.with_call(call_id.clone());
    let scores = crate::pipeline::g_eval::evaluate_summary(
        &provider,
        &transcript,
        &serde_json::Value::String(recap),
        call.lang_detected.as_deref(),
    )
    .await?;
    log::info!(
        "g-eval {call_id}: coherence={} faithfulness={} relevance={} conciseness={} avg={:.2} — {}",
        scores.coherence,
        scores.faithfulness,
        scores.relevance,
        scores.conciseness,
        scores.average(),
        scores.justification,
    );
    Ok(RecapEvalDto {
        coherence: scores.coherence,
        faithfulness: scores.faithfulness,
        relevance: scores.relevance,
        conciseness: scores.conciseness,
        average: scores.average(),
        justification: scores.justification,
    })
}
