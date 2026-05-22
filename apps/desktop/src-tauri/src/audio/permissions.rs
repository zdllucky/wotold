use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::AppError;

const SIDECAR_NAME: &str = "wotold-audio";
const CHECK_TIMEOUT_SECS: u64 = 5;
const REQUEST_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionsStatus {
    pub microphone: String,
    pub screen_recording: String,
    pub accessibility: String,
}

/// Запрашивает текущий статус разрешений без триггера диалогов.
pub async fn check(app: &AppHandle) -> Result<PermissionsStatus, AppError> {
    one_shot(
        app,
        serde_json::json!({"cmd": "check_permissions"}),
        CHECK_TIMEOUT_SECS,
    )
    .await
}

/// Триггерит macOS-диалоги запроса разрешений. target = "microphone" |
/// "screen_recording" | "all". После реквеста возвращает обновлённый статус.
/// Note: для Screen Recording новый статус вступает в силу только в новом
/// процессе sidecar'а (TCC quirk) — поэтому при denied → granted нужен
/// перезапуск приложения чтобы запись заработала.
pub async fn request(app: &AppHandle, target: &str) -> Result<PermissionsStatus, AppError> {
    one_shot(
        app,
        serde_json::json!({"cmd": "request_permissions", "target": target}),
        REQUEST_TIMEOUT_SECS,
    )
    .await
}

async fn one_shot(
    app: &AppHandle,
    cmd: Value,
    timeout_secs: u64,
) -> Result<PermissionsStatus, AppError> {
    let sidecar = app
        .shell()
        .sidecar(SIDECAR_NAME)
        .map_err(|e| AppError::Other(format!("sidecar lookup: {e}")))?;

    let (mut rx, mut child) = sidecar
        .spawn()
        .map_err(|e| AppError::Other(format!("sidecar spawn: {e}")))?;

    let mut line = cmd.to_string();
    line.push('\n');
    child
        .write(line.as_bytes())
        .map_err(|e| AppError::Other(format!("sidecar write: {e}")))?;

    let event = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        while let Some(event) = rx.recv().await {
            if let CommandEvent::Stdout(bytes) = event {
                let line = String::from_utf8_lossy(&bytes);
                if let Ok(json) = serde_json::from_str::<Value>(line.trim()) {
                    if json.get("event").and_then(Value::as_str) == Some("permissions") {
                        return Some(json);
                    }
                }
            }
        }
        None
    })
    .await
    .map_err(|_| AppError::Other(format!("permissions probe timed out ({timeout_secs}s)")))?;

    drop(child);

    let event =
        event.ok_or_else(|| AppError::Other("sidecar exited without permissions event".into()))?;

    Ok(PermissionsStatus {
        microphone: event
            .get("microphone")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        screen_recording: event
            .get("screen_recording")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        accessibility: event
            .get("accessibility")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
    })
}
