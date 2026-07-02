use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::{
    audio::{call_detect::CallDetectHandle, macos::RecordingSession},
    call_store::CallStore,
    db, device,
    pipeline::chunk_orchestrator::OrchestratorSummary,
    AppError,
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
    /// [M13.1.5c] Handle активного chunk_orchestrator (если CHUNKED_PIPELINE=ON
    /// и engine=local на момент start_recording). None в happy path.
    /// Orchestrator умирает natural'но когда sidecar terminates (rms_rx закрывается).
    /// stop_recording просто делает `take()` — `await` не нужен.
    pub orchestrator: Arc<Mutex<Option<JoinHandle<OrchestratorSummary>>>>,
    /// [M13.2.1] Sender для pause/resume сигналов в активный orchestrator
    /// (`true` = pause, `false` = resume). `None` если orchestrator не
    /// запущен. Cleared в stop_recording одновременно с handle'ом.
    /// Pause/resume Tauri commands делают `try_send` fire-and-forget.
    pub orchestrator_pause_tx: Arc<Mutex<Option<mpsc::Sender<bool>>>>,
    /// [M13 review fix] Sender oneshot stop-сигнала для orchestrator. Если бы
    /// мы оставили `stop_tx` в локальной переменной `spawn_orchestrator`, она
    /// бы дропалась при возврате функции → `stop_rx` сразу видит closed канал
    /// и orchestrator exit'ил преждевременно. Храним в AppState чтобы tx
    /// жил столько же сколько recording session. `stop_recording` делает
    /// `take()` — sender дропается, orchestrator корректно exit'ит на
    /// `stop_rx` arm.
    pub orchestrator_stop_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    /// [Bulk recap] Cancel-флаг для массового пересоздания пустых рекапов.
    /// `regenerate_empty_recaps` проверяет его между звонками; `cancel_bulk_recap`
    /// взводит. Sequential по природе (local LLM semaphore=1).
    pub bulk_recap_cancel: Arc<std::sync::atomic::AtomicBool>,
    /// [B2] Живой resident `llama-server` (настройка `local_engine.keep_resident`).
    /// `Some` пока модель держится в RAM всю сессию; `None` — one-shot режим.
    /// Поднимается на старте / по тумблеру, гасится на выходе / смене preset.
    #[cfg(target_os = "macos")]
    pub llm_server: Arc<Mutex<Option<crate::local_engine::llm_server::LlamaServer>>>,
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

    // Подметаем зависшие 'processing' с прошлой сессии (краш, force-quit) →
    // 'failed' (есть финализированное аудио, юзер сможет переобработать).
    // Орфан-'recording' обрабатываются ниже в reconcile_orphan_recordings.
    let swept = db::sweep_stale_calls(&pool).await?;
    if swept > 0 {
        log::warn!("sweep_stale_calls: {swept} зависших звонков → failed");
    }

    // [Settings UX rework] BYO больше не доступен в UI. Существующих BYO-users
    // мигрируем в managed (ключи в keychain не трогаем — могут пригодиться
    // если BYO вернём). Идемпотентно.
    migrate_byo_to_managed(&pool).await?;

    let store = Arc::new(CallStore::new(app_data_dir.clone()));

    // [B19.6] Прерванные записи (орфан-'recording'): <30с → удалить, ≥30с → failed.
    // Startup продолжается даже при ошибке reconcile (app должен подняться);
    // error-level, т.к. это сбой startup-задачи, а не штатный warn.
    match crate::commands::recording::reconcile_orphan_recordings(&pool, &store).await {
        Ok(n) if n > 0 => log::warn!("reconcile_orphan_recordings: {n} прерванных записей"),
        Ok(_) => {}
        Err(e) => log::error!("reconcile_orphan_recordings failed: {e}"),
    }

    Ok(AppState {
        db: pool,
        device_id: Arc::from(device_id.as_str()),
        app_data_dir,
        store,
        recording: Arc::new(Mutex::new(None)),
        pipeline_tasks: Arc::new(Mutex::new(HashMap::new())),
        call_detect: Arc::new(crate::audio::call_detect::CallDetectController::new()),
        orchestrator: Arc::new(Mutex::new(None)),
        orchestrator_pause_tx: Arc::new(Mutex::new(None)),
        orchestrator_stop_tx: Arc::new(Mutex::new(None)),
        bulk_recap_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        #[cfg(target_os = "macos")]
        llm_server: Arc::new(Mutex::new(None)),
    })
}

/// Migrate legacy BYO users to managed. Idempotent: no-op if values are already
/// `managed`/`cloud_managed`. Keychain BYO keys are intentionally not touched.
async fn migrate_byo_to_managed(pool: &SqlitePool) -> Result<(), AppError> {
    let path_updated = sqlx::query(
        "UPDATE settings SET value = 'managed' WHERE key = 'provider_path' AND value = 'byo'",
    )
    .execute(pool)
    .await
    .map_err(|e| AppError::Init(format!("migrate provider_path: {e}")))?
    .rows_affected();
    let engine_updated = sqlx::query(
        "UPDATE settings SET value = 'cloud_managed' WHERE key = 'local_engine.active' AND value = 'cloud_byo'",
    )
    .execute(pool)
    .await
    .map_err(|e| AppError::Init(format!("migrate active_engine: {e}")))?
    .rows_affected();
    if path_updated > 0 || engine_updated > 0 {
        log::info!(
            "migrate_byo_to_managed: provider_path={path_updated} active_engine={engine_updated}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::migrate_byo_to_managed;
    use crate::db;
    use crate::db::test_support::fresh_db;

    #[tokio::test]
    async fn migrate_byo_provider_path_to_managed() {
        let test_db = fresh_db().await;
        db::set_setting(&test_db.pool, "provider_path", "byo")
            .await
            .unwrap();
        migrate_byo_to_managed(&test_db.pool).await.unwrap();
        let val = db::get_setting(&test_db.pool, "provider_path")
            .await
            .unwrap();
        assert_eq!(val.as_deref(), Some("managed"));
    }

    #[tokio::test]
    async fn migrate_cloud_byo_engine_to_cloud_managed() {
        let test_db = fresh_db().await;
        db::set_setting(&test_db.pool, "local_engine.active", "cloud_byo")
            .await
            .unwrap();
        migrate_byo_to_managed(&test_db.pool).await.unwrap();
        let val = db::get_setting(&test_db.pool, "local_engine.active")
            .await
            .unwrap();
        assert_eq!(val.as_deref(), Some("cloud_managed"));
    }

    #[tokio::test]
    async fn migrate_idempotent_for_managed() {
        // Already managed/cloud_managed → no-op, value preserved.
        let test_db = fresh_db().await;
        db::set_setting(&test_db.pool, "provider_path", "managed")
            .await
            .unwrap();
        db::set_setting(&test_db.pool, "local_engine.active", "local")
            .await
            .unwrap();
        migrate_byo_to_managed(&test_db.pool).await.unwrap();
        assert_eq!(
            db::get_setting(&test_db.pool, "provider_path")
                .await
                .unwrap()
                .as_deref(),
            Some("managed")
        );
        assert_eq!(
            db::get_setting(&test_db.pool, "local_engine.active")
                .await
                .unwrap()
                .as_deref(),
            Some("local")
        );
    }

    #[tokio::test]
    async fn migrate_no_op_on_empty_settings() {
        // Empty settings → migration returns Ok without writing anything.
        let test_db = fresh_db().await;
        migrate_byo_to_managed(&test_db.pool).await.unwrap();
        assert!(db::get_setting(&test_db.pool, "provider_path")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn migrate_only_byo_values() {
        // provider_path stuck on some weird non-byo value → НЕ трогаем.
        let test_db = fresh_db().await;
        db::set_setting(&test_db.pool, "provider_path", "weird-other")
            .await
            .unwrap();
        migrate_byo_to_managed(&test_db.pool).await.unwrap();
        assert_eq!(
            db::get_setting(&test_db.pool, "provider_path")
                .await
                .unwrap()
                .as_deref(),
            Some("weird-other")
        );
    }
}
