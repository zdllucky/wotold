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
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Mutex;

use crate::providers::llm::{LlmError, LlmProvider, LlmRequest};

use super::models::{model_path, ModelId};

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

/// Default timeout per M12.3.6. 5 минут на 1-часовое аудио.
pub const LOCAL_LLM_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Локальный LLM на llama.cpp. Owns путь к GGUF модели + sidecar config.
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
}

impl LocalLlamaProvider {
    /// Для preset'а — резолвит путь к LLM-модели preset'а.
    pub fn for_preset(app_data_dir: &Path, llm_id: ModelId) -> Self {
        Self {
            model_path: model_path(app_data_dir, llm_id.as_str()),
            timeout: LOCAL_LLM_TIMEOUT,
            app: Mutex::new(None),
            tmp_dir: std::env::temp_dir(),
        }
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

    /// Кастомный timeout — для тестов и потенциальных tier'ов.
    /// Helper used by tests + диагностическим UI; production pipeline
    /// полагается на `LOCAL_LLM_TIMEOUT` default.
    #[allow(dead_code)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override temp dir для тестов (изолируем prompt-файлы).
    #[cfg(test)]
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
}

#[async_trait]
impl LlmProvider for LocalLlamaProvider {
    async fn generate(&self, request: LlmRequest) -> Result<Value, LlmError> {
        let app = {
            let guard = self.app.lock().await;
            guard.clone()
        };
        let Some(app) = app else {
            // Headless context (tests) — без AppHandle нет shell-доступа.
            return Err(LlmError::NotImplemented);
        };

        // 1. Сериализуем prompt в файл. llama-cli `-f` читает целиком с диска,
        //    не страдает от stdin escaping на UTF-8 кириллице.
        let prompt = build_prompt(&request);
        let prompt_path = self
            .tmp_dir
            .join(format!("wotold-llama-{}.txt", uuid::Uuid::new_v4()));
        tokio::fs::write(&prompt_path, &prompt)
            .await
            .map_err(|e| LlmError::Provider(format!("prompt write: {e}")))?;

        // 2. Спавним sidecar. Args строго соответствуют capability validator'ам.
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

        let sidecar = app
            .shell()
            .sidecar(SIDECAR_NAME)
            .map_err(|e| LlmError::Provider(format!("sidecar lookup: {e}")))?
            .args([
                "-m",
                &model_path_str,
                "--temp",
                &format!("{DEFAULT_TEMP}"),
                "--ctx-size",
                &format!("{DEFAULT_CTX_SIZE}"),
                "--n-predict",
                &format!("{max_tokens}"),
                "--threads",
                &format!("{DEFAULT_THREADS}"),
                "--no-conversation",
                "--no-display-prompt",
                "--simple-io",
                "--log-disable",
                "-f",
                &prompt_path_str,
            ]);

        let result = run_sidecar_with_timeout(sidecar, self.timeout).await;

        // 3. Чистим prompt-файл вне зависимости от исхода.
        let _ = tokio::fs::remove_file(&prompt_path).await;

        let stdout = result?;
        // 4. Извлекаем JSON из stdout (модель может выдать echo / whitespace
        //    даже с no-display-prompt).
        extract_json_object(&stdout)
            .ok_or_else(|| LlmError::Provider("no JSON object in llama output".into()))
            .and_then(|json_str| {
                serde_json::from_str::<Value>(&json_str)
                    .map_err(|e| LlmError::Provider(format!("malformed JSON: {e}")))
            })
            .and_then(validate_recap_shape)
    }
}

/// Собрать финальный prompt: system + двойной перенос + transcript.
fn build_prompt(request: &LlmRequest) -> String {
    let mut s = String::with_capacity(request.system.len() + request.input.len() + 4);
    s.push_str(&request.system);
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s.push('\n');
    s.push_str(&request.input);
    s
}

/// Запустить sidecar, ждать `Terminated`, агрегировать stdout. Таймаут
/// `tokio::time::timeout`. На таймауте — `drop(child)` шлёт kill сигнал.
async fn run_sidecar_with_timeout(
    sidecar: tauri_plugin_shell::process::Command,
    timeout: Duration,
) -> Result<String, LlmError> {
    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| LlmError::Provider(format!("sidecar spawn: {e}")))?;
    // [M12.3.6] llama-cli не читает stdin (prompt передан через `-f`), но
    // `tauri-plugin-shell::CommandChild` всё равно держит PipeWriter
    // открытым до drop. Это не блокирует процесс — llama-cli просто игнорит
    // stdin. Закрытие происходит при `drop(child)` или `child.kill()`.

    let mut stdout = Vec::<u8>::new();
    let mut exit_code: Option<i32> = None;
    let drained = tokio::time::timeout(timeout, async {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(b) => stdout.extend_from_slice(&b),
                CommandEvent::Stderr(_) => {
                    // llama-cli с --log-disable не должен писать в stderr,
                    // но если что — игнорируем. UI получит ошибку из stdout.
                }
                CommandEvent::Terminated(p) => {
                    exit_code = p.code;
                    return Ok::<(), LlmError>(());
                }
                CommandEvent::Error(e) => {
                    return Err(LlmError::Provider(format!("sidecar error: {e}")));
                }
                _ => {}
            }
        }
        Ok(())
    })
    .await;

    match drained {
        Ok(Ok(())) => {
            // Terminated event already received — процесс мёртв, drop = no-op.
            drop(child);
            if let Some(code) = exit_code {
                if code != 0 {
                    return Err(LlmError::Provider(format!(
                        "sidecar exit code {code}; output {} bytes",
                        stdout.len()
                    )));
                }
            }
            Ok(String::from_utf8_lossy(&stdout).into_owned())
        }
        Ok(Err(e)) => {
            // Sidecar Error event получен — child может быть ещё жив.
            // Явный kill чтобы не оставлять зомби.
            let _ = child.kill();
            Err(e)
        }
        Err(_) => {
            // [PRD §M12.3.6] Таймаут — child всё ещё работает, нужен явный
            // SIGKILL. `tauri_plugin_shell::CommandChild::kill()` →
            // `SharedChild::kill()` → POSIX kill(SIGKILL). `drop(child)`
            // НЕ убивает процесс (плагин не имеет Drop impl), только закрывает
            // stdin pipe writer.
            let _ = child.kill();
            Err(LlmError::Provider("local_llm_timeout".into()))
        }
    }
}

/// Найти первый сбалансированный JSON-объект в строке. Модель может
/// выдать чуть-чуть мусора до/после; ищем по brace-counter.
fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            match b {
                b'\\' => escape = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return std::str::from_utf8(&bytes[start..=i])
                        .ok()
                        .map(String::from);
                }
            }
            _ => {}
        }
    }
    None
}

/// Проверить что модель вернула хотя бы обязательные поля. Невалидный
/// формат → `LlmError::Provider("bad_shape: missing X")` чтобы UI мог
/// предложить retry / Cloud-fallback.
fn validate_recap_shape(json: Value) -> Result<Value, LlmError> {
    if !json.is_object() {
        return Err(LlmError::Provider("bad_shape: root not object".into()));
    }
    for key in ["title", "summary"] {
        if json.get(key).is_none() {
            return Err(LlmError::Provider(format!("bad_shape: missing {key}")));
        }
    }
    Ok(json)
}

/// Промпт для local-LLM. Отдельный от Anthropic (PRD §M12.3.3): явные
/// инструкции «отвечай только JSON», few-shot пример. Готов сейчас чтобы
/// TDD для real impl не блокировался когда sidecar land'нет.
///
/// # Why exported
///
/// Тесты в M12.3 будут проверять что real impl получает именно этот prompt
/// (substring match). Также позволяет UI показывать «что именно мы шлём
/// модели» для debug.
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

    #[tokio::test]
    async fn generate_returns_not_implemented_for_stub() {
        let p = LocalLlamaProvider::for_preset(Path::new("/d"), ModelId::QWEN25_3B);
        let err = p
            .generate(LlmRequest {
                model: None,
                system: LOCAL_LLM_SYSTEM_PROMPT.to_string(),
                input: "transcript".to_string(),
                max_tokens: Some(1024),
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

    // ── build_prompt ────────────────────────────────────────────────────

    #[test]
    fn build_prompt_concatenates_system_and_input() {
        let req = LlmRequest {
            model: None,
            system: "SYS".into(),
            input: "BODY".into(),
            max_tokens: None,
        };
        let p = build_prompt(&req);
        assert!(p.starts_with("SYS"));
        assert!(p.ends_with("BODY"));
        assert!(
            p.contains("SYS\n\nBODY"),
            "missing blank separator, got: {p:?}"
        );
    }

    #[test]
    fn build_prompt_normalizes_trailing_newline() {
        // system уже с newline → не плодим лишних \n\n\n
        let req = LlmRequest {
            model: None,
            system: "SYS\n".into(),
            input: "BODY".into(),
            max_tokens: None,
        };
        let p = build_prompt(&req);
        assert!(p.contains("SYS\n\nBODY"));
        assert!(!p.contains("SYS\n\n\nBODY"));
    }

    // ── extract_json_object ─────────────────────────────────────────────

    #[test]
    fn extract_json_finds_object_among_prose() {
        let s = "leading garbage\n{\"title\":\"X\",\"summary\":\"Y\"}\ntrailing";
        let out = extract_json_object(s).unwrap();
        assert_eq!(out, "{\"title\":\"X\",\"summary\":\"Y\"}");
    }

    #[test]
    fn extract_json_handles_nested_braces() {
        let s = r#"{"a":{"b":{"c":1}},"d":"}}"}"#;
        let out = extract_json_object(s).unwrap();
        // вернёт весь объект, не зацикливается на внутренних `}`
        assert_eq!(out, s);
    }

    #[test]
    fn extract_json_handles_escaped_quote_in_string() {
        let s = r#"{"text":"He said \"hi\" then left"}"#;
        let out = extract_json_object(s).unwrap();
        assert_eq!(out, s);
    }

    #[test]
    fn extract_json_returns_none_when_no_brace() {
        assert!(extract_json_object("plain text no json").is_none());
    }

    #[test]
    fn extract_json_returns_none_when_unbalanced() {
        // Открывающая скобка без закрывающей → нет результата.
        assert!(extract_json_object("{\"a\":1").is_none());
    }

    // ── validate_recap_shape ────────────────────────────────────────────

    #[test]
    fn validate_accepts_minimal_recap() {
        let v: Value = serde_json::from_str(r#"{"title":"T","summary":"S"}"#).unwrap();
        assert!(validate_recap_shape(v).is_ok());
    }

    #[test]
    fn validate_rejects_missing_title() {
        let v: Value = serde_json::from_str(r#"{"summary":"S"}"#).unwrap();
        let err = validate_recap_shape(v).unwrap_err();
        assert!(err.to_string().contains("missing title"));
    }

    #[test]
    fn validate_rejects_non_object_root() {
        let v: Value = serde_json::from_str("[\"not\", \"object\"]").unwrap();
        let err = validate_recap_shape(v).unwrap_err();
        assert!(err.to_string().contains("not object"));
    }
}
