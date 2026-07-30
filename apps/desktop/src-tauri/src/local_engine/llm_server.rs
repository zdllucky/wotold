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
/// [TD-08] Порт больше не фиксирован. Раньше это был константный 47331, и
/// чужой процесс, занявший его раньше нас и отвечающий на `/health`,
/// становился «нашим сервером» — то есть получал все промпты с транскриптами.
/// Теперь порт берётся эфемерным на старте, а принадлежность сервера
/// подтверждается аутентифицированным запросом (см. `verify_is_ours`).
fn pick_ephemeral_port() -> Result<u16, AppError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AppError::Other(format!("llama-server: не нашли свободный порт: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::Other(format!("llama-server: local_addr: {e}")))?
        .port();
    drop(listener);
    Ok(port)
}

/// [TD-08] Случайный ключ доступа к серверу — 32 hex-символа.
///
/// Передаётся **через env `LLAMA_API_KEY`, а не через `--api-key`**: аргументы
/// командной строки видны в `ps aux` любому пользователю машины, то есть
/// секрет, которым мы закрываем «любой локальный процесс», сам был бы этому
/// процессу доступен. Env читается только тем же UID.
fn generate_api_key() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
/// Ctx сервера. Совпадает с `llm::DEFAULT_CTX_SIZE` (8192) — parity с one-shot.
const CTX_SIZE: u32 = 8192;
/// Timeout ожидания `/health`. Включает Metal shader compile (~30с) + загрузку
/// модели на первом старте после апгрейда бинаря.
const HEALTH_TIMEOUT_SECS: u64 = 180;

/// Живой resident-сервер: дочерний процесс + его URL + preset (модель).
pub struct LlamaServer {
    /// `Option` ради `Drop`: `shutdown()` забирает child, чтобы Drop не убивал
    /// уже убитый процесс.
    child: Option<CommandChild>,
    preset: LocalEnginePreset,
    url: String,
    api_key: String,
}

/// [B28.3] PID-файл сервера. При force-quit/kill приложения дочерний
/// llama-server осиротевает и держит порт — все последующие старты падают
/// «/health timeout» → резидентная LLM мертва до ручного вмешательства
/// (живой кейс 23.07: сирота пережила остановку, три llama-server подряд не
/// поднялись). Перед spawn читаем pidfile и добиваем сироту (только если
/// процесс с этим PID — действительно наш sidecar по имени).
const PID_FILE: &str = "llama-server.pid";

#[cfg(unix)]
fn kill_stale_server(app_data_dir: &Path) {
    let pid_path = app_data_dir.join(PID_FILE);
    let Some(pid) = std::fs::read_to_string(&pid_path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    else {
        return;
    };
    // Имя процесса обязано совпадать с нашим сайдкаром — чужие PID не трогаем
    // (PID мог быть переиспользован системой).
    let comm = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if comm.ends_with(SERVER_SIDECAR) {
        log::warn!("llama-server: убиваем сироту pid={pid} с прошлой сессии (держит порт)");
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
    let _ = std::fs::remove_file(&pid_path);
}

#[cfg(not(unix))]
fn kill_stale_server(_app_data_dir: &Path) {}

/// Аргументы запуска сервера. Вынесены чистой функцией, чтобы список можно было
/// проверить тестом — спавн сайдкара юнитом не покрыть (нужен `AppHandle`).
///
/// # Про `capabilities/default.json`
///
/// Список аргументов там — граница для **webview**, не для нас: валидация
/// scope живёт в JS-команде `plugin:shell|execute`, а `Shell::sidecar()` из
/// Rust идёт мимо неё. Поэтому переменная длина (черновая модель опциональна)
/// нашему спавну ничего не ломает, но пара `--model-draft` в allowlist всё
/// равно нужна: без неё скомпрометированный webview не смог бы поднять сервер
/// в той же конфигурации, что и мы, и список перестал бы описывать реальность.
fn build_server_args<'a>(
    model: &'a str,
    port: &'a str,
    ctx: &'a str,
    draft: Option<&'a str>,
) -> Vec<&'a str> {
    let mut args = vec![
        "-m",
        model,
        "--host",
        "127.0.0.1",
        "--port",
        port,
        "--ctx-size",
        ctx,
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
    ];
    if let Some(d) = draft {
        args.push("--model-draft");
        args.push(d);
    }
    args
}

impl LlamaServer {
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn preset(&self) -> LocalEnginePreset {
        self.preset
    }
    pub fn api_key(&self) -> &str {
        &self.api_key
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
        // [B28.3] Сирота с прошлой сессии держит порт → добиваем до spawn.
        kill_stale_server(app_data_dir);

        let port = pick_ephemeral_port()?;
        let port_str = port.to_string();
        let api_key = generate_api_key();
        let ctx_str = CTX_SIZE.to_string();

        // Спекулятивное декодирование: черновая модель предлагает токены,
        // целевая их подтверждает пачкой. Резидентный сервер этого аргумента
        // не получал вовсе — он был только у one-shot `llama-cli`, поэтому с
        // включённым «держать модель активной» ускоритель был пустышкой.
        let draft_path =
            super::models::model_path(app_data_dir, super::models::ModelId::QWEN25_0_5B.as_str());
        let draft_str = draft_path.to_string_lossy().to_string();
        let args = build_server_args(
            &model_str,
            &port_str,
            &ctx_str,
            draft_path.exists().then_some(draft_str.as_str()),
        );
        if !draft_path.exists() {
            log::warn!(
                "llama-server: черновой модели нет ({}) — работаем без ускорения",
                draft_path.display()
            );
        }

        let sidecar = app
            .shell()
            .sidecar(SERVER_SIDECAR)
            .map_err(|e| AppError::Other(format!("llama-server sidecar lookup: {e}")))?
            .env("DYLD_FALLBACK_LIBRARY_PATH", "/opt/homebrew/lib")
            // [TD-08] Ключ через env, не через args — см. `generate_api_key`.
            .env("LLAMA_API_KEY", &api_key)
            .args(&args);
        let (mut rx, child) = sidecar
            .spawn()
            .map_err(|e| AppError::Other(format!("llama-server spawn: {e}")))?;

        // [B28.3] PID нового сервера — чтобы следующий старт мог добить
        // сироту, если этот процесс переживёт приложение.
        if let Err(e) = std::fs::write(app_data_dir.join(PID_FILE), child.pid().to_string()) {
            log::warn!("llama-server: pidfile write failed: {e}");
        }

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

        let url = format!("http://127.0.0.1:{port}");
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
                            // [TD-08] `/health` у llama.cpp публичен, поэтому
                            // «поднялся» ещё не значит «это наш процесс».
                            // Подтверждаем принадлежность аутентифицированным
                            // запросом: чужой сервер нашего ключа не знает.
                            if !verify_is_ours(&client, &url, &api_key).await {
                                let _ = child.kill();
                                return Err(AppError::Other(
                                    "llama-server: порт занят чужим процессом (auth-проверка не прошла)"
                                        .into(),
                                ));
                            }
                            log::info!(
                                "llama-server ready (preset={preset:?}) за {}s на {url}",
                                started.elapsed().as_secs()
                            );
                            return Ok(Self {
                                child: Some(child),
                                preset,
                                url,
                                api_key,
                            });
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Остановить сервер (kill процесса). Consuming — handle больше не валиден.
    pub fn shutdown(mut self) {
        if let Some(child) = self.child.take() {
            let _ = child.kill();
            log::info!("llama-server stopped");
        }
    }
}

/// [TD-08] Аутентифицированная проверка «это наш сервер». `/props` требует
/// ключ (в отличие от публичного `/health`), поэтому 200 здесь означает, что
/// на порту действительно процесс, которому мы этот ключ передали.
async fn verify_is_ours(client: &reqwest::Client, url: &str, api_key: &str) -> bool {
    match client
        .get(format!("{url}/props"))
        .bearer_auth(api_key)
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(e) => {
            log::warn!("llama-server: auth-проверка не удалась: {e}");
            false
        }
    }
}

/// [TD-08] Раньше дропнутый handle оставлял процесс жить — убивал только явный
/// `shutdown()`. Паттерн взят из `local_engine::sidecar::SidecarGuard`.
impl Drop for LlamaServer {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            let _ = child.kill();
            log::info!("llama-server stopped (drop)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // [TD-08] Спавн сайдкара юнитом не покрыть (нужен AppHandle), поэтому
    // тестируем вынесенные чистые части — как в TD-06 с classify_event.

    #[test]
    fn api_key_is_32_hex_chars() {
        let key = generate_api_key();
        assert_eq!(key.len(), 32, "uuid simple = 32 символа");
        assert!(
            key.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "ожидали lowercase hex, получили {key}"
        );
    }

    #[test]
    fn api_keys_differ_between_servers() {
        // Ключ на сессию: два старта не должны делить секрет.
        let a = generate_api_key();
        let b = generate_api_key();
        assert_ne!(a, b);
    }

    #[test]
    fn ephemeral_port_is_usable_after_release() {
        // Порт отдаётся уже освобождённым — сайдкар должен суметь его занять.
        let port = pick_ephemeral_port().expect("свободный порт");
        assert_ne!(port, 0);
        let listener = std::net::TcpListener::bind(("127.0.0.1", port))
            .expect("порт обязан быть свободен после drop листенера");
        drop(listener);
    }

    #[test]
    fn ephemeral_ports_are_not_the_old_fixed_one() {
        // Регрессия: раньше порт был константой 47331, и чужой процесс мог
        // занять его заранее, чтобы получать наши промпты с транскриптами.
        let ports: Vec<u16> = (0..5).filter_map(|_| pick_ephemeral_port().ok()).collect();
        assert_eq!(ports.len(), 5);
        assert!(
            ports.iter().all(|&p| p != 47331),
            "порт не должен быть прежней константой"
        );
    }

    /// Резидентный сервер обязан получать черновую модель. Раньше аргумент был
    /// только у one-shot `llama-cli`, поэтому с включённым «держать модель
    /// активной» ускорение генерации не работало вообще.
    #[test]
    fn server_args_carry_the_draft_model_when_it_is_on_disk() {
        let args = build_server_args("/m/target.bin", "1234", "8192", Some("/m/draft.bin"));
        let pos = args
            .iter()
            .position(|a| *a == "--model-draft")
            .expect("аргумент черновой модели обязан быть в списке");
        assert_eq!(args.get(pos + 1), Some(&"/m/draft.bin"));
        // Пара уходит в конец — тем же порядком, что перечислен в
        // `capabilities/default.json`, чтобы список там описывал реальность.
        assert_eq!(args[args.len() - 2], "--model-draft");
    }

    #[test]
    fn server_args_omit_the_draft_model_when_it_is_absent() {
        let args = build_server_args("/m/target.bin", "1234", "8192", None);
        assert!(!args.contains(&"--model-draft"));
        assert_eq!(args[0], "-m");
        assert_eq!(args[1], "/m/target.bin");
        assert_eq!(args[args.len() - 2], "--parallel");
    }
}
