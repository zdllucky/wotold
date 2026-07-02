//! [B2] Persistent `llama-server` backend — модель грузится ОДИН раз и живёт в
//! RAM всю сессию вместо one-shot `llama-cli` на каждый вызов. Включается
//! настройкой `local_engine.keep_resident` (default OFF, opt-in — модель
//! резидентно занимает ~2-5 ГБ RAM). Один recap = classifier + N map + reduce +
//! post-pass ≈ 6-7 вызовов; резидентный сервер убирает 6-7 перезагрузок модели.
//!
//! HTTP-контракт (llama.cpp server b9270): `POST /completion` c body
//! `{prompt, n_predict, temperature, repeat_penalty, json_schema?}` → `{content}`.
//! `json_schema` — per-request, потому один загруженный сервер обслуживает
//! разные схемы (classifier/map/reduce). Sampling/schema per-request; модель +
//! ctx + ngl — load-time.

use std::path::Path;
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use super::preset::LocalEnginePreset;
use crate::AppError;

const SERVER_SIDECAR: &str = "wotold-llama-server";
/// Локальный порт (bind только 127.0.0.1). Фиксированный high-range: при
/// коллизии `/health` не поднимется → `start` вернёт Err → caller фолбэкнет
/// на one-shot `llama-cli` (graceful degradation, не фатально).
pub const SERVER_PORT: u16 = 47331;
/// Ctx сервера. Совпадает с `llm::DEFAULT_CTX_SIZE` (8192) — parity с one-shot.
const CTX_SIZE: u32 = 8192;
/// Timeout ожидания `/health`. Включает Metal shader compile (~30с) + загрузку
/// модели на первом старте после апгрейда бинаря.
const HEALTH_TIMEOUT_SECS: u64 = 180;

/// Живой resident-сервер: дочерний процесс + его URL + preset (модель).
pub struct LlamaServer {
    child: CommandChild,
    preset: LocalEnginePreset,
    url: String,
}

impl LlamaServer {
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn preset(&self) -> LocalEnginePreset {
        self.preset
    }

    /// Запустить сервер с моделью `preset`'а и дождаться готовности `/health`.
    /// На любой ошибке (нет модели / spawn fail / health timeout) — Err; caller
    /// продолжит на one-shot пути.
    pub async fn start(
        app: &AppHandle,
        app_data_dir: &Path,
        preset: LocalEnginePreset,
    ) -> Result<Self, AppError> {
        let llm_id = preset.llm_model_id();
        let model_path = super::models::model_path(app_data_dir, llm_id.as_str());
        if !model_path.exists() {
            return Err(AppError::Other(format!(
                "local_engine_model_missing: {}",
                llm_id.as_str()
            )));
        }
        let model_str = model_path
            .to_str()
            .ok_or_else(|| AppError::Other("non-utf8 model path".into()))?
            .to_string();
        let port_str = SERVER_PORT.to_string();
        let ctx_str = CTX_SIZE.to_string();

        let sidecar = app
            .shell()
            .sidecar(SERVER_SIDECAR)
            .map_err(|e| AppError::Other(format!("llama-server sidecar lookup: {e}")))?
            .env("DYLD_FALLBACK_LIBRARY_PATH", "/opt/homebrew/lib")
            .args([
                "-m",
                &model_str,
                "--host",
                "127.0.0.1",
                "--port",
                &port_str,
                "--ctx-size",
                &ctx_str,
                "-ngl",
                "99",
                "-fa",
                "on",
                "-ctk",
                "q8_0",
                "-ctv",
                "q8_0",
                "--parallel",
                "1",
            ]);
        let (mut rx, child) = sidecar
            .spawn()
            .map_err(|e| AppError::Other(format!("llama-server spawn: {e}")))?;

        // Дренируем вывод сервера в лог — иначе event-канал заполнится и сервер
        // застынет на write. Только логируем (stderr = perf/загрузка).
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stderr(bytes) => {
                        let line = String::from_utf8_lossy(&bytes);
                        let t = line.trim();
                        if !t.is_empty() {
                            log::debug!("llama-server: {t}");
                        }
                    }
                    CommandEvent::Terminated(payload) => {
                        log::warn!("llama-server terminated: code={:?}", payload.code);
                        break;
                    }
                    _ => {}
                }
            }
        });

        let url = format!("http://127.0.0.1:{SERVER_PORT}");
        let client = reqwest::Client::new();
        let health = format!("{url}/health");
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() > Duration::from_secs(HEALTH_TIMEOUT_SECS) {
                let _ = child.kill();
                return Err(AppError::Other("llama-server /health timeout".into()));
            }
            if let Ok(resp) = client
                .get(&health)
                .timeout(Duration::from_secs(3))
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(j) = resp.json::<serde_json::Value>().await {
                        if j.get("status").and_then(|v| v.as_str()) == Some("ok") {
                            log::info!(
                                "llama-server ready (preset={preset:?}) за {}s на {url}",
                                started.elapsed().as_secs()
                            );
                            return Ok(Self { child, preset, url });
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Остановить сервер (kill процесса). Consuming — handle больше не валиден.
    pub fn shutdown(self) {
        let LlamaServer { child, .. } = self;
        let _ = child.kill();
        log::info!("llama-server stopped");
    }
}
