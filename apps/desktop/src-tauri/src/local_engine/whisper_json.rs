//! Парсинг JSON от whisper.cpp (`--output-json-full`) в `DiarizedTranscript`.
//!
//! Выделен из `stt.rs` при TD-20 — тот упёрся в лимит когезии 800 строк, а
//! фикс требует читать из JSON поле, которое раньше игнорировалось
//! (инженерное правило 8). Граница естественная: здесь схема сайдкарного
//! вывода и её преобразование, в `stt.rs` — сам запуск сайдкара.

use std::path::Path;

use serde::Deserialize;

use crate::providers::transcription::{DiarizedTranscript, TranscriptSegment, TranscriptionError};

use super::hallucination::is_hallucination;
use super::stt::TrackKind;

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

pub(crate) async fn parse_whisper_json(
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
