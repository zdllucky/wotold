//! [M12.1] LocalWhisperProvider — `TranscriptionProvider` impl через
//! `whisper-cli` sidecar (whisper.cpp).
//!
//! # Архитектура (sidecar, не sherpa-onnx)
//!
//! sherpa-onnx Whisper требует encoder.onnx + decoder.onnx пары, что
//! несовместимо с ggerganov `.bin` форматом который мы держим в
//! [`super::models::MODEL_CATALOG`] (refresh script даёт SHA256 для этого
//! формата). Поэтому Whisper интеграция — через whisper.cpp `whisper-cli`
//! sidecar по тому же паттерну что и llama-cli ([`super::llm`]).
//!
//! Pipeline:
//! 1. Spawn `wotold-whisper` с `-m <model.bin> -f <audio.wav>
//!    --output-json-full -of <stem> -l <lang>`.
//! 2. `whisper-cli` пишет `<stem>.json` с сегментами + word timestamps.
//! 3. По `Terminated` Rust читает файл, парсит, мапит в `TranscriptSegment`.
//! 4. Чистит временный JSON файл.
//!
//! # Per-track (PRD §M12.1.2)
//!
//! mic → `TrackKind::MicOwner` → все сегменты получат `speaker:owner`
//! (без диаризации, M3.7).
//! system → `TrackKind::System` → сегменты идут «как есть» (speaker tags
//! ставятся в [`super::diarization`] + [`super::merge`]).

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Mutex;

use crate::pipeline::resource_queue::{self, Resource};
use crate::providers::transcription::{
    DiarizedTranscript, TranscriptSegment, TranscriptionError, TranscriptionOpts,
    TranscriptionProvider,
};

use super::hallucination::is_hallucination;
use super::models::{model_path, ModelId};
use super::sidecar::{SidecarGuard, TempFileGuard};

/// Имя whisper.cpp sidecar бинаря.
const SIDECAR_NAME: &str = "wotold-whisper";

/// Sane default — 30 минут аудио должно влезать. Pipeline-runner может
/// override через `with_timeout`.
pub const LOCAL_WHISPER_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// [P15.1] 6 → 8. M-series chips имеют 8+ performance cores; whisper-cli
/// CPU decoder pass scales линейно до 8 threads на Apple Silicon. Encoder
/// уже GPU-bound через Metal backend.
const DEFAULT_THREADS: u32 = 8;

/// Per-PRD-§M12.1.2 «per-track processing»: mic-дорожка получает
/// owner-speaker (без диаризации, M3.7), system-дорожка идёт в M12.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// Микрофонная дорожка владельца устройства.
    MicOwner,
    /// Системная (динамики) — multi-speaker, идёт через диаризацию.
    System,
}

/// Local-движок Whisper. Hold'ит resolved-path к модели + per-track mode.
/// Конструируется в `pipeline::run` после resolve preset → model id.
pub struct LocalWhisperProvider {
    /// `$APP_DATA/local_engine/models/whisper-{small|medium|large-v3}.bin`.
    model_path: PathBuf,
    /// Mic/owner или system. Влияет на post-process: mic → один speaker_tag,
    /// system → segments передаются дальше в [`super::diarization`].
    track: TrackKind,
    /// Tauri AppHandle для shell sidecar. `None` в unit-тестах → NotImplemented.
    app: Mutex<Option<AppHandle>>,
    /// Куда писать временный JSON-файл whisper-cli (default = `std::env::temp_dir()`).
    tmp_dir: PathBuf,
    /// Таймаут на один transcribe call.
    timeout: Duration,
    /// [Q] call_id для QueueMonitor: чей звонок держит/ждёт STT-ресурс.
    queue_call_id: Option<String>,
}

impl LocalWhisperProvider {
    /// Сконструировать провайдер для preset'а. Не валидирует наличие файла —
    /// проверка происходит в [`super::models::check_status`] до запуска
    /// pipeline (M12.6).
    pub fn for_preset(app_data_dir: &Path, whisper_id: ModelId, track: TrackKind) -> Self {
        Self {
            model_path: model_path(app_data_dir, whisper_id.as_str()),
            track,
            app: Mutex::new(None),
            tmp_dir: std::env::temp_dir(),
            timeout: LOCAL_WHISPER_TIMEOUT,
            queue_call_id: None,
        }
    }

    /// [Q] Привязать call_id к STT-очереди (QueueMonitor покажет чей звонок).
    pub fn with_call(mut self, call_id: impl Into<String>) -> Self {
        self.queue_call_id = Some(call_id.into());
        self
    }

    /// Прикрепить AppHandle — pipeline-runner вызывает после resolve.
    /// Без handle `transcribe()` возвращает `NotImplemented`.
    pub async fn with_app(self, app: AppHandle) -> Self {
        {
            let mut guard = self.app.lock().await;
            *guard = Some(app);
        }
        self
    }

    #[allow(dead_code)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    // [CI] dead_code под default-features --all-targets — caller voice-onnx-gated.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn with_tmp_dir(mut self, dir: PathBuf) -> Self {
        self.tmp_dir = dir;
        self
    }

    /// Текущий разрешённый путь к модели (для тестов / диагностики).
    #[allow(dead_code)]
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    #[allow(dead_code)]
    pub fn track(&self) -> TrackKind {
        self.track
    }
}

#[async_trait]
impl TranscriptionProvider for LocalWhisperProvider {
    async fn transcribe(
        &self,
        audio_path: &Path,
        opts: TranscriptionOpts,
    ) -> Result<DiarizedTranscript, TranscriptionError> {
        let app = {
            let guard = self.app.lock().await;
            guard.clone()
        };
        let Some(app) = app else {
            return Err(TranscriptionError::NotImplemented);
        };

        // [TD-11] Whisper-cli пишет output JSON (расшифровка звонка) с дефолтным
        // umask (0644). Раньше stem лежал прямо в общем tmp_dir, и chmod 0600
        // накладывался только ПОСЛЕ окончания транскрипции — весь этот период
        // файл читаем чужим UID. Теперь output идёт в приватную поддиректорию
        // 0o700, созданную ДО спавна: содержимое неважно, войти внутрь чужой
        // UID не может. Guard рекурсивно чистит директорию при любом выходе,
        // включая abort (ручной remove_file на await этого не давал).
        let mut temp_guard = TempFileGuard::new();
        let work_dir = self
            .tmp_dir
            .join(format!("wotold-stt-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&work_dir)
            .await
            .map_err(|e| TranscriptionError::Provider(format!("mkdir stt work-dir: {e}")))?;
        temp_guard.push_dir(&work_dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                tokio::fs::set_permissions(&work_dir, std::fs::Permissions::from_mode(0o700)).await;
        }
        let stem = work_dir.join("out");
        let json_path = stem.with_extension("json");

        // [Security M-3] Defense-in-depth: блокируем `..` в любом из путей.
        // Capability validator `^[A-Za-z0-9._/\-]+$` пропускает traversal —
        // Rust обязан страховать.
        for p in [&self.model_path, audio_path, &stem] {
            if p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(TranscriptionError::Provider(format!(
                    "path traversal blocked: {}",
                    p.display()
                )));
            }
        }
        crate::call_id::ensure_path_under(&stem, &self.tmp_dir)
            .map_err(TranscriptionError::Provider)?;

        let model_str = self
            .model_path
            .to_str()
            .ok_or_else(|| TranscriptionError::Provider("non-utf8 model path".into()))?
            .to_string();
        let audio_str = audio_path
            .to_str()
            .ok_or_else(|| TranscriptionError::Provider("non-utf8 audio path".into()))?
            .to_string();
        let stem_str = stem
            .to_str()
            .ok_or_else(|| TranscriptionError::Provider("non-utf8 stem".into()))?
            .to_string();

        // Whisper-cli ожидает BCP47-короткий код. opts.lang = 'auto' маппим
        // в 'auto' (whisper-cli accepts it и сам детектит).
        let lang = normalize_lang(&opts.lang);

        // whisper.cpp `--print-progress` — это bool-флаг **без значения**.
        // Передача `false` как next arg попадала в positional input slot
        // («input file not found 'false'»), whisper-cli не записывал JSON.
        // `--no-prints` уже подавляет всё включая progress callback, так что
        // флаг можно опустить.
        //
        // [Dev] Homebrew-built whisper-cli линкуется к `@rpath/libwhisper.1.dylib`.
        // Если бинарь скопирован в `target/debug/binaries/`, rpath ищет
        // `target/debug/../lib/` (пустой) → dyld fail. Production solution —
        // install_name_tool + bundle dylibs рядом. Dev workaround —
        // `DYLD_FALLBACK_LIBRARY_PATH` → /opt/homebrew/lib.
        let mut sidecar = app
            .shell()
            .sidecar(SIDECAR_NAME)
            .map_err(|e| TranscriptionError::Provider(format!("sidecar lookup: {e}")))?
            .env("DYLD_FALLBACK_LIBRARY_PATH", "/opt/homebrew/lib")
            .args([
                "-m",
                &model_str,
                "-f",
                &audio_str,
                "--output-json-full",
                "-of",
                &stem_str,
                "-l",
                &lang,
                "--threads",
                &format!("{DEFAULT_THREADS}"),
                "--no-prints",
                // [P15.1 bug-fix] Whisper-cli `-t N` = `--threads N`,
                // НЕ temperature! Раньше передавали `-t 0.0` думая что это
                // temperature → клобрило `--threads` setting в 0.
                // Temperature в whisper-cli — `-tp` / `--temperature`.
                // P12.2 anti-hallucination флаги `--no-speech-thold 0.6`,
                // `--entropy-thold 2.4`, `--logprob-thold -1.0` совпадают
                // с whisper-cli defaults — удалили как no-op (см. --help).
                "--temperature",
                "0.0",
                // [P-fix5] КРИТИЧНО: max-context 0 = не кондишенить на
                // предыдущий декодированный текст. Без этого whisper на длинном
                // файле с тишиной/шумом зацикливается (повторяет последнюю фразу
                // / [шум] / эхо-промпт по всему треку, вытесняя реальную речь).
                // Доказано на 392ea1cc: -mc 0 → 79 реальных реплик vs 0 без него.
                // Initial `--prompt` (chunk-context) при этом продолжает работать.
                "--max-context",
                "0",
            ]);

        // [P15.2] VAD silence-trim — если silero-vad model скачана, добавляем
        // `--vad --vad-model <path>`. Дропает silence regions ДО encoder pass
        // → 30-50% wall-clock reduction на pause-heavy calls. Если модель
        // отсутствует, gracefully skip — STT работает без VAD как раньше.
        let vad_model_path = self
            .model_path
            .parent()
            .map(|d| d.join("silero-vad-v5.bin"));
        if let Some(vad_path) = vad_model_path
            .as_ref()
            .filter(|p| p.exists())
            .and_then(|p| p.to_str())
        {
            log::debug!(
                "stt[{:?}]: enabling --vad with model {vad_path}",
                self.track
            );
            sidecar = sidecar.args([
                "--vad",
                "--vad-model",
                vad_path,
                "--vad-threshold",
                "0.5",
                "--vad-min-speech-duration-ms",
                "250",
                "--vad-min-silence-duration-ms",
                "300",
            ]);
        }
        // [M13.1.3a] Context priming через `--prompt` — ТОЛЬКО реальный
        // chunk-context (tail предыдущего chunk'а при live chunked-записи).
        //
        // [P-fix5] Статический language-anchor («Это разговор на русском
        // языке») УБРАН: на тишине/низкой уверенности whisper эхо-зацикливался
        // на промпте (весь mic-трек → «что вы говорите на русском языке»),
        // вытесняя реальную речь. Bias языка теперь обеспечивает явный пин
        // (pick_pinned_lang / call_language), anchor не нужен. Без opts.prompt
        // `--prompt` не передаётся вовсе. Sanitize обязателен (strip \r\n + 1000).
        let prompt_to_use: Option<String> = opts
            .prompt
            .as_deref()
            .map(sanitize_prompt)
            .filter(|p| !p.is_empty());
        if let Some(p) = prompt_to_use.as_deref() {
            sidecar = sidecar.args(["--prompt", p]);
        }

        // [P15.3] STT timing telemetry — RTF (Real-Time Factor) = audio_sec
        // / wall_clock_sec. RTF=10× значит «в 10 раз быстрее реального
        // времени». M-series + Metal на Whisper Medium даёт RTF~15-25×;
        // RTF~1-3× указывает на CPU-only path (Metal backend не загрузился).
        // [Q] Очередь STT-ресурса: 1 permit = 1 spawn whisper-cli (8 потоков).
        // Дорожки одного звонка (tokio::join! у caller'ов) встают друг за
        // другом — ровно один 8-поточный декодер единовременно. Скоуп permit'а
        // — только sidecar-ран; parse JSON уже вне очереди.
        let start = std::time::Instant::now();
        let run_result = {
            let _q = resource_queue::acquire(Resource::Stt, self.queue_call_id.as_deref()).await;
            run_sidecar_with_timeout(sidecar, self.timeout).await
        };
        let elapsed = start.elapsed();
        let parse_result = match run_result {
            Ok(()) => parse_whisper_json(&json_path, self.track).await,
            Err(e) => Err(e),
        };
        if let Ok(ref t) = parse_result {
            let elapsed_sec = elapsed.as_secs_f64();
            let rtf = if elapsed_sec > 0.001 {
                t.duration_sec / elapsed_sec
            } else {
                0.0
            };
            log::info!(
                "stt[{:?}]: {:.1}s audio → {:.1}s wall-clock, RTF={:.2}× ({} segments)",
                self.track,
                t.duration_sec,
                elapsed_sec,
                rtf,
                t.segments.len()
            );
        }

        // [TD-11] cleanup — на temp_guard (Drop, рекурсивно по work_dir).
        // Ручной remove_file убран: не срабатывал при отмене задачи.
        drop(temp_guard);
        parse_result
    }
}

/// [M13.1.3a] Sanitize prompt для `--prompt` arg whisper-cli. Удаляет
/// `\r\n` (capability validator `^[^\r\n]{0,1000}$` запрещает) + truncate
/// до 1000 char-points для compliance. Pure-функция, тестируется отдельно.
pub(crate) fn sanitize_prompt(raw: &str) -> String {
    let mut out: String = raw.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    // Char-aware truncate (не байтовый — иначе режет UTF-8 codepoints).
    if out.chars().count() > 1000 {
        let truncated: String = out.chars().take(1000).collect();
        out = truncated;
    }
    out
}

// [P-fix5] `default_prompt_for_lang` УДАЛЁН: статический language-anchor
// вызывал prompt-echo галлюцинации на тишине (whisper повторял промпт вместо
// речи). Язык теперь пиним явно (pick_pinned_lang / call_language), anchor не
// нужен. Context-priming оставлен только через реальный `opts.prompt`.

/// 'auto' / '' → 'auto'. Иначе нормализуем lowercase, обрезаем по '-' до
/// 2-5 alnum (соответствует capability validator `^[a-z]{2,5}$`).
fn normalize_lang(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return "auto".to_string();
    }
    let head = trimmed
        .split(['-', '_'])
        .next()
        .unwrap_or("auto")
        .to_lowercase();
    if head.chars().all(|c| c.is_ascii_lowercase()) && head.len() >= 2 && head.len() <= 5 {
        head
    } else {
        "auto".to_string()
    }
}

async fn run_sidecar_with_timeout(
    sidecar: tauri_plugin_shell::process::Command,
    timeout: Duration,
) -> Result<(), TranscriptionError> {
    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| TranscriptionError::Provider(format!("sidecar spawn: {e}")))?;
    // [M12.6.4] SidecarGuard — RAII kill при cancel/panic. Без него
    // whisper-cli переживает task abort на часы (large model в processing).
    let mut guard = Some(SidecarGuard::new(child));

    let mut exit_code: Option<i32> = None;
    let drained = tokio::time::timeout(timeout, async {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Terminated(p) => {
                    exit_code = p.code;
                    return Ok::<(), TranscriptionError>(());
                }
                CommandEvent::Error(e) => {
                    return Err(TranscriptionError::Provider(format!("sidecar error: {e}")));
                }
                _ => {}
            }
        }
        Ok(())
    })
    .await;

    match drained {
        Ok(Ok(())) => {
            if let Some(g) = guard.take() {
                g.release();
            }
            if let Some(code) = exit_code {
                if code != 0 {
                    return Err(TranscriptionError::Provider(format!(
                        "whisper-cli exit {code}"
                    )));
                }
            }
            Ok(())
        }
        Ok(Err(e)) => {
            if let Some(g) = guard.take() {
                g.kill();
            }
            Err(e)
        }
        Err(_) => {
            // [PRD §M12.6.4] timeout / cancel → SIGKILL через SidecarGuard.
            // Defense-in-depth: даже если эта ветка не достигнута (task abort),
            // `guard` всё равно убьёт child через Drop при unwind.
            if let Some(g) = guard.take() {
                g.kill();
            }
            Err(TranscriptionError::Provider("local_whisper_timeout".into()))
        }
    }
}

/// JSON-схема whisper.cpp `--output-json-full`. Подмножество — только нужные
/// нам поля.
#[derive(Debug, Deserialize)]
struct WhisperJsonFile {
    #[serde(default)]
    result: Option<WhisperResultMeta>,
    transcription: Vec<WhisperSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperResultMeta {
    #[serde(default)]
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    text: String,
    /// `offsets` есть всегда — таймкоды в миллисекундах (whisper.cpp нативный
    /// формат); `timestamps` (HH:MM:SS) только при `--print-timestamps`.
    offsets: WhisperOffsets,
}

#[derive(Debug, Deserialize)]
struct WhisperOffsets {
    from: i64,
    to: i64,
}

async fn parse_whisper_json(
    path: &Path,
    track: TrackKind,
) -> Result<DiarizedTranscript, TranscriptionError> {
    // [Security M-2] whisper-cli создаёт output JSON с default umask (обычно
    // 0o644). Содержимое — расшифровка звонка, sensitive. Tighten до 0o600
    // ДО чтения чтобы между write и cleanup чужой process не успел прочитать.
    // На non-unix — no-op (Linux/Windows под R9 пока не поддерживаются).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = tokio::fs::metadata(path).await {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = tokio::fs::set_permissions(path, perms).await;
        }
    }
    // [M13 fix] whisper.cpp иногда эмитит невалидный UTF-8 когда multibyte-токен
    // (кириллица / CJK) режется на границе сегмента. Строгий `read_to_string`
    // тогда падал с `stream did not contain valid UTF-8` → весь chunk (или весь
    // звонок на full-file пути) терял расшифровку. Читаем байты + lossy-decode:
    // повреждается максимум один символ (U+FFFD), а не вся дорожка.
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| TranscriptionError::Provider(format!("read whisper json: {e}")))?;
    let raw = String::from_utf8_lossy(&bytes);
    let parsed: WhisperJsonFile = serde_json::from_str(&raw)
        .map_err(|e| TranscriptionError::Provider(format!("parse whisper json: {e}")))?;
    Ok(build_transcript(parsed, track))
}

fn build_transcript(parsed: WhisperJsonFile, track: TrackKind) -> DiarizedTranscript {
    let lang_detected = parsed
        .result
        .as_ref()
        .and_then(|r| r.language.clone())
        .filter(|s| !s.is_empty());
    // [P14.4] Telemetry — сколько segments дропнуто hallucination filter.
    // Без этого regressions невозможно отловить ни в dev'е, ни в prod'е.
    let mut dropped_count: usize = 0;
    let mut empty_count: usize = 0;
    let total_before = parsed.transcription.len();
    let mut segments: Vec<TranscriptSegment> = parsed
        .transcription
        .into_iter()
        .filter_map(|seg| {
            let start = seg.offsets.from as f64 / 1000.0;
            let end = seg.offsets.to as f64 / 1000.0;
            if !start.is_finite() || !end.is_finite() || end < start {
                return None;
            }
            let text = seg.text.trim().to_string();
            if text.is_empty() {
                empty_count += 1;
                return None;
            }
            // [P12.1] Whisper hallucinates на silence / low-confidence
            // фреймах. Comprehensive filter — exact + substring + shape.
            if is_hallucination(&text) {
                log::debug!("stt[{track:?}]: hallucination drop: {text:?}");
                dropped_count += 1;
                return None;
            }
            let speaker_tag = match track {
                TrackKind::MicOwner => "speaker:owner".to_string(),
                TrackKind::System => "speaker:0".to_string(),
            };
            Some(TranscriptSegment {
                start,
                end,
                text,
                speaker_tag,
                confidence: None,
            })
        })
        .collect();
    if dropped_count > 0 || empty_count > 0 {
        log::info!(
            "stt[{track:?}]: filter stats — {dropped_count} hallucinations + {empty_count} empty / {total_before} total → {} kept",
            segments.len()
        );
    }
    let duration_sec = segments.last().map(|s| s.end).unwrap_or(0.0);
    segments.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    DiarizedTranscript {
        version: 1,
        lang_detected,
        duration_sec,
        provider: "local-whisper".to_string(),
        segments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn for_preset_resolves_model_path() {
        let p = LocalWhisperProvider::for_preset(
            Path::new("/data"),
            ModelId::WHISPER_MEDIUM,
            TrackKind::System,
        );
        assert!(
            p.model_path()
                .to_string_lossy()
                .ends_with("/data/local_engine/models/whisper-medium.bin"),
            "got {}",
            p.model_path().display()
        );
        assert_eq!(p.track(), TrackKind::System);
    }

    #[tokio::test]
    async fn transcribe_returns_not_implemented_without_app() {
        let p = LocalWhisperProvider::for_preset(
            Path::new("/data"),
            ModelId::WHISPER_SMALL,
            TrackKind::MicOwner,
        );
        let opts = TranscriptionOpts {
            lang: "ru".to_string(),
            diarization: true,
            prompt: None,
        };
        let err = p
            .transcribe(Path::new("/tmp/missing.wav"), opts)
            .await
            .expect_err("без AppHandle → NotImplemented");
        assert!(matches!(err, TranscriptionError::NotImplemented));
    }

    // ── normalize_lang ──────────────────────────────────────────────────

    #[test]
    fn normalize_lang_passes_through_short_code() {
        assert_eq!(normalize_lang("ru"), "ru");
        assert_eq!(normalize_lang("en"), "en");
    }

    #[test]
    fn normalize_lang_strips_region() {
        assert_eq!(normalize_lang("ru-RU"), "ru");
        assert_eq!(normalize_lang("en_US"), "en");
    }

    #[test]
    fn normalize_lang_handles_auto_and_empty() {
        assert_eq!(normalize_lang("auto"), "auto");
        assert_eq!(normalize_lang(""), "auto");
        assert_eq!(normalize_lang("   "), "auto");
    }

    #[test]
    fn normalize_lang_rejects_unsafe_characters() {
        // Capability validator `^[a-z]{2,5}$` — должны фолбэк'нуться на 'auto'.
        assert_eq!(normalize_lang("ru;rm"), "auto");
        assert_eq!(normalize_lang("../etc"), "auto");
        assert_eq!(normalize_lang("123"), "auto");
    }

    // ── sanitize_prompt (M13.1.3a) ──────────────────────────────────────

    #[test]
    fn sanitize_prompt_strips_crlf() {
        // Capability validator `^[^\r\n]{0,1000}$` блокирует \r\n.
        assert_eq!(sanitize_prompt("hello\nworld"), "helloworld");
        assert_eq!(sanitize_prompt("a\r\nb"), "ab");
        assert_eq!(sanitize_prompt("без переносов"), "без переносов");
    }

    #[test]
    fn sanitize_prompt_truncates_to_1000_chars() {
        let raw = "a".repeat(1500);
        let out = sanitize_prompt(&raw);
        assert_eq!(out.chars().count(), 1000);
    }

    #[test]
    fn sanitize_prompt_truncates_char_aware_not_byte() {
        // Кириллица — 2 bytes per char. Byte-truncate резал бы UTF-8
        // codepoints. Char-truncate сохраняет валидность.
        let raw = "тест ".repeat(300); // ~1500 chars
        let out = sanitize_prompt(&raw);
        assert_eq!(out.chars().count(), 1000);
        // Should be valid UTF-8 (Rust String guarantees this, но проверим).
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn sanitize_prompt_passes_short_text() {
        assert_eq!(
            sanitize_prompt("last words from chunk"),
            "last words from chunk"
        );
        assert_eq!(sanitize_prompt(""), "");
    }

    // ── build_transcript ────────────────────────────────────────────────

    fn json_with_two_segments() -> WhisperJsonFile {
        WhisperJsonFile {
            result: Some(WhisperResultMeta {
                language: Some("ru".to_string()),
            }),
            transcription: vec![
                WhisperSegment {
                    text: "  Привет.  ".into(),
                    offsets: WhisperOffsets { from: 0, to: 1500 },
                },
                WhisperSegment {
                    text: "Как дела?".into(),
                    offsets: WhisperOffsets {
                        from: 1500,
                        to: 3200,
                    },
                },
            ],
        }
    }

    #[test]
    fn build_transcript_mic_track_uses_owner_speaker() {
        let t = build_transcript(json_with_two_segments(), TrackKind::MicOwner);
        assert_eq!(t.provider, "local-whisper");
        assert_eq!(t.lang_detected.as_deref(), Some("ru"));
        assert_eq!(t.segments.len(), 2);
        assert!(t.segments.iter().all(|s| s.speaker_tag == "speaker:owner"));
        // Текст триммится.
        assert_eq!(t.segments[0].text, "Привет.");
        // Таймкоды в секундах.
        assert!((t.segments[0].start - 0.0).abs() < 1e-9);
        assert!((t.segments[0].end - 1.5).abs() < 1e-9);
        assert!((t.duration_sec - 3.2).abs() < 1e-9);
    }

    #[test]
    fn build_transcript_system_track_tags_speaker_zero() {
        let t = build_transcript(json_with_two_segments(), TrackKind::System);
        assert!(t.segments.iter().all(|s| s.speaker_tag == "speaker:0"));
    }

    #[test]
    fn build_transcript_filters_whisper_hallucinations() {
        let parsed = WhisperJsonFile {
            result: None,
            transcription: vec![
                WhisperSegment {
                    text: " you".into(), // leading space — common whisper artifact
                    offsets: WhisperOffsets { from: 0, to: 300 },
                },
                WhisperSegment {
                    text: "Thank you.".into(), // not in list → keeps
                    offsets: WhisperOffsets { from: 300, to: 900 },
                },
                WhisperSegment {
                    text: "real text".into(),
                    offsets: WhisperOffsets {
                        from: 900,
                        to: 2000,
                    },
                },
                WhisperSegment {
                    text: "(silence)".into(),
                    offsets: WhisperOffsets {
                        from: 2000,
                        to: 2500,
                    },
                },
            ],
        };
        let t = build_transcript(parsed, TrackKind::System);
        // "you" and "(silence)" filtered; "Thank you." (mixed case + period) kept
        assert_eq!(t.segments.len(), 2);
        assert_eq!(t.segments[0].text, "Thank you.");
        assert_eq!(t.segments[1].text, "real text");
    }

    #[test]
    fn build_transcript_filters_empty_and_invalid_segments() {
        let parsed = WhisperJsonFile {
            result: None,
            transcription: vec![
                WhisperSegment {
                    text: "   ".into(),
                    offsets: WhisperOffsets { from: 0, to: 1000 },
                },
                WhisperSegment {
                    text: "bad-range".into(),
                    offsets: WhisperOffsets {
                        from: 5000,
                        to: 1000,
                    },
                },
                WhisperSegment {
                    text: "good".into(),
                    offsets: WhisperOffsets {
                        from: 2000,
                        to: 3000,
                    },
                },
            ],
        };
        let t = build_transcript(parsed, TrackKind::System);
        assert_eq!(t.segments.len(), 1);
        assert_eq!(t.segments[0].text, "good");
        assert!(t.lang_detected.is_none(), "empty language → None");
    }

    #[test]
    fn build_transcript_sorts_by_start() {
        let parsed = WhisperJsonFile {
            result: None,
            transcription: vec![
                WhisperSegment {
                    text: "late".into(),
                    offsets: WhisperOffsets {
                        from: 5000,
                        to: 6000,
                    },
                },
                WhisperSegment {
                    text: "early".into(),
                    offsets: WhisperOffsets { from: 0, to: 1000 },
                },
            ],
        };
        let t = build_transcript(parsed, TrackKind::System);
        assert_eq!(t.segments[0].text, "early");
        assert_eq!(t.segments[1].text, "late");
    }

    #[tokio::test]
    async fn parse_whisper_json_reads_disk_and_decodes() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.json");
        let body = r#"{
            "result": {"language": "en"},
            "transcription": [
                {"text": "Hello", "offsets": {"from": 0, "to": 500}}
            ]
        }"#;
        tokio::fs::write(&path, body).await.unwrap();
        let t = parse_whisper_json(&path, TrackKind::System).await.unwrap();
        assert_eq!(t.lang_detected.as_deref(), Some("en"));
        assert_eq!(t.segments.len(), 1);
        assert_eq!(t.segments[0].text, "Hello");
    }

    #[tokio::test]
    async fn parse_whisper_json_errors_on_malformed() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.json");
        tokio::fs::write(&path, "{ not json }").await.unwrap();
        let err = parse_whisper_json(&path, TrackKind::System)
            .await
            .expect_err("malformed → Err");
        assert!(matches!(err, TranscriptionError::Provider(_)));
    }

    /// [M13 fix] whisper.cpp может вставить невалидный UTF-8 байт когда режет
    /// кириллический токен на границе сегмента. Раньше `read_to_string` падал
    /// на весь chunk. Теперь lossy-decode сохраняет валидные сегменты, портит
    /// максимум один символ.
    #[tokio::test]
    async fn parse_whisper_json_survives_invalid_utf8() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.json");
        // Валидный JSON, но с сырым байтом 0xFF внутри строки (invalid UTF-8).
        // ASCII-каркас — byte-string; кириллица — через .as_bytes() (byte-string
        // литералы не принимают non-ASCII).
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(br#"{"result":{"language":"ru"},"transcription":[{"text":""#);
        body.extend_from_slice("Привет ".as_bytes());
        body.push(0xFF); // lone invalid byte (whisper.cpp split-token artifact)
        body.extend_from_slice("мир".as_bytes());
        body.extend_from_slice(br#"","offsets":{"from":0,"to":500}}]}"#);
        tokio::fs::write(&path, &body).await.unwrap();

        let t = parse_whisper_json(&path, TrackKind::System)
            .await
            .expect("lossy decode должен спасти chunk, не падать");
        assert_eq!(t.lang_detected.as_deref(), Some("ru"));
        assert_eq!(t.segments.len(), 1, "сегмент сохранён несмотря на bad byte");
        assert!(
            t.segments[0].text.starts_with("Привет"),
            "текст до bad byte цел: {}",
            t.segments[0].text
        );
    }
}
