//! [M12.3] LocalLlamaProvider — `LlmProvider` impl через llama.cpp sidecar.
//!
//! # Архитектура (O2 = sidecar)
//!
//! Биндинг через Tauri sidecar `wotold-llama` (llama.cpp `llama-cli`):
//! - Sidecar бинарь зарегистрирован в `tauri.conf.json::bundle.externalBin`
//!   и whitelisted в `capabilities/default.json::shell:allow-execute` со
//!   строгими args-валидаторами.
//! - Промпт + транскрипт сериализуются в temp-файл (избегаем stdin escaping
//!   на тысячах байт UTF-8) и передаются через `-f`.
//! - stdout читается потоково, на `Terminated` — буфер парсится: ищется
//!   первый `{...}` с балансом скобок, парсится `serde_json::Value`,
//!   валидируется обязательным набором полей (title/summary).
//! - Таймаут (`LOCAL_LLM_TIMEOUT`, 5 min per PRD §M12.3.6) обёрнут
//!   `tokio::time::timeout`; `drop(child)` убивает процесс при превышении.
//!
//! # Контракт промпта (PRD §M12.3.2-3)
//!
//! `LlmRequest.system` — отдельный от Anthropic-промпта (`LOCAL_LLM_SYSTEM_PROMPT`):
//! явные «only JSON», few-shot пример. Anthropic-промпт НЕ работает на
//! 2-7B модели (типичная грабля). Тесты — `system_prompt_*` regression.
//!
//! # Без AppHandle (tests / headless)
//!
//! `generate()` без `AppHandle` возвращает `LlmError::NotImplemented` —
//! pipeline runner всегда передаёт реальный handle; unit-тесты этим
//! пользуются для контракт-проверки без подъёма Tauri runtime.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Mutex;

use crate::call_id::ensure_path_under;
use crate::pipeline::resource_queue::{self, Resource};
use crate::providers::llm::{LlmError, LlmProvider, LlmRequest};

use super::llm_json::extract_json_object;
use super::llm_prompt::build_prompt;
use super::models::{model_path, ModelId};
use super::preset::LocalEnginePreset;
use super::sidecar::{run_sidecar_with_timeout, TempFileGuard};

// [Q] Сериализация LLM-вызовов (llama-cli грузит 1.5-7B GGUF, ~3-5 GB RAM;
// параллель = OOM/contention) жила в локальном `LLM_SEMAPHORE`; мигрировала
// в общий реестр `pipeline::resource_queue` (Resource::Llm, permit=1, FIFO) —
// та же семантика + наблюдаемость: QueueMonitor видит busy/очередь LLM.

/// Имя sidecar бинаря — совпадает с `tauri.conf.json::externalBin` и
/// capability whitelist'ом. Файлы на диске: `binaries/wotold-llama-<triple>`.
const SIDECAR_NAME: &str = "wotold-llama";

/// Контекст-окно по умолчанию. 8192 хватает на ~6000 английских / 4500
/// русских слов транскрипта — типичный 30-60 мин звонок. На Quality preset
/// (Qwen 7B) ctx можно поднять, но это память — оставляем conservative.
const DEFAULT_CTX_SIZE: u32 = 8192;
const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMP: f32 = 0.2;
const DEFAULT_THREADS: u32 = 6;
/// [recap-fix] Anti-repetition. Слабые модели (Qwen 1.5-3B) на extraction
/// сваливаются в repeat-loop: одна и та же строка («…новая должность проектного
/// инженера» ×80) льётся пока массив не закрыт → n-predict cap → обрезанный
/// JSON → «no JSON object» → чанк теряется + впустую сожжён budget. repeat
/// penalty + окно последних N токенов ломают петлю. 1.15 хватает, fluency не
/// страдает; low temp (0.2) сам по себе усиливает loops, потому penalty нужен.
const DEFAULT_REPEAT_PENALTY: f32 = 1.15;
const DEFAULT_REPEAT_LAST_N: u32 = 256;

/// Default timeout. Раньше 5 мин (M12.3.6 первоначально), но юзеры репортили
/// `local_llm_timeout` на full recap regen: 4096 max_tokens плюс GBNF
/// constraint плюс 7B/3B inference на M-series CPU могут занимать около 5-8
/// минут в worst case. 10 минут — margin без зависания; на таймауте
/// `drop(child)` шлёт kill сигнал, юзер видит явный error.
///
/// [P1.3] Backward-compat дефолт; для production callers использовать
/// [`timeout_for_preset`] — Light быстрее, Quality медленнее.
pub const LOCAL_LLM_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// [P1.3] Timeout per preset — отражает реальный compute cost. Без этой
/// дифференциации Light (1.5B) имел overkill cap (10 мин при реальных 1-3
/// мин), а Quality (7B) на длинных map-reduce transcripts получал SIGKILL
/// при еле-уложившемся в cap результате. Применяется через `.with_timeout(...)`
/// в callsite (`pipeline::run_local_inner` + `regenerate_recap_local`).
pub const fn timeout_for_preset(preset: LocalEnginePreset) -> Duration {
    match preset {
        LocalEnginePreset::Light => Duration::from_secs(5 * 60),
        LocalEnginePreset::Balanced => Duration::from_secs(10 * 60),
        LocalEnginePreset::Quality => Duration::from_secs(15 * 60),
    }
}

/// Локальный LLM на llama.cpp. Owns путь к GGUF модели + sidecar config.
/// [TD-08] Координаты живого resident-сервера. Ключ обязателен: без заголовка
/// `Authorization` сервер отвечает 401 на всё, кроме публичного `/health`.
#[derive(Debug, Clone)]
pub struct ServerHandle {
    pub url: String,
    pub api_key: String,
}

pub struct LocalLlamaProvider {
    /// `$APP_DATA/local_engine/models/{qwen25-1_5b|qwen25-3b|qwen25-7b}.bin`.
    model_path: PathBuf,
    /// Таймаут на один запрос (default `LOCAL_LLM_TIMEOUT`).
    timeout: Duration,
    /// Tauri AppHandle для доступа к shell sidecar. `None` в unit-тестах —
    /// тогда `generate()` возвращает `NotImplemented` (контракт-тест).
    app: Mutex<Option<AppHandle>>,
    /// Временная директория для prompt-файла. По умолчанию `std::env::temp_dir()`.
    tmp_dir: PathBuf,
    /// [M14 T-16 P2] Optional draft model для speculative decoding. Когда
    /// `Some` и файл существует — provider добавляет `--model-draft <path>`
    /// arg. Когда file отсутствует — graceful skip (log warn) и fall back
    /// на non-speculative path.
    draft_model_path: Option<PathBuf>,
    /// [B2] Если `Some` — resident `llama-server` поднят; `generate()` идёт
    /// HTTP `POST {url}/completion` вместо one-shot `llama-cli` спавна (модель
    /// уже в RAM). `None` — обычный one-shot путь.
    ///
    /// [TD-08] Кроме URL несёт api-key: сервер теперь требует авторизацию.
    server: Option<ServerHandle>,
    /// [Q] call_id для QueueMonitor: чей звонок держит/ждёт LLM-ресурс.
    /// `None` — служебная задача (warm-up).
    queue_call_id: Option<String>,
    /// [M15.7] Server-путь: реюз KV-префикса между запросами (`cache_prompt`).
    /// Ассистент включает (follow-up с общим префиксом = быстрый prefill);
    /// рекап-пайплайн оставляет false (одноразовые несвязанные промпты).
    cache_prompt: bool,
}

impl LocalLlamaProvider {
    /// Для preset'а — резолвит путь к LLM-модели preset'а.
    pub fn for_preset(app_data_dir: &Path, llm_id: ModelId) -> Self {
        Self {
            model_path: model_path(app_data_dir, llm_id.as_str()),
            timeout: LOCAL_LLM_TIMEOUT,
            app: Mutex::new(None),
            tmp_dir: std::env::temp_dir(),
            draft_model_path: None,
            server: None,
            queue_call_id: None,
            cache_prompt: false,
        }
    }

    /// [B2] Прикрепить координаты живого resident-сервера. `Some` → HTTP-путь,
    /// `None` → one-shot. Caller (build_local_llm_provider) читает handle из
    /// AppState.
    pub fn with_server(mut self, server: Option<ServerHandle>) -> Self {
        self.server = server;
        self
    }

    /// [Q] Привязать call_id к очереди LLM-ресурса (QueueMonitor покажет,
    /// чей звонок держит/ждёт LLM). Ставится при конструировании провайдера —
    /// вся generate-цепочка (classifier/refine/post-pass/narrative) наследует.
    pub fn with_call(mut self, call_id: impl Into<String>) -> Self {
        self.queue_call_id = Some(call_id.into());
        self
    }

    /// [M15.7] Включить реюз KV-префикса на server-пути (`cache_prompt`).
    /// На one-shot пути не влияет (у llama-cli нет эквивалента).
    pub fn with_cache_prompt(mut self, on: bool) -> Self {
        self.cache_prompt = on;
        self
    }

    /// [M14 T-16 P2] Set optional draft model для speculative decoding.
    /// Когда path is Some + file exists — provider добавит `--model-draft <path>`
    /// в sidecar args. Caller must pass `None` если speculative decoding off
    /// OR preset != Quality (см. `run_local_inner`).
    pub fn with_draft_model(mut self, path: Option<PathBuf>) -> Self {
        self.draft_model_path = path;
        self
    }

    /// Прикрепить AppHandle — pipeline-runner вызывает после resolve preset'а.
    /// Без handle `generate()` отвечает `NotImplemented` (см. модульный комментарий).
    pub async fn with_app(self, app: AppHandle) -> Self {
        {
            let mut guard = self.app.lock().await;
            *guard = Some(app);
        }
        self
    }

    /// Кастомный timeout — для тестов, диагностики, и [P1.3] per-preset
    /// дифференциации (`timeout_for_preset(preset)`).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override temp dir для тестов (изолируем prompt-файлы).
    /// [CI] `--all-targets` default-features clippy флагует dead_code т.к.
    /// текущие callers — voice-onnx-gated; helper сохраняем для future tests.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn with_tmp_dir(mut self, dir: PathBuf) -> Self {
        self.tmp_dir = dir;
        self
    }

    #[allow(dead_code)]
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    #[allow(dead_code)]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// [B2] HTTP-путь через resident `llama-server`. Тот же prompt (system +
    /// input) и та же per-request форма (json_schema/grammar), что one-shot,
    /// но без спавна процесса и перезагрузки модели.
    async fn generate_via_server(
        &self,
        server: &ServerHandle,
        request: LlmRequest,
    ) -> Result<Value, LlmError> {
        // [Q] Та же очередь что и CLI-путь — сервер `--parallel 1`, FIFO.
        let _permit = resource_queue::acquire(Resource::Llm, self.queue_call_id.as_deref()).await;

        let prompt = build_prompt(&request);
        let max_tokens = request
            .max_tokens
            .map(|n| n.clamp(256, 8192))
            .unwrap_or(DEFAULT_MAX_TOKENS);
        let mut body = serde_json::json!({
            "prompt": prompt,
            "n_predict": max_tokens,
            "temperature": DEFAULT_TEMP,
            "repeat_penalty": DEFAULT_REPEAT_PENALTY,
            "repeat_last_n": DEFAULT_REPEAT_LAST_N,
            "cache_prompt": self.cache_prompt,
        });
        if let Some(schema) = request.json_schema.as_deref() {
            match serde_json::from_str::<Value>(schema) {
                Ok(v) => body["json_schema"] = v,
                Err(e) => return Err(LlmError::Provider(format!("bad json_schema: {e}"))),
            }
        } else if let Some(grammar) = request.grammar.as_deref() {
            body["grammar"] = Value::String(grammar.to_string());
        }

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/completion", server.url))
            // [TD-08] Без ключа сервер отвечает 401.
            .bearer_auth(&server.api_key)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| LlmError::Provider(format!("llama-server request: {e}")))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            // Ключ не подошёл — на порту не наш сервер либо он перезапустился.
            return Err(LlmError::Auth("llama-server отверг api-key".into()));
        }
        if !resp.status().is_success() {
            return Err(LlmError::Provider(format!(
                "llama-server HTTP {}",
                resp.status()
            )));
        }
        let json: Value = resp
            .json()
            .await
            .map_err(|e| LlmError::Provider(format!("llama-server resp: {e}")))?;
        let content = json.get("content").and_then(Value::as_str).unwrap_or("");
        extract_json_object(content)
            .ok_or_else(|| LlmError::Provider("no JSON object in llama-server output".into()))
            .and_then(|json_str| {
                serde_json::from_str::<Value>(&json_str)
                    .map_err(|e| LlmError::Provider(format!("malformed JSON: {e}")))
            })
    }
}

#[async_trait]
impl LlmProvider for LocalLlamaProvider {
    async fn generate(&self, request: LlmRequest) -> Result<Value, LlmError> {
        // [B2] Resident-server путь: модель уже в RAM, шлём HTTP вместо спавна
        // one-shot процесса. Не нужен AppHandle.
        if let Some(server) = self.server.clone() {
            return self.generate_via_server(&server, request).await;
        }
        let app = {
            let guard = self.app.lock().await;
            guard.clone()
        };
        let Some(app) = app else {
            // Headless context (tests) — без AppHandle нет shell-доступа.
            return Err(LlmError::NotImplemented);
        };

        // [Q] Serialize subprocess spawns — 1 llama-completion at a time.
        // Permit держится до конца функции (drop при return / panic / cancel),
        // следующий caller автоматически продолжает с очереди. Acquire ПОСЛЕ
        // `Some(app)`-проверки — headless-тесты не трогают глобальный реестр.
        let _permit = resource_queue::acquire(Resource::Llm, self.queue_call_id.as_deref()).await;

        // 1. Сериализуем prompt в файл. llama-cli `-f` читает целиком с диска,
        //    не страдает от stdin escaping на UTF-8 кириллице.
        //
        // [Security M-2] Создаём с mode 0o600 + O_CREAT|O_EXCL — sensitive
        //    транскрипт не должен быть readable other users в shared /tmp.
        //    `std::env::temp_dir()` на macOS обычно даёт user-scoped
        //    /var/folders, но defense-in-depth: explicit perms.
        // [TD-11] Все temp-файлы несут транскрипт. Guard чистит их при ЛЮБОМ
        // выходе — happy-path, ранний `?` (ниже их несколько), panic, abort.
        // Раньше был ручной cleanup после await: при отмене задачи или раннем
        // `?` он не выполнялся, и файл оставался в /tmp.
        let mut temp_guard = TempFileGuard::new();

        let prompt = build_prompt(&request);
        let prompt_path = self
            .tmp_dir
            .join(format!("wotold-llama-{}.txt", uuid::Uuid::new_v4()));
        write_user_only(&prompt_path, prompt.as_bytes())
            .await
            .map_err(|e| LlmError::Provider(format!("prompt write: {e}")))?;
        temp_guard.push_file(&prompt_path);

        // 2. Спавним sidecar. Args строго соответствуют capability validator'ам.
        // [Security M-3] Defense-in-depth path checks ДО передачи в sidecar:
        //    model_path обязан быть под `local_engine/models/` директорией,
        //    prompt_path — под tmp_dir. `..` сегменты блокируются.
        // model_path под app_data_dir (constants → no '..'). Validate inline.
        if self
            .model_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(LlmError::Provider(
                "model path contains '..' segment".into(),
            ));
        }
        ensure_path_under(&prompt_path, &self.tmp_dir).map_err(LlmError::Provider)?;

        // [M14 T-09 Phase E] Optional GBNF grammar file. Когда request.grammar
        // set — пишем во временный файл (mirror prompt-file pattern) + передаём
        // через `--grammar-file <path>` в llama-cli. Cleanup в конце.
        let grammar_path: Option<PathBuf> = if let Some(grammar_text) = &request.grammar {
            let path = self
                .tmp_dir
                .join(format!("wotold-grammar-{}.gbnf", uuid::Uuid::new_v4()));
            write_user_only(&path, grammar_text.as_bytes())
                .await
                .map_err(|e| LlmError::Provider(format!("grammar write: {e}")))?;
            temp_guard.push_file(&path);
            ensure_path_under(&path, &self.tmp_dir).map_err(LlmError::Provider)?;
            Some(path)
        } else {
            None
        };

        // [M14 follow-up] Optional JSON Schema file. Когда request.json_schema set —
        // пишем во временный файл + передаём `--json-schema-file <path>`; llama.cpp
        // сам конвертит схему в GBNF и форсит форму. Сильнее generic grammar.
        let schema_path: Option<PathBuf> = if let Some(schema_text) = &request.json_schema {
            let path = self
                .tmp_dir
                .join(format!("wotold-schema-{}.json", uuid::Uuid::new_v4()));
            write_user_only(&path, schema_text.as_bytes())
                .await
                .map_err(|e| LlmError::Provider(format!("json-schema write: {e}")))?;
            temp_guard.push_file(&path);
            ensure_path_under(&path, &self.tmp_dir).map_err(LlmError::Provider)?;
            Some(path)
        } else {
            None
        };

        let max_tokens = request
            .max_tokens
            .map(|n| n.clamp(256, 8192))
            .unwrap_or(DEFAULT_MAX_TOKENS);
        let model_path_str = self
            .model_path
            .to_str()
            .ok_or_else(|| LlmError::Provider("non-utf8 model path".into()))?
            .to_string();
        let prompt_path_str = prompt_path
            .to_str()
            .ok_or_else(|| LlmError::Provider("non-utf8 prompt path".into()))?
            .to_string();
        let grammar_path_str: Option<String> = grammar_path
            .as_ref()
            .map(|p| {
                p.to_str()
                    .ok_or_else(|| LlmError::Provider("non-utf8 grammar path".into()))
                    .map(|s| s.to_string())
            })
            .transpose()?;
        let schema_path_str: Option<String> = schema_path
            .as_ref()
            .map(|p| {
                p.to_str()
                    .ok_or_else(|| LlmError::Provider("non-utf8 schema path".into()))
                    .map(|s| s.to_string())
            })
            .transpose()?;

        // [Dev] Brew-built llama-cli использует `@rpath/libllama.dylib` +
        // `libggml*.dylib`. См. stt.rs для подробностей.
        let mut sidecar = app
            .shell()
            .sidecar(SIDECAR_NAME)
            .map_err(|e| LlmError::Provider(format!("sidecar lookup: {e}")))?
            .env("DYLD_FALLBACK_LIBRARY_PATH", "/opt/homebrew/lib")
            .args([
                "-m",
                &model_path_str,
                "--temp",
                &format!("{DEFAULT_TEMP}"),
                // [recap-fix] Anti-repetition — ломает degenerate loop на extraction.
                "--repeat-penalty",
                &format!("{DEFAULT_REPEAT_PENALTY}"),
                "--repeat-last-n",
                &format!("{DEFAULT_REPEAT_LAST_N}"),
                "--ctx-size",
                &format!("{DEFAULT_CTX_SIZE}"),
                "--n-predict",
                &format!("{max_tokens}"),
                "--threads",
                &format!("{DEFAULT_THREADS}"),
                // [P9.1] Metal GPU offload — Apple Silicon M-series ~5-7×
                // быстрее CPU на Qwen 1.5-7B Q4_K_M. Metal shaders компилируются
                // первый раз на ~30 сек, потом кешируются (`~/Library/Caches/
                // llama.cpp/ggml-metal.shaderlib` либо рядом с binary). Этот
                // 30-сек overhead покрывается `LOCAL_LLM_TIMEOUT` (10 мин)
                // для самой первой инвокации после `brew upgrade llama.cpp`;
                // последующие вызовы стартуют instant.
                "-ngl",
                "99",
                // [P9.1+P10.1] Flash attention — 1.3-1.5× prompt eval speedup
                // на длинных prompts. Llama.cpp b9270+ требует значение
                // (`on|off|auto`) после `-fa`; bare flag съедает следующий
                // arg как value и валит sidecar с exit code 1.
                "-fa",
                "on",
                // [P9.1] KV cache quantization q8_0 — ~50% RAM cut (важно для
                // 7B + ctx 8192 на 16 GB M1 Pro где GPU wired limit ~10.6 GB).
                // Accuracy impact на recap-уровне ниже шума temperature=0.2.
                "-ctk",
                "q8_0",
                "-ctv",
                "q8_0",
                "--no-conversation",
                "--no-display-prompt",
                "--simple-io",
                // NOTE: НЕ передаём `--log-disable` — в `llama-completion` (b9270+)
                // этот флаг подавляет не только internal logs но и сам ответ модели
                // (stdout становится пустым). Pipeline парсит stdout → "no JSON
                // object in llama output". perf-логи всё равно идут в stderr, мы
                // их не читаем.
                "-f",
                &prompt_path_str,
            ]);
        if let Some(g) = grammar_path_str.as_deref() {
            sidecar = sidecar.args(["--grammar-file", g]);
        }
        if let Some(s) = schema_path_str.as_deref() {
            sidecar = sidecar.args(["--json-schema-file", s]);
        }

        // [M14 T-16 P2] Speculative decoding — добавить `--model-draft <path>`
        // когда draft model configured AND file exists. Если file отсутствует
        // (например пользователь enabled flag preempt'ивно до download) —
        // graceful skip с log warn (не fail весь generation).
        let draft_arg: Option<String> = self.draft_model_path.as_ref().and_then(|p| {
            if !p.exists() {
                log::warn!(
                    "T-16 speculative: draft model not found at {} — fallback to non-speculative",
                    p.display()
                );
                return None;
            }
            p.to_str().map(|s| s.to_string())
        });
        if let Some(d) = draft_arg.as_deref() {
            sidecar = sidecar.args(["--model-draft", d]);
        }

        let result = run_sidecar_with_timeout(sidecar, self.timeout).await;

        // [TD-11] cleanup — на `temp_guard` (Drop). Ручные remove_file убраны:
        // они не срабатывали при отмене задачи и на ранних `?`.
        let stdout = result?;
        // 4. Извлекаем JSON из stdout (модель может выдать echo / whitespace
        //    даже с no-display-prompt).
        //
        // [P8.1] Shape validation НЕ делается на этом уровне — provider
        // возвращает любой parseable JSON. Per-stage caller'ы (classifier
        // парсит `ClassifierJson`, recap парсит `RecapV2Json`, action_items
        // post-pass свой shape) валидируют через serde. Хардкоднутый
        // recap-shape валидатор тут раньше ломал classifier callers
        // (missing title) → wasted retry-cycle через gbnf grammar fallback.
        extract_json_object(&stdout)
            .ok_or_else(|| LlmError::Provider("no JSON object in llama output".into()))
            .and_then(|json_str| {
                serde_json::from_str::<Value>(&json_str)
                    .map_err(|e| LlmError::Provider(format!("malformed JSON: {e}")))
            })
    }
}

/// [Security M-2] Запись в файл с mode 0o600 (owner read/write only).
/// `O_EXCL` гарантирует что файл не существовал ранее — защита от race
/// или симлинк-атаки в shared /tmp. Помечен `pub(super)` для переиспользования
/// в [`super::stt`].
pub(super) async fn write_user_only(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    // `tokio::fs::OpenOptions::mode` принимает u32 — это не Unix-only ext,
    // а нативный tokio метод (на не-unix просто no-op).
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true) // = O_CREAT | O_EXCL
        .mode(0o600)
        .open(path)
        .await?;
    file.write_all(bytes).await?;
    file.flush().await?;
    Ok(())
}

// [P8.1] `validate_recap_shape` удалён — был хардкоднутый title+summary
// валидатор который ломал classifier и другие non-recap callers
// (`bad_shape: missing title` на любой `{call_type, confidence}` ответ).
// Per-stage validation теперь через serde в caller'ах:
// - `classifier::parse_classifier_response` → `ClassifierJson`
// - `recap::parse_v2_response` → `RecapV2Json`
// - `action_item_post_pass::parse_response` → list shape

/// Промпт для local-LLM. Отдельный от Anthropic (PRD §M12.3.3): явные
/// инструкции «отвечай только JSON», few-shot пример. Готов сейчас чтобы
/// TDD для real impl не блокировался когда sidecar land'нет.
///
/// # Why exported
///
/// Тесты в M12.3 будут проверять что real impl получает именно этот prompt
/// (substring match). Также позволяет UI показывать «что именно мы шлём
/// модели» для debug.
///
/// **[M14 T-04 Phase A]** Production callers больше не используют этот prompt
/// напрямую — local pipeline теперь идёт через `pipeline::local_orchestrator`
/// → `recap::build_v2_system_prompt`. Константа сохраняется как
/// pre-v2 baseline для debugging UI + regression-тестов в M12.3.
#[allow(dead_code)]
pub const LOCAL_LLM_SYSTEM_PROMPT: &str = concat!(
    "Ты — помощник по составлению краткой сводки телефонного разговора.\n\n",
    "Отвечай ТОЛЬКО валидным JSON. Никакого текста до или после JSON.\n\n",
    "Структура ответа:\n",
    "{\n",
    "  \"title\": \"краткое название звонка, до 60 символов\",\n",
    "  \"summary\": \"1-2 параграфа summary\",\n",
    "  \"key_points\": [\"важный пункт 1\", \"пункт 2\"],\n",
    "  \"mom\": \"minutes of meeting в Markdown\",\n",
    "  \"action_items\": [{\"text\": \"действие\", \"ownerHint\": \"имя если ясно\", \"due\": \"ISO date если ясно\"}],\n",
    "  \"participants\": [{\"speakerTag\": \"speaker:0\", \"displayName\": \"имя если ясно\"}]\n",
    "}\n\n",
    "Не выдумывай факты. Если поле неизвестно — оставь пустым / опусти.\n\n",
    "Пример:\n",
    "Input transcript: \"Привет, Иван. Давай завтра встретимся в 10.\"\n",
    "Output:\n",
    "{\"title\":\"Договорённость о встрече\",\"summary\":\"Звонящий предложил встречу собеседнику Ивану на завтра в 10.\",\"key_points\":[\"Встреча завтра в 10\"],\"mom\":\"## Встреча\\n- Завтра в 10\",\"action_items\":[{\"text\":\"Встретиться\",\"ownerHint\":\"Иван\",\"due\":\"\"}],\"participants\":[{\"speakerTag\":\"speaker:0\",\"displayName\":\"Иван\"}]}\n",
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_preset_resolves_model_path() {
        let p = LocalLlamaProvider::for_preset(Path::new("/data"), ModelId::QWEN25_3B);
        assert!(
            p.model_path()
                .to_string_lossy()
                .ends_with("/data/local_engine/models/qwen25-3b.bin"),
            "got {}",
            p.model_path().display()
        );
        assert_eq!(p.timeout(), LOCAL_LLM_TIMEOUT);
    }

    #[test]
    fn with_timeout_overrides_default() {
        let p = LocalLlamaProvider::for_preset(Path::new("/d"), ModelId::QWEN25_1_5B)
            .with_timeout(Duration::from_secs(10));
        assert_eq!(p.timeout(), Duration::from_secs(10));
    }

    // [P1.3] Per-preset timeout dispatch — Light быстрее (1.5B), Quality
    // медленнее (7B + map-reduce). Жёстко прописанные values защищают от
    // случайной перестановки.
    #[test]
    fn timeout_for_preset_light_is_5_min() {
        assert_eq!(
            timeout_for_preset(LocalEnginePreset::Light),
            Duration::from_secs(5 * 60)
        );
    }

    #[test]
    fn timeout_for_preset_balanced_matches_legacy_default() {
        assert_eq!(
            timeout_for_preset(LocalEnginePreset::Balanced),
            Duration::from_secs(10 * 60)
        );
        assert_eq!(
            timeout_for_preset(LocalEnginePreset::Balanced),
            LOCAL_LLM_TIMEOUT
        );
    }

    #[test]
    fn timeout_for_preset_quality_is_15_min() {
        assert_eq!(
            timeout_for_preset(LocalEnginePreset::Quality),
            Duration::from_secs(15 * 60)
        );
    }

    #[tokio::test]
    async fn generate_returns_not_implemented_for_stub() {
        let p = LocalLlamaProvider::for_preset(Path::new("/d"), ModelId::QWEN25_3B);
        let err = p
            .generate(LlmRequest {
                model: None,
                system: LOCAL_LLM_SYSTEM_PROMPT.to_string(),
                input: "transcript".to_string(),
                max_tokens: Some(1024),
                grammar: None,
                json_schema: None,
            })
            .await
            .expect_err("stub must error");
        assert!(matches!(err, LlmError::NotImplemented));
    }

    #[test]
    fn system_prompt_enforces_json_only() {
        // Регрессия: real impl будет тестироваться против substring match на
        // эти ключевые инструкции (PRD §M12.3.3 «явные инструкции»).
        assert!(LOCAL_LLM_SYSTEM_PROMPT.contains("ТОЛЬКО валидным JSON"));
        assert!(LOCAL_LLM_SYSTEM_PROMPT.contains("title"));
        assert!(LOCAL_LLM_SYSTEM_PROMPT.contains("action_items"));
        assert!(LOCAL_LLM_SYSTEM_PROMPT.contains("participants"));
    }

    #[test]
    fn system_prompt_includes_few_shot_example() {
        // Few-shot critical для 2-7B моделей. Без примера они часто свалятся
        // в free-form prose. Тест guard'ит регрессию промпта.
        assert!(LOCAL_LLM_SYSTEM_PROMPT.contains("Пример:"));
        assert!(LOCAL_LLM_SYSTEM_PROMPT.contains("Output:"));
    }

    // [P8.1] validate_recap_shape удалён — shape валидация теперь
    // per-caller через serde structs. Зеркало-тесты живут в
    // `pipeline::recap` (RecapV2Json serde tests) и
    // `pipeline::classifier` (ClassifierJson).

    // ── write_user_only ─────────────────────────────────────────────────

    #[tokio::test]
    async fn write_user_only_creates_file_with_0600_perms() {
        use std::os::unix::fs::PermissionsExt;
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("prompt.txt");
        write_user_only(&path, b"sensitive transcript")
            .await
            .unwrap();
        let meta = tokio::fs::metadata(&path).await.unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0o600, got 0o{mode:o}");
        let body = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(body, "sensitive transcript");
    }

    #[tokio::test]
    async fn write_user_only_refuses_existing_path() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        tokio::fs::write(&path, b"prior").await.unwrap();
        // O_EXCL → AlreadyExists. Защита от race / симлинк подмены.
        let err = write_user_only(&path, b"new").await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn provider_default_has_no_draft_model() {
        let p = LocalLlamaProvider::for_preset(Path::new("/data"), ModelId::QWEN25_7B);
        assert!(p.draft_model_path.is_none());
    }

    #[test]
    fn with_draft_model_sets_path() {
        let p = LocalLlamaProvider::for_preset(Path::new("/data"), ModelId::QWEN25_7B)
            .with_draft_model(Some(PathBuf::from(
                "/data/local_engine/models/qwen25-0_5b.bin",
            )));
        assert_eq!(
            p.draft_model_path.as_deref(),
            Some(Path::new("/data/local_engine/models/qwen25-0_5b.bin"))
        );
    }

    #[test]
    fn with_draft_model_none_keeps_none() {
        let p = LocalLlamaProvider::for_preset(Path::new("/data"), ModelId::QWEN25_7B)
            .with_draft_model(None);
        assert!(p.draft_model_path.is_none());
    }
}
