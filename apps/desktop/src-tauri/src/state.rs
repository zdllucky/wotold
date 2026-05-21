use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

use crate::{
    audio::{call_detect::CallDetectHandle, macos::RecordingSession},
    call_store::CallStore,
    db, device, AppError,
};

pub struct AppState {
    pub db: SqlitePool,
    pub device_id: Arc<str>,
    pub app_data_dir: PathBuf,
    /// [Phase 4 R10] Filesystem-репо для `calls/<id>/*` артефактов. Все
    /// callsite'ы, которые раньше делали `app_data_dir.join("calls").join(...)`,
    /// теперь идут через `state.store.xxx(...)`. Cheap to clone (Arc).
    pub store: Arc<CallStore>,
    pub recording: Arc<Mutex<Option<RecordingSession>>>,
    // [B16 audit P0]: храним JoinHandle от pipeline tasks per-call_id, чтобы
    // при shutdown окна можно было ждать завершения (или хотя бы знать какие
    // pipeline-ы ещё бегут). До этого spawn-handle dropped → race на shutdown.
    pub pipeline_tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    /// [S2] Single-instance controller для probe (Core Audio + NSWorkspace).
    /// Идемпотентный enable/disable через `audio::call_detect::CallDetectController`.
    pub call_detect: CallDetectHandle,
}

pub async fn init(app: AppHandle) -> Result<AppState, AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Init(format!("app_data_dir: {e}")))?;
    tokio::fs::create_dir_all(&app_data_dir).await?;

    let device_id = device::ensure_device_id(&app_data_dir).await?;
    log::info!("device id: {device_id}");

    let pool = db::init(&app_data_dir).await?;
    let owner = db::ensure_owner_contact(&pool).await?;
    log::info!("owner contact: {}", owner.id);

    // Подметаем зависшие recording/processing с прошлой сессии (краш,
    // force-quit). Альтернатива была бы попытка резюмировать pipeline,
    // но raw_stt.json не гарантирован — проще пометить failed чтобы
    // юзер видел чёткое состояние.
    let swept = db::sweep_stale_calls(&pool).await?;
    if swept > 0 {
        log::warn!("sweep_stale_calls: {swept} зависших звонков → failed");
    }

    let store = Arc::new(CallStore::new(app_data_dir.clone()));

    Ok(AppState {
        db: pool,
        device_id: Arc::from(device_id.as_str()),
        app_data_dir,
        store,
        recording: Arc::new(Mutex::new(None)),
        pipeline_tasks: Arc::new(Mutex::new(HashMap::new())),
        call_detect: Arc::new(crate::audio::call_detect::CallDetectController::new()),
    })
}
