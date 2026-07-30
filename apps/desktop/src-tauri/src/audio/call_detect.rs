//! [S2/S3] Auto-detect "user joined a call/meeting" → suggest recording.
//!
//! R3 deviation passport: настройка opt-in, default OFF. См. `CLAUDE.md` R3.
//!
//! Поднимает долгоживущий wotold-audio sidecar в режиме `call_detect_start`,
//! слушает NDJSON `call_suggested` события, применяет cooldown per-app
//! (in-memory HashMap, рестарт обнуляет), затем эмитит typed Tauri event
//! `recording:suggested` для frontend и нативного уведомления (S4/S5).
//!
//! Чтобы не суетиться, пока сами пишем — controller проверяет
//! `state.recording` перед стартом probe и автоматически перезапускает
//! probe после `recording:stopped`. Внутри pipeline пишущего звонка mic
//! всё равно busy, и Core Audio probe не разрулит "это мы" vs "это они" —
//! проще выключить probe на время записи.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::Mutex;

use crate::state::AppState;
use crate::AppError;

const SIDECAR_NAME: &str = "wotold-audio";
const START_TIMEOUT_SECS: u64 = 5;
pub const RECORDING_SUGGESTED_EVENT: &str = "recording:suggested";

/// Payload Tauri-события `recording:suggested`. Frontend (S5) / native
/// notification (S4) подписываются на этот канал.
#[derive(Debug, Clone, Serialize)]
pub struct RecordingSuggestedEvent {
    pub bundle_id: String,
    pub app_name: String,
    pub reason: String,
}

/// Singleton-контроллер probe. Живёт в `AppState::call_detect`. Все мутации
/// идут через `tokio::Mutex` чтобы start/stop были serial.
pub struct CallDetectController {
    inner: Mutex<Inner>,
}

struct Inner {
    /// Дескриптор активного sidecar-процесса (None = probe off).
    child: Option<CommandChild>,
    /// Handle dispatcher task. Drop = task cancelled (поэтому stop кладёт None).
    handle: Option<JoinHandle<()>>,
    /// Per-bundle-id cooldown. Хранится Instant'ом — старая запись пропускает
    /// сравнение если её timestamp < now() − cooldown.
    cooldown: HashMap<String, Instant>,
    /// Cooldown в минутах (read from settings on enable).
    cooldown_min: u64,
}

impl CallDetectController {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                child: None,
                handle: None,
                cooldown: HashMap::new(),
                cooldown_min: 5,
            }),
        }
    }

    pub async fn is_enabled(&self) -> bool {
        self.inner.lock().await.child.is_some()
    }

    /// Стартует probe. Идемпотентно: если уже запущен — no-op.
    pub async fn enable(&self, app: AppHandle, cooldown_min: u64) -> Result<(), AppError> {
        let mut inner = self.inner.lock().await;
        if inner.child.is_some() {
            inner.cooldown_min = cooldown_min;
            return Ok(());
        }

        let sidecar = app
            .shell()
            .sidecar(SIDECAR_NAME)
            .map_err(|e| AppError::Other(format!("call-detect sidecar lookup: {e}")))?;

        let (mut rx, mut child) = sidecar
            .spawn()
            .map_err(|e| AppError::Other(format!("call-detect sidecar spawn: {e}")))?;

        let cmd = b"{\"cmd\":\"call_detect_start\"}\n";
        child
            .write(cmd)
            .map_err(|e| AppError::Other(format!("call-detect sidecar write: {e}")))?;

        // Ждём `call_detect_started` ack — гарантирует что probe внутри
        // sidecar'а реально подцепил Core Audio + NSWorkspace observer'ы.
        let ack = tokio::time::timeout(Duration::from_secs(START_TIMEOUT_SECS), async {
            while let Some(event) = rx.recv().await {
                if let CommandEvent::Stdout(bytes) = event {
                    let line = String::from_utf8_lossy(&bytes);
                    if let Ok(json) = serde_json::from_str::<Value>(line.trim()) {
                        if json.get("event").and_then(Value::as_str) == Some("call_detect_started")
                        {
                            return Some(json);
                        }
                    }
                }
            }
            None
        })
        .await
        .map_err(|_| {
            AppError::Other(format!(
                "call-detect probe ack timed out after {START_TIMEOUT_SECS}s"
            ))
        })?;

        if ack.is_none() {
            let _ = child.kill();
            return Err(AppError::Other(
                "call-detect sidecar exited before ack".into(),
            ));
        }

        // Spawn dispatcher для дальнейших `call_suggested` событий.
        let app_for_task = app.clone();
        let handle = tauri::async_runtime::spawn(async move {
            run_dispatcher(rx, app_for_task).await;
        });

        inner.child = Some(child);
        inner.handle = Some(handle);
        inner.cooldown_min = cooldown_min;
        inner.cooldown.clear();
        log::info!("call-detect: probe enabled (cooldown={cooldown_min}min)");
        Ok(())
    }

    pub async fn disable(&self) -> Result<(), AppError> {
        let mut inner = self.inner.lock().await;
        let Some(mut child) = inner.child.take() else {
            return Ok(());
        };
        let _ = child.write(b"{\"cmd\":\"call_detect_stop\"}\n");
        drop(child); // sidecar exits when stdin closes
        if let Some(handle) = inner.handle.take() {
            handle.abort();
        }
        inner.cooldown.clear();
        log::info!("call-detect: probe disabled");
        Ok(())
    }

    /// Проверяет cooldown и обновляет timestamp если событие пропускаем.
    /// Возвращает `true` если событие НЕ заглушено (можно эмитить).
    async fn should_emit(&self, bundle_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let cooldown = Duration::from_secs(inner.cooldown_min * 60);
        if let Some(prev) = inner.cooldown.get(bundle_id) {
            if now.duration_since(*prev) < cooldown {
                return false;
            }
        }
        inner.cooldown.insert(bundle_id.to_string(), now);
        true
    }
}

impl Default for CallDetectController {
    fn default() -> Self {
        Self::new()
    }
}

async fn run_dispatcher(mut rx: tokio::sync::mpsc::Receiver<CommandEvent>, app: AppHandle) {
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                let Ok(json) = serde_json::from_str::<Value>(line.trim()) else {
                    log::debug!("call-detect non-json stdout: {line}");
                    continue;
                };
                let ev = json.get("event").and_then(Value::as_str).unwrap_or("");
                if ev != "call_suggested" {
                    log::debug!("call-detect passthrough: {json}");
                    continue;
                }
                let bundle_id = json
                    .get("bundle_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let app_name = json
                    .get("app_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let reason = json
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if bundle_id.is_empty() {
                    continue;
                }
                if !maybe_emit(&app, &bundle_id, &app_name, &reason).await {
                    log::debug!("call-detect suggest suppressed by cooldown: {bundle_id}");
                }
            }
            CommandEvent::Stderr(bytes) => {
                log::warn!("call-detect stderr: {}", String::from_utf8_lossy(&bytes));
            }
            CommandEvent::Terminated(payload) => {
                log::warn!("call-detect sidecar terminated: {payload:?}");
                return;
            }
            _ => {}
        }
    }
}

/// Returns `true` если эмитнули, `false` если cooldown проглотил. Также
/// блокирует эмит во время активной записи — пока мы пишем сами, не нужно
/// предлагать "стартовать запись".
async fn maybe_emit(app: &AppHandle, bundle_id: &str, app_name: &str, reason: &str) -> bool {
    let state = app.state::<AppState>();
    if state.recording.lock().await.is_some() {
        return false;
    }
    let detector = &state.call_detect;
    if !detector.should_emit(bundle_id).await {
        return false;
    }
    let payload = RecordingSuggestedEvent {
        bundle_id: bundle_id.to_string(),
        app_name: app_name.to_string(),
        reason: reason.to_string(),
    };
    if let Err(e) = app.emit(RECORDING_SUGGESTED_EVENT, &payload) {
        log::warn!("emit {RECORDING_SUGGESTED_EVENT} failed: {e}");
    }
    // [S4/T7] Нативный баннер поднимает фронт через `show_notification`:
    // строки обязаны идти через `t()` и три локали (правило 4), а здесь они
    // были русскими литералами — казах и англичанин видели русский текст.
    // Webview остаётся живым при свёрнутом окне (close-to-tray прячет, а не
    // закрывает), так что баннер доезжает и в фоне. Tauri 2 не поддерживает
    // action-кнопки в уведомлениях на macOS, поэтому действие даёт in-app
    // баннер на том же событии.
    true
}

/// Хелпер для wiring — поднимается из CallDetectController через
/// `Arc<CallDetectController>` в AppState.
pub type CallDetectHandle = Arc<CallDetectController>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fresh_inner(cooldown_min: u64) -> CallDetectController {
        CallDetectController {
            inner: Mutex::new(Inner {
                child: None,
                handle: None,
                cooldown: HashMap::new(),
                cooldown_min,
            }),
        }
    }

    #[tokio::test]
    async fn should_emit_first_call_for_bundle() {
        let ctrl = fresh_inner(5);
        assert!(ctrl.should_emit("us.zoom.xos").await);
    }

    #[tokio::test]
    async fn should_emit_suppresses_within_cooldown() {
        let ctrl = fresh_inner(5);
        assert!(ctrl.should_emit("us.zoom.xos").await);
        assert!(!ctrl.should_emit("us.zoom.xos").await);
    }

    #[tokio::test]
    async fn should_emit_independent_per_bundle() {
        let ctrl = fresh_inner(5);
        assert!(ctrl.should_emit("us.zoom.xos").await);
        assert!(ctrl.should_emit("com.microsoft.teams2").await);
        assert!(!ctrl.should_emit("us.zoom.xos").await);
    }

    #[tokio::test]
    async fn should_emit_after_cooldown_expires() {
        // Zero-cooldown даёт нам быстрый прогон без `tokio::time::pause` гимнастики.
        let ctrl = CallDetectController {
            inner: Mutex::new(Inner {
                child: None,
                handle: None,
                cooldown: HashMap::new(),
                cooldown_min: 0,
            }),
        };
        assert!(ctrl.should_emit("us.zoom.xos").await);
        // С 0-minute cooldown следующий вызов сразу проходит.
        assert!(ctrl.should_emit("us.zoom.xos").await);
    }

    #[tokio::test]
    async fn is_enabled_false_by_default() {
        let ctrl = CallDetectController::new();
        assert!(!ctrl.is_enabled().await);
    }

    #[tokio::test]
    async fn cooldown_records_distinct_bundle_timestamps() {
        let ctrl = fresh_inner(5);
        let _ = ctrl.should_emit("a").await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        let _ = ctrl.should_emit("b").await;
        let inner = ctrl.inner.lock().await;
        let a = inner.cooldown.get("a").copied().unwrap();
        let b = inner.cooldown.get("b").copied().unwrap();
        assert!(b > a);
    }
}
