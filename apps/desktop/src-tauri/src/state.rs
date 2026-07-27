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
    db,
    pipeline::chunk_orchestrator::OrchestratorSummary,
    AppError,
};

pub struct AppState {
    pub db: SqlitePool,
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

    let store = Arc::new(CallStore::new(app_data_dir.clone()));

    // [B19.6] Прерванные записи (орфан-'recording'): <30с → удалить, ≥30с → failed.
    // Startup продолжается даже при ошибке reconcile (app должен подняться);
    // error-level, т.к. это сбой startup-задачи, а не штатный warn.
    match crate::commands::orphan_reconcile::reconcile_orphan_recordings(&pool, &store).await {
        Ok(n) if n > 0 => log::warn!("reconcile_orphan_recordings: {n} прерванных записей"),
        Ok(_) => {}
        Err(e) => log::error!("reconcile_orphan_recordings failed: {e}"),
    }

    // [TD-50] Каталоги удалённых звонков: аудио оставалось на диске после
    // удаления строки — место и приватность (C5). Самолечение на старте,
    // после reconcile: тот сам решает судьбу орфан-'recording' и может
    // удалить строку, каталог которой подметём здесь же.
    match crate::commands::orphan_reconcile::remove_orphan_call_dirs(&pool, &store).await {
        Ok(n) if n > 0 => log::warn!("remove_orphan_call_dirs: удалено {n} каталогов без строки"),
        Ok(_) => {}
        Err(e) => log::error!("remove_orphan_call_dirs failed: {e}"),
    }

    Ok(AppState {
        db: pool,
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
