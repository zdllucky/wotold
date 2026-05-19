use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

use crate::{audio::macos::RecordingSession, db, device, AppError};

pub struct AppState {
    pub db: SqlitePool,
    pub device_id: Arc<str>,
    pub app_data_dir: PathBuf,
    pub recording: Arc<Mutex<Option<RecordingSession>>>,
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

    Ok(AppState {
        db: pool,
        device_id: Arc::from(device_id.as_str()),
        app_data_dir,
        recording: Arc::new(Mutex::new(None)),
    })
}
