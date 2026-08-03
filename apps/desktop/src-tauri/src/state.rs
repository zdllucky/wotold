use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::{
    audio::silence_watch::SilenceWatchHandles,
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
    /// [T2/R15] Ручки наблюдателя тишины активной записи. `None` — запись не
    /// идёт либо обе настройки тишины выключены. В отличие от оркестратора
    /// наблюдатель работает и при выключенном chunked-режиме: тишину надо
    /// ловить всегда.
    pub silence_watch: Arc<Mutex<Option<SilenceWatchHandles>>>,
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

/// ENV-override каталога данных. Имя то же, что читает MCP-сервер.
const APP_DATA_DIR_ENV: &str = "WOTOLD_APP_DATA_DIR";

/// [env-split] Каталог данных: override из ENV главнее `app_data_dir()`.
///
/// Паритет с MCP (`services/mcp/src/server.ts` читает `WOTOLD_APP_DATA_DIR`):
/// override был только у одного из двух потребителей одной и той же БД, и
/// направить их на общий нештатный каталог было нечем — ровно тот перекос
/// «одинаковый контракт, разная зрелость», который запрещает правило 2
/// CLAUDE.md. Нужен для support-сценариев и прогонов на копии базы.
///
/// Значение принимается как есть: это переменная окружения собственного
/// процесса, а не вход из webview или MCP. Факт подмены логируется — иначе
/// «приложение потеряло все звонки» диагностируется вслепую.
fn resolve_app_data_dir(from_tauri: PathBuf, env_override: Option<String>) -> PathBuf {
    match env_override {
        Some(raw) if !raw.trim().is_empty() => {
            let overridden = PathBuf::from(raw.trim());
            log::warn!(
                "{APP_DATA_DIR_ENV} задан: данные читаются из {}, штатный {} не используется",
                overridden.display(),
                from_tauri.display()
            );
            overridden
        }
        _ => from_tauri,
    }
}

/// [env-split] Сборка обязана оказаться в своём каталоге — иначе не стартуем.
///
/// Разделение держится на том, что `tauri dev` идёт с оверлеем
/// `tauri.dev.conf.json`. Забыть флаг легко: `pnpm tauri dev` из мышечной
/// памяти соберёт debug с продовым identifier, и dev снова начнёт лить
/// миграции в боевую базу — то есть вернёт ровно ту поломку, ради которой
/// среды разводили. Молчать тут нельзя, поэтому сверяем последний сегмент
/// каталога с [`crate::app_env::identifier`] и падаем с инструкцией.
///
/// ENV-override снимает проверку: каталог там задан руками и называться
/// может как угодно.
fn ensure_matching_env(
    app_data_dir: &Path,
    expected_identifier: &str,
    env_overridden: bool,
) -> Result<(), AppError> {
    if env_overridden {
        return Ok(());
    }
    let actual = app_data_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if actual == expected_identifier {
        return Ok(());
    }
    Err(AppError::Init(format!(
        "каталог данных {} не принадлежит этой сборке (ожидался {expected_identifier}). \
         Dev запускается как `pnpm --filter @wotold/desktop tauri:dev` — с оверлеем \
         tauri.dev.conf.json; без него dev пишет в боевой каталог и ломает релиз.",
        app_data_dir.display()
    )))
}

pub async fn init(app: AppHandle) -> Result<AppState, AppError> {
    let env_override = std::env::var(APP_DATA_DIR_ENV).ok();
    let env_overridden = env_override
        .as_deref()
        .is_some_and(|v| !v.trim().is_empty());
    let app_data_dir = resolve_app_data_dir(
        app.path()
            .app_data_dir()
            .map_err(|e| AppError::Init(format!("app_data_dir: {e}")))?,
        env_override,
    );
    ensure_matching_env(&app_data_dir, crate::app_env::identifier(), env_overridden)?;
    tokio::fs::create_dir_all(&app_data_dir).await?;

    // Голосовой эмбеддер переехал в каталог моделей — забираем файл со старого
    // пути до первого обращения к нему (иначе апгрейд выглядит как «модуль
    // пропал» и требует повторные 26 MB). Дешёвая проверка двух путей.
    #[cfg(target_os = "macos")]
    crate::local_engine::model_migrate::migrate_legacy_voice_embedder(&app_data_dir);

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
        silence_watch: Arc::new(Mutex::new(None)),
        bulk_recap_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        #[cfg(target_os = "macos")]
        llm_server: Arc::new(Mutex::new(None)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Чистая функция вместо чтения ENV внутри теста: переменные окружения
    /// процесс-глобальны, а тесты идут параллельно — правило 6 CLAUDE.md.
    #[test]
    fn falls_back_to_tauri_dir_when_env_absent() {
        let tauri_dir = PathBuf::from("/data/app.wotold.desktop");
        assert_eq!(
            resolve_app_data_dir(tauri_dir.clone(), None),
            tauri_dir,
            "без переменной берём штатный каталог"
        );
    }

    #[test]
    fn env_override_wins_over_tauri_dir() {
        let resolved = resolve_app_data_dir(
            PathBuf::from("/data/app.wotold.desktop"),
            Some("/tmp/wotold-copy".to_string()),
        );
        assert_eq!(resolved, PathBuf::from("/tmp/wotold-copy"));
    }

    /// Пустая или пробельная переменная — не «каталог ''», а её отсутствие:
    /// иначе `WOTOLD_APP_DATA_DIR=` в шелле уводил бы данные в CWD.
    #[test]
    fn blank_env_override_is_ignored() {
        let tauri_dir = PathBuf::from("/data/app.wotold.desktop");
        for blank in ["", "   "] {
            assert_eq!(
                resolve_app_data_dir(tauri_dir.clone(), Some(blank.to_string())),
                tauri_dir,
                "пустое значение {blank:?} не должно переопределять каталог"
            );
        }
    }

    #[test]
    fn env_override_is_trimmed() {
        let resolved = resolve_app_data_dir(
            PathBuf::from("/data/app.wotold.desktop"),
            Some("  /tmp/wotold-copy  ".to_string()),
        );
        assert_eq!(resolved, PathBuf::from("/tmp/wotold-copy"));
    }

    #[test]
    fn matching_identifier_passes() {
        assert!(ensure_matching_env(
            Path::new("/data/app.wotold.desktop.dev"),
            "app.wotold.desktop.dev",
            false,
        )
        .is_ok());
    }

    /// `pnpm tauri dev` без оверлея: debug-бинарь в боевом каталоге.
    /// Это и есть возврат исходной поломки — старт обязан отказать.
    #[test]
    fn dev_build_in_prod_dir_is_rejected() {
        let err = ensure_matching_env(
            Path::new("/data/app.wotold.desktop"),
            "app.wotold.desktop.dev",
            false,
        )
        .expect_err("несовпадение каталога обязано ронять старт");
        let msg = err.to_string();
        assert!(
            msg.contains("tauri:dev"),
            "ошибка обязана назвать правильную команду, получили: {msg}"
        );
    }

    /// Обратное направление: релиз, случайно нацеленный в dev-каталог.
    #[test]
    fn prod_build_in_dev_dir_is_rejected() {
        assert!(ensure_matching_env(
            Path::new("/data/app.wotold.desktop.dev"),
            "app.wotold.desktop",
            false,
        )
        .is_err());
    }

    /// Явный ENV-override — осознанное действие: каталог может называться как
    /// угодно, проверка снимается.
    #[test]
    fn env_override_skips_identifier_check() {
        assert!(ensure_matching_env(
            Path::new("/tmp/wotold-copy"),
            "app.wotold.desktop.dev",
            true,
        )
        .is_ok());
    }
}
