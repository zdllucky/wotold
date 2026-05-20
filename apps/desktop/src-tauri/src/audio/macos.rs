use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc::Receiver;

use super::{AudioCapture, CaptureError, CaptureResult};
use crate::AppError;

const SIDECAR_NAME: &str = "wotold-audio";
const START_TIMEOUT_SECS: u64 = 5;
const STOP_TIMEOUT_SECS: u64 = 10;

/// Активная сессия записи macOS-аудио. Хранит дескриптор sidecar-процесса
/// и канал событий, чтобы stop() мог дождаться "stopped" события.
pub struct RecordingSession {
    pub call_id: String,
    pub mic_path: PathBuf,
    pub system_path: PathBuf,
    pub started_at: chrono::DateTime<chrono::Utc>,
    child: CommandChild,
    rx: Receiver<CommandEvent>,
}

#[derive(Debug, Clone, Copy)]
pub struct StopResult {
    pub duration_sec: f64,
    pub mic_bytes: u64,
    pub system_bytes: u64,
}

/// Спавнит wotold-audio sidecar, шлёт start-команду, ждёт `started` с таймаутом.
pub async fn start(
    app: &AppHandle,
    call_id: String,
    mic_path: PathBuf,
    system_path: PathBuf,
) -> Result<RecordingSession, AppError> {
    if let Some(parent) = mic_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(parent) = system_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let sidecar = app
        .shell()
        .sidecar(SIDECAR_NAME)
        .map_err(|e| AppError::Other(format!("sidecar lookup failed: {e}")))?;

    let (mut rx, mut child) = sidecar
        .spawn()
        .map_err(|e| AppError::Other(format!("sidecar spawn failed: {e}")))?;

    let cmd = serde_json::json!({
        "cmd": "start",
        "mic_path": mic_path.to_string_lossy(),
        "system_path": system_path.to_string_lossy(),
    });
    let mut line = cmd.to_string();
    line.push('\n');
    child
        .write(line.as_bytes())
        .map_err(|e| AppError::Other(format!("sidecar write failed: {e}")))?;

    let first = tokio::time::timeout(Duration::from_secs(START_TIMEOUT_SECS), async {
        wait_for_event(&mut rx, &["started", "error"]).await
    })
    .await
    .map_err(|_| AppError::Other(format!("audio start timed out after {START_TIMEOUT_SECS}s")))?;

    let first =
        first.ok_or_else(|| AppError::Other("audio sidecar exited before started event".into()))?;

    match first.get("event").and_then(Value::as_str) {
        Some("started") => Ok(RecordingSession {
            call_id,
            mic_path,
            system_path,
            started_at: chrono::Utc::now(),
            child,
            rx,
        }),
        Some("error") => {
            let msg = first
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let _ = child.kill();
            Err(AppError::Other(format!("audio start failed: {msg}")))
        }
        _ => Err(AppError::Other(format!(
            "unexpected sidecar event: {first}"
        ))),
    }
}

/// Отправляет stop команду, ждёт `stopped` с длительностью и размером, закрывает sidecar.
pub async fn stop(mut session: RecordingSession) -> Result<StopResult, AppError> {
    let stop_cmd = b"{\"cmd\":\"stop\"}\n";
    session
        .child
        .write(stop_cmd)
        .map_err(|e| AppError::Other(format!("sidecar write failed: {e}")))?;

    let result = tokio::time::timeout(Duration::from_secs(STOP_TIMEOUT_SECS), async {
        wait_for_event(&mut session.rx, &["stopped", "error"]).await
    })
    .await
    .map_err(|_| AppError::Other(format!("audio stop timed out after {STOP_TIMEOUT_SECS}s")))?;

    // После stop sidecar выходит когда мы закрываем stdin (drop child).
    drop(session.child);

    let event = result
        .ok_or_else(|| AppError::Other("audio sidecar exited without stopped event".into()))?;
    match event.get("event").and_then(Value::as_str) {
        Some("stopped") => {
            if let Some(warning) = event.get("warning").and_then(Value::as_str) {
                log::warn!("audio stop warning: {warning}");
            }
            Ok(StopResult {
                duration_sec: event
                    .get("duration_sec")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                mic_bytes: event.get("mic_bytes").and_then(Value::as_u64).unwrap_or(0),
                system_bytes: event
                    .get("system_bytes")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            })
        }
        Some("error") => {
            let msg = event
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            Err(AppError::Other(format!("audio stop failed: {msg}")))
        }
        _ => Err(AppError::Other(format!(
            "unexpected sidecar event: {event}"
        ))),
    }
}

async fn wait_for_event(rx: &mut Receiver<CommandEvent>, names: &[&str]) -> Option<Value> {
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                if let Ok(json) = serde_json::from_str::<Value>(line.trim()) {
                    let ev = json.get("event").and_then(Value::as_str).unwrap_or("");
                    if names.contains(&ev) {
                        return Some(json);
                    }
                    log::debug!("audio passthrough event: {json}");
                }
            }
            CommandEvent::Stderr(bytes) => {
                log::warn!("audio stderr: {}", String::from_utf8_lossy(&bytes));
            }
            CommandEvent::Terminated(payload) => {
                log::warn!("audio sidecar terminated: {payload:?}");
                return None;
            }
            _ => {}
        }
    }
    None
}

/// Сохраняем оригинальную stub-имплементацию trait'а для совместимости со scaffold'ом.
/// Реальный путь — модульные функции `start`/`stop`.
pub struct MacOsCoreAudioCapture;

impl MacOsCoreAudioCapture {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacOsCoreAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AudioCapture for MacOsCoreAudioCapture {
    async fn start(&self) -> Result<(), CaptureError> {
        Err(CaptureError::Other(
            "use audio::macos::start() with AppHandle instead".into(),
        ))
    }

    async fn stop(&self) -> Result<CaptureResult, CaptureError> {
        Err(CaptureError::Other(
            "use audio::macos::stop() with session instead".into(),
        ))
    }
}
