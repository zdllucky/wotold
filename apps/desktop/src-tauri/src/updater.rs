use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::AppError;

/// Аварийный downgrade-режим (M11.8 паспорта). Активируется env-переменной
/// в момент запуска, в UI не выставляется. Используется только при выкатке
/// исправленного `latest.json` с меньшим semver для отката плохого релиза.
const ALLOW_DOWNGRADE_ENV: &str = "WOTOLD_UPDATER_ALLOW_DOWNGRADE";

/// Сравнение версий: по умолчанию обновляемся только вверх. При выставленной
/// env `WOTOLD_UPDATER_ALLOW_DOWNGRADE` — берём любую версию, не равную текущей
/// (аварийный откат).
pub fn compare_versions(
    current: semver::Version,
    release: tauri_plugin_updater::RemoteRelease,
) -> bool {
    if std::env::var(ALLOW_DOWNGRADE_ENV).is_ok() {
        release.version != current
    } else {
        release.version > current
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct AvailableUpdate {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
}

/// Неблокирующая проверка (M11.4). Дёргается из фронта при старте.
pub async fn check(app: &AppHandle) -> Result<Option<AvailableUpdate>, AppError> {
    let updater = app
        .updater()
        .map_err(|e| AppError::Other(format!("updater not configured: {e}")))?;

    let update = updater
        .check()
        .await
        .map_err(|e| AppError::Other(format!("update check failed: {e}")))?;

    Ok(update.map(|u| AvailableUpdate {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone(),
        pub_date: u.date.map(|d| d.to_string()),
    }))
}

/// Скачать и поставить апдейт, затем перезапуск. Никогда не возвращается при успехе
/// (`app.restart()` завершает процесс).
pub async fn apply(app: &AppHandle) -> Result<(), AppError> {
    let updater = app
        .updater()
        .map_err(|e| AppError::Other(format!("updater not configured: {e}")))?;

    let update = updater
        .check()
        .await
        .map_err(|e| AppError::Other(format!("update check failed: {e}")))?
        .ok_or_else(|| AppError::Other("no update available".into()))?;

    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| AppError::Other(format!("update install failed: {e}")))?;

    app.restart();
}
