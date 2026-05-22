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

use crate::providers::transcription::{
    DiarizedTranscript, TranscriptSegment, TranscriptionError, TranscriptionOpts,
    TranscriptionProvider,
};

use super::models::{model_path, ModelId};
use super::sidecar::SidecarGuard;

/// Имя whisper.cpp sidecar бинаря.
const SIDECAR_NAME: &str = "wotold-whisper";

/// Sane default — 30 минут аудио должно влезать. Pipeline-runner может
/// override через `with_timeout`.
pub const LOCAL_WHISPER_TIMEOUT: Duration = Duration::from_secs(20 * 60);

const DEFAULT_THREADS: u32 = 6;

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
        }
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

    #[cfg(test)]
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

        // Уникальный stem для output JSON. Whisper-cli добавляет `.json`.
        let stem = self
            .tmp_dir
            .join(format!("wotold-whisper-{}", uuid::Uuid::new_v4()));
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
        super::llm::ensure_path_under(&stem, &self.tmp_dir)
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
        let sidecar = app
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
            ]);

        let run_result = run_sidecar_with_timeout(sidecar, self.timeout).await;
        let parse_result = match run_result {
            Ok(()) => parse_whisper_json(&json_path, self.track).await,
            Err(e) => Err(e),
        };

        // Cleanup temp JSON (best-effort).
        let _ = tokio::fs::remove_file(&json_path).await;

        parse_result
    }
}

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
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| TranscriptionError::Provider(format!("read whisper json: {e}")))?;
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
                return None;
            }
            // Whisper hallucinates common short words/phrases at audio
            // boundaries and silence frames. Filter exact matches only.
            static HALLUCINATIONS: &[&str] = &[
                "you",
                "thank you",
                "thanks",
                "bye",
                "goodbye",
                "thanks for watching",
                "[blank_audio]",
                "(silence)",
                "[music]",
                "(music)",
                "[applause]",
            ];
            if HALLUCINATIONS.contains(&text.to_lowercase().as_str()) {
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
                    offsets: WhisperOffsets { from: 900, to: 2000 },
                },
                WhisperSegment {
                    text: "(silence)".into(),
                    offsets: WhisperOffsets { from: 2000, to: 2500 },
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
}
