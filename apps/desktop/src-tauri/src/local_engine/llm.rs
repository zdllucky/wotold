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
use super::preset::LocalEnginePreset;
use super::sidecar::SidecarGuard;

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
        }
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
        //
        // [Security M-2] Создаём с mode 0o600 + O_CREAT|O_EXCL — sensitive
        //    транскрипт не должен быть readable other users в shared /tmp.
        //    `std::env::temp_dir()` на macOS обычно даёт user-scoped
        //    /var/folders, но defense-in-depth: explicit perms.
        let prompt = build_prompt(&request);
        let prompt_path = self
            .tmp_dir
            .join(format!("wotold-llama-{}.txt", uuid::Uuid::new_v4()));
        write_user_only(&prompt_path, prompt.as_bytes())
            .await
            .map_err(|e| LlmError::Provider(format!("prompt write: {e}")))?;

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
                "--ctx-size",
                &format!("{DEFAULT_CTX_SIZE}"),
                "--n-predict",
                &format!("{max_tokens}"),
                "--threads",
                &format!("{DEFAULT_THREADS}"),
                // Newer llama.cpp attempts Metal shader compilation on first run
                // which can exceed LOCAL_LLM_TIMEOUT. CPU is fast enough for
                // 1.5–7B models on M-series and avoids GPU init entirely.
                "-ngl",
                "0",
                "--no-conversation",
                "--no-display-prompt",
                "--simple-io",
                "--log-disable",
                "-f",
                &prompt_path_str,
            ]);
        if let Some(g) = grammar_path_str.as_deref() {
            sidecar = sidecar.args(["--grammar-file", g]);
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

        // 3. Чистим prompt-файл + grammar-файл (если был) вне зависимости от исхода.
        let _ = tokio::fs::remove_file(&prompt_path).await;
        if let Some(g) = grammar_path.as_ref() {
            let _ = tokio::fs::remove_file(g).await;
        }

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

/// [Security M-3] Defense-in-depth: проверить что path не содержит `..`
/// сегментов И начинается с разрешённого prefix. Capability validator
/// `^[A-Za-z0-9._/\-]+$` пропускает `../../etc/passwd` — это последняя
/// граница. Канонических `.canonicalize()` НЕ делаем (path может не
/// существовать на момент проверки — например, output stem whisper-cli).
///
/// Returns Err если найден `..` сегмент или prefix не совпадает.
pub(super) fn ensure_path_under(path: &Path, allowed_prefix: &Path) -> Result<(), String> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("path {} contains '..' segment", path.display()));
    }
    if !path.starts_with(allowed_prefix) {
        return Err(format!(
            "path {} not under prefix {}",
            path.display(),
            allowed_prefix.display()
        ));
    }
    Ok(())
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
    // [M12.3.6 + M12.6.4] llama-cli не читает stdin (prompt через `-f`).
    // `SidecarGuard` гарантирует SIGKILL процессу при cancel / panic /
    // unhandled Err — без него process переживает abort task'а на 5+ минут.
    let mut guard = Some(SidecarGuard::new(child));

    let mut stdout = Vec::<u8>::new();
    let mut stderr = Vec::<u8>::new();
    let mut exit_code: Option<i32> = None;
    let drained = tokio::time::timeout(timeout, async {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(b) => stdout.extend_from_slice(&b),
                CommandEvent::Stderr(b) => stderr.extend_from_slice(&b),
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

    let stderr_snippet = || {
        let s = String::from_utf8_lossy(&stderr);
        if s.is_empty() {
            String::new()
        } else {
            format!("; stderr: {}", &s[..s.len().min(512)])
        }
    };

    match drained {
        Ok(Ok(())) => {
            // Terminated event received — процесс мёртв. Release guard
            // чтобы Drop не пытался killить уже-завершившийся pid.
            if let Some(g) = guard.take() {
                g.release();
            }
            if let Some(code) = exit_code {
                if code != 0 {
                    return Err(LlmError::Provider(format!(
                        "sidecar exit code {code}; output {} bytes{}",
                        stdout.len(),
                        stderr_snippet()
                    )));
                }
            }
            Ok(String::from_utf8_lossy(&stdout).into_owned())
        }
        Ok(Err(e)) => {
            // Sidecar Error event — child может быть ещё жив, явный kill.
            if let Some(g) = guard.take() {
                g.kill();
            }
            Err(e)
        }
        Err(_) => {
            // [PRD §M12.3.6 + M12.6.4] Timeout / cancel — child работает,
            // нужен SIGKILL. `tauri_plugin_shell::CommandChild::kill()` →
            // `SharedChild::kill()` → POSIX kill(SIGKILL). При abort task'а
            // `guard` всё равно drop'нется и убьёт процесс (defense-in-depth).
            if let Some(g) = guard.take() {
                g.kill();
            }
            Err(LlmError::Provider(format!(
                "local_llm_timeout{}",
                stderr_snippet()
            )))
        }
    }
}

/// Найти первый сбалансированный JSON-объект в строке. Модель может
/// выдать чуть-чуть мусора до/после; ищем по brace-counter.
///
/// # UTF-8 safety
///
/// Функция итерирует raw `u8`, но это безопасно для UTF-8 строк по
/// определению кодировки: continuation bytes (0x80..=0xBF) НЕ пересекаются
/// с ASCII-кодами которые мы трекаем (`"` 0x22, `{` 0x7B, `}` 0x7D, `\` 0x5C).
/// Любой multi-byte Unicode codepoint имеет ведущий byte ≥ 0xC0 — тоже вне
/// нашего набора. Поэтому мы не можем «случайно» войти в строку посреди
/// многобайтового символа. Регрессия покрыта `extract_json_handles_escaped_quote_in_string`
/// + `extract_json_handles_nested_braces` тестами.
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
            grammar: None,
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
            grammar: None,
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

    // ── ensure_path_under ───────────────────────────────────────────────

    #[test]
    fn ensure_path_under_accepts_path_inside_prefix() {
        assert!(ensure_path_under(
            Path::new("/data/local_engine/models/whisper-small.bin"),
            Path::new("/data/local_engine"),
        )
        .is_ok());
    }

    #[test]
    fn ensure_path_under_rejects_dotdot_segment() {
        let err = ensure_path_under(
            Path::new("/data/local_engine/../etc/passwd"),
            Path::new("/data/local_engine"),
        )
        .expect_err("`..` сегмент → Err");
        assert!(err.contains("'..' segment"));
    }

    #[test]
    fn ensure_path_under_rejects_path_outside_prefix() {
        let err = ensure_path_under(Path::new("/etc/passwd"), Path::new("/data/local_engine"))
            .expect_err("вне prefix → Err");
        assert!(err.contains("not under prefix"));
    }

    #[test]
    fn ensure_path_under_handles_relative_paths_safely() {
        // Relative paths не starts_with absolute prefix — должны быть отклонены.
        let err = ensure_path_under(
            Path::new("models/whisper.bin"),
            Path::new("/data/local_engine"),
        )
        .expect_err("relative → Err");
        assert!(err.contains("not under prefix"));
    }

    // ── [M14 T-16 P2] Speculative decoding draft model plumbing ─────────

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
