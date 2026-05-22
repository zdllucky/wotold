use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::{AudioCapture, CaptureError, CaptureResult};
use crate::{events::EventBus, AppError};

const SIDECAR_NAME: &str = "wotold-audio";
const START_TIMEOUT_SECS: u64 = 5;
const STOP_TIMEOUT_SECS: u64 = 10;

/// [B14] Tauri event payload — каждые 100ms эмитится `audio:level` пока запись
/// идёт. mic/system нормализованы 0..1.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct LevelPayload {
    pub mic: f32,
    pub system: f32,
}

/// Активная сессия записи macOS-аудио. Хранит дескриптор sidecar-процесса +
/// terminal_rx (oneshot из background dispatcher task), который завершится с
/// `stopped` либо `error` Value.
pub struct RecordingSession {
    pub call_id: String,
    pub mic_path: PathBuf,
    pub system_path: PathBuf,
    pub started_at: chrono::DateTime<chrono::Utc>,
    child: CommandChild,
    /// Resolved когда dispatcher получит "stopped" или "error" event.
    terminal_rx: oneshot::Receiver<Value>,
    /// Удерживаем handle чтобы task жил столько, сколько и session.
    /// Drop при stop / error cancels dispatcher (если ещё работает).
    _dispatcher: JoinHandle<()>,
}

#[derive(Debug, Clone, Copy)]
pub struct StopResult {
    pub duration_sec: f64,
    // [B16] Сохраняем размеры файлов для будущей quota-аналитики и диагностики
    // неполной записи. Сейчас not read — но это legitimate metadata.
    #[allow(dead_code)]
    pub mic_bytes: u64,
    #[allow(dead_code)]
    pub system_bytes: u64,
}

/// Спавнит wotold-audio sidecar, шлёт start-команду, ждёт `started` с таймаутом.
/// После started — spawn'ит background dispatcher что эмитит Tauri-события для
/// frontend (`audio:level`) пока не придёт terminal event.
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
        Some("started") => {
            // [B14] Spawn dispatcher что owns rx, forwards level/passthrough
            // events to frontend, и сигналит terminal через oneshot.
            let (terminal_tx, terminal_rx) = oneshot::channel::<Value>();
            let app_clone = app.clone();
            let dispatcher = tokio::spawn(async move {
                run_dispatcher(rx, app_clone, terminal_tx).await;
            });

            Ok(RecordingSession {
                call_id,
                mic_path,
                system_path,
                started_at: chrono::Utc::now(),
                child,
                terminal_rx,
                _dispatcher: dispatcher,
            })
        }
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

/// Отправляет stop команду, ждёт `stopped` через terminal channel, закрывает sidecar.
/// [M13.1.2] Атомарный chunk-rotation: close current WAV + open new ones.
/// Sidecar emit'ит `rotated` event обратно через dispatcher, который
/// конвертирует его в Tauri webview event `audio:rotated` для orchestrator.
///
/// Phase 1 foundation: fire-and-forget — НЕ await'аем ack. Phase 1.5 добавит
/// proper rotate_pending channel в session для синхронной верификации что
/// файл закрыт перед enqueue'ом pipeline-job'а. Сейчас orchestrator должен
/// слушать `audio:rotated` event на Tauri side.
#[allow(dead_code)]
pub async fn rotate(
    session: &mut RecordingSession,
    next_mic_path: PathBuf,
    next_system_path: PathBuf,
) -> Result<(), AppError> {
    let cmd = serde_json::json!({
        "cmd": "rotate",
        "next_mic_path": next_mic_path.to_string_lossy(),
        "next_system_path": next_system_path.to_string_lossy(),
    })
    .to_string()
        + "\n";
    session
        .child
        .write(cmd.as_bytes())
        .map_err(|e| AppError::Other(format!("sidecar rotate write failed: {e}")))?;
    // Phase 1: ack приходит через dispatcher → audio:rotated Tauri event.
    Ok(())
}

pub async fn stop(mut session: RecordingSession) -> Result<StopResult, AppError> {
    let stop_cmd = b"{\"cmd\":\"stop\"}\n";
    session
        .child
        .write(stop_cmd)
        .map_err(|e| AppError::Other(format!("sidecar write failed: {e}")))?;

    let event = tokio::time::timeout(
        Duration::from_secs(STOP_TIMEOUT_SECS),
        &mut session.terminal_rx,
    )
    .await
    .map_err(|_| AppError::Other(format!("audio stop timed out after {STOP_TIMEOUT_SECS}s")))?
    .map_err(|_| AppError::Other("audio sidecar exited without stopped event".into()))?;

    // После stop sidecar выходит когда мы закрываем stdin (drop child).
    drop(session.child);

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

/// [B14] Background dispatcher — consumes sidecar's rx после `started`. Эмитит
/// в Tauri webview `audio:level` события для frontend meter, остальные
/// passthrough логирует. При `stopped`/`error` шлёт Value в terminal_tx и
/// завершается.
async fn run_dispatcher(
    mut rx: Receiver<CommandEvent>,
    app: AppHandle,
    terminal_tx: oneshot::Sender<Value>,
) {
    let mut terminal_tx = Some(terminal_tx);
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                let Ok(json) = serde_json::from_str::<Value>(line.trim()) else {
                    log::debug!("audio non-json stdout: {line}");
                    continue;
                };
                let ev = json.get("event").and_then(Value::as_str).unwrap_or("");
                match ev {
                    "level" => {
                        let payload = LevelPayload {
                            mic: json.get("mic").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                            system: json.get("system").and_then(Value::as_f64).unwrap_or(0.0)
                                as f32,
                        };
                        EventBus::new(Some(&app)).audio_level(&payload);
                    }
                    "rotated" => {
                        // [M13.1.2] Sidecar закрыл предыдущий chunk WAV и открыл
                        // новый. Эмитим Tauri webview event чтобы orchestrator
                        // (frontend или Rust-side listener) enqueue'ил pipeline
                        // job на закрытый chunk файл.
                        EventBus::new(Some(&app)).audio_rotated(&json);
                    }
                    "stopped" | "error" => {
                        if let Some(tx) = terminal_tx.take() {
                            let _ = tx.send(json);
                        }
                        return;
                    }
                    _ => {
                        log::debug!("audio passthrough event: {json}");
                    }
                }
            }
            CommandEvent::Stderr(bytes) => {
                log::warn!("audio stderr: {}", String::from_utf8_lossy(&bytes));
            }
            CommandEvent::Terminated(payload) => {
                log::warn!("audio sidecar terminated: {payload:?}");
                if let Some(tx) = terminal_tx.take() {
                    let _ = tx.send(serde_json::json!({
                        "event": "error",
                        "message": "sidecar terminated unexpectedly"
                    }));
                }
                return;
            }
            _ => {}
        }
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
