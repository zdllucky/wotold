use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc::{Receiver, Sender};
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

/// [M13.1.5b] Опциональные каналы для chunk_orchestrator. Если переданы в
/// `audio::macos::start`, dispatcher фан-аутит каждый `"level"` event в
/// `rms_tx` + `"rotated"` event в `rotate_tx` (помимо обычной emit'у webview
/// событий). orchestrator owns rx-end'ы.
///
/// BC: `start()` принимает `Option<OrchestratorChannels>`, `None` сохраняет
/// текущее behavior (no fan-out, recording happy path не тронут).
#[derive(Debug, Clone)]
pub struct OrchestratorChannels {
    /// `(timestamp_ms_от_started_at, max(mic, system))` для silence_detector.
    pub rms_tx: Sender<(u64, f32)>,
    /// Raw sidecar JSON `{event:"rotated", duration_sec, mic_bytes, system_bytes}`.
    pub rotate_tx: Sender<Value>,
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
    /// [P6] Per-chunk duration от sidecar (per-rotate reset в Swift) — caller
    /// должен игнорировать это и computить total из Rust-side started_at.
    /// Поле остаётся в struct для observability / future debugging.
    #[allow(dead_code)]
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
///
/// [M13 fix] `mic_path`/`system_path` — куда sidecar **физически пишет** первый
/// chunk. `final_mic_path`/`final_system_path` — что кладётся в
/// `RecordingSession` (цель финального merge + источник non-chunked STT). При
/// chunked-режиме sidecar пишет в `chunks/0/`, а session указывает на root
/// `mic.wav`/`system.wav` (куда audio_merger склеит все chunk'и на stop). В
/// non-chunked режиме `final_* == mic_path/system_path` (root) — байт-в-байт
/// прежнее поведение.
#[allow(clippy::too_many_arguments)]
pub async fn start(
    app: &AppHandle,
    call_id: String,
    mic_path: PathBuf,
    system_path: PathBuf,
    final_mic_path: PathBuf,
    final_system_path: PathBuf,
    orchestrator: Option<OrchestratorChannels>,
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
            let started_at = chrono::Utc::now();
            // [M13.1.5b] Pass started_at into dispatcher so it can compute
            // `timestamp_ms` for orchestrator's silence_detector. Cheap copy
            // (DateTime<Utc> is Copy).
            let started_for_dispatcher = started_at;
            let dispatcher = tokio::spawn(async move {
                run_dispatcher(
                    rx,
                    app_clone,
                    terminal_tx,
                    orchestrator,
                    started_for_dispatcher,
                )
                .await;
            });

            Ok(RecordingSession {
                call_id,
                // [M13 fix] Session хранит final (root) paths — цель merge +
                // non-chunked STT source. Sidecar пишет в mic_path/system_path
                // (chunks/0/ при chunked), но это ephemeral write target.
                mic_path: final_mic_path,
                system_path: final_system_path,
                started_at,
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
///
/// [M13.1.5b] Если orchestrator каналы переданы — фан-аутит `"level"` →
/// `rms_tx` (с timestamp_ms от `started_at`) + `"rotated"` → `rotate_tx`.
/// Webview emit'ы всегда happen независимо от orchestrator state.
/// [TD-07] Остановить захват на обеих дорожках. Не завершает сессию: тап и
/// IOProc остаются живыми, просто кадры дропаются до записи в WAV.
///
/// Ack (`paused`) приходит в диспатчер и логируется как passthrough — команда
/// не ждёт его, чтобы не блокировать UI на времени round-trip'а.
pub async fn pause(session: &mut RecordingSession) -> Result<(), AppError> {
    write_cmd(session, "pause")
}

/// [TD-07] Возобновить захват.
pub async fn resume(session: &mut RecordingSession) -> Result<(), AppError> {
    write_cmd(session, "resume")
}

fn write_cmd(session: &mut RecordingSession, cmd: &str) -> Result<(), AppError> {
    let line = serde_json::json!({ "cmd": cmd }).to_string() + "\n";
    session
        .child
        .write(line.as_bytes())
        .map_err(|e| AppError::Other(format!("sidecar {cmd} write failed: {e}")))
}

/// [TD-06] Класс события сайдкара. Вынесен из `run_dispatcher` отдельной
/// чистой функцией, потому что сам диспатчер принимает `Receiver<CommandEvent>`
/// и `AppHandle` — в юнит-тесте их не сконструировать. Тестируем решение,
/// а не петлю (тот же приём, что и `plan_final_chunk`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventClass {
    /// RMS-сэмпл для UI и оркестратора.
    Level,
    /// Chunk закрыт, открыт следующий.
    Rotated,
    /// Сессия окончена (штатно или фатально) — диспатчер завершается.
    Terminal,
    /// Операционный сбой: запись ЖИВА, диспатчер обязан продолжать.
    NonFatal,
    /// Всё прочее — только в лог.
    Passthrough,
}

/// Классификация по имени события.
///
/// До TD-06 `error` был единственной ошибкой в протоколе, и диспатчер считал
/// его терминальным. Но Swift слал его же на нефатальный сбой ротации, после
/// которого тап и IOProc продолжают писать: один transient убивал часовую
/// запись при целых WAV на диске. Теперь нефатальное приходит отдельным
/// `rotate_error`.
fn classify_event(ev: &str) -> EventClass {
    match ev {
        "level" => EventClass::Level,
        "rotated" => EventClass::Rotated,
        "stopped" | "error" => EventClass::Terminal,
        "rotate_error" => EventClass::NonFatal,
        _ => EventClass::Passthrough,
    }
}

async fn run_dispatcher(
    mut rx: Receiver<CommandEvent>,
    app: AppHandle,
    terminal_tx: oneshot::Sender<Value>,
    orchestrator: Option<OrchestratorChannels>,
    started_at: chrono::DateTime<chrono::Utc>,
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
                match classify_event(ev) {
                    EventClass::Level => {
                        let payload = LevelPayload {
                            mic: json.get("mic").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                            system: json.get("system").and_then(Value::as_f64).unwrap_or(0.0)
                                as f32,
                        };
                        EventBus::new(Some(&app)).audio_level(&payload);
                        // [M13.1.5b] Fan-out к chunk_orchestrator, если активен.
                        if let Some(channels) = orchestrator.as_ref() {
                            let elapsed =
                                (chrono::Utc::now() - started_at).num_milliseconds().max(0) as u64;
                            let combined = payload.mic.max(payload.system);
                            // try_send — если orchestrator буфер полный или умер,
                            // дропаем sample, не блокируем dispatcher.
                            let _ = channels.rms_tx.try_send((elapsed, combined));
                        }
                    }
                    EventClass::Rotated => {
                        // [M13.1.2] Sidecar закрыл предыдущий chunk WAV и открыл
                        // новый. Эмитим Tauri webview event чтобы orchestrator
                        // (frontend или Rust-side listener) enqueue'ил pipeline
                        // job на закрытый chunk файл.
                        EventBus::new(Some(&app)).audio_rotated(&json);
                        if let Some(channels) = orchestrator.as_ref() {
                            // rotate event редкий (раз в 10мин), full send OK.
                            let _ = channels.rotate_tx.send(json.clone()).await;
                        }
                        // [P5.2] Live duration update: persist в DB + emit
                        // `recording:duration` event для UI. Без этого
                        // HomePage показывал stale значение для 30+ мин активных
                        // записей (duration_sec writeable только в finish_recording).
                        //
                        // [P6] Sidecar's `duration_sec` в rotated payload = только
                        // current chunk (per-rotate reset в Swift). Игнорируем,
                        // computим accumulated wall-clock из session.started_at.
                        let app_clone = app.clone();
                        tokio::spawn(async move {
                            update_duration_from_rotate(&app_clone).await;
                        });
                    }
                    EventClass::Terminal => {
                        if let Some(tx) = terminal_tx.take() {
                            let _ = tx.send(json);
                        }
                        return;
                    }
                    // [TD-06] Нефатальное: НЕ трогаем terminal_tx и НЕ выходим.
                    // Прежний `return` ронял orchestrator-сендеры вместе с
                    // задачей, из-за чего ротации прекращались навсегда, а
                    // stop() потом доставал эту ошибку из terminal_rx и метил
                    // звонок failed. Персистентный degraded-флаг для UI — TD-37.
                    EventClass::NonFatal => {
                        let leg = json.get("leg").and_then(Value::as_str).unwrap_or("?");
                        let mic_rotated = json
                            .get("mic_rotated")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let msg = json.get("message").and_then(Value::as_str).unwrap_or("");
                        log::warn!(
                            "audio degraded (запись продолжается): leg={leg} mic_rotated={mic_rotated}: {msg}"
                        );
                    }
                    EventClass::Passthrough => {
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

/// [P5.2] Helper для rotated event handler: persist live duration в DB +
/// emit `recording:duration` event.
///
/// Spawned background task — все errors логируются, не блокируют dispatcher.
/// Resolve'ит current call_id + started_at через AppState (один writer
/// recording session active).
///
/// [P6] Вычисляет accumulated wall-clock из `session.started_at` минус
/// `paused_total_ms` из DB. Sidecar's rotate payload `duration_sec` отражает
/// только current chunk (per-rotate reset в Swift) — игнорируем.
async fn update_duration_from_rotate(app: &AppHandle) {
    use tauri::Manager;

    let Some(state) = app.try_state::<crate::state::AppState>() else {
        log::warn!("recording:duration: AppState not yet initialized");
        return;
    };
    let (call_id, started_at) = {
        let guard = state.recording.lock().await;
        match guard.as_ref() {
            Some(s) => (s.call_id.clone(), s.started_at),
            None => return,
        }
    };
    let elapsed_ms = (chrono::Utc::now() - started_at).num_milliseconds().max(0);
    let paused_ms: i64 = sqlx::query_scalar("SELECT paused_total_ms FROM calls WHERE id = ?1")
        .bind(&call_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);
    let duration_sec = (elapsed_ms - paused_ms).max(0) as f64 / 1000.0;
    if let Err(e) = crate::db::update_call_duration(&state.db, &call_id, duration_sec).await {
        log::warn!("recording:duration: DB update failed for {call_id}: {e}");
        return;
    }
    EventBus::new(Some(app)).recording_duration(&crate::events::RecordingDurationEvent {
        call_id,
        duration_sec: duration_sec.round() as i64,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // [TD-06] Первые тесты в этом файле. `run_dispatcher` не тестируем — он
    // принимает Receiver<CommandEvent> и AppHandle, которые в юните не
    // сконструировать; тестируем вынесенное из него решение.

    #[test]
    fn rotate_error_is_not_terminal() {
        // Суть TD-06: до фикса `rotate_error` не существовал, а нефатальный
        // сбой ротации приезжал как `error` и убивал сессию.
        assert_eq!(classify_event("rotate_error"), EventClass::NonFatal);
        assert_ne!(classify_event("rotate_error"), EventClass::Terminal);
    }

    #[test]
    fn stopped_and_error_stay_terminal() {
        assert_eq!(classify_event("stopped"), EventClass::Terminal);
        assert_eq!(classify_event("error"), EventClass::Terminal);
    }

    #[test]
    fn level_and_rotated_are_routed() {
        assert_eq!(classify_event("level"), EventClass::Level);
        assert_eq!(classify_event("rotated"), EventClass::Rotated);
    }

    #[test]
    fn unknown_events_pass_through_without_killing_session() {
        for ev in [
            "pong",
            "started",
            "call_suggested",
            "call_detect_started",
            "",
        ] {
            assert_eq!(
                classify_event(ev),
                EventClass::Passthrough,
                "{ev} не должен быть терминальным"
            );
        }
    }
}
