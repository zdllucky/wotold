//! Парсинг JSON от whisper.cpp (`--output-json-full`) в `DiarizedTranscript`.
//!
//! Выделен из `stt.rs` при TD-20 — тот упёрся в лимит когезии 800 строк, а
//! фикс требует читать из JSON поле, которое раньше игнорировалось
//! (инженерное правило 8). Граница естественная: здесь схема сайдкарного
//! вывода и её преобразование, в `stt.rs` — сам запуск сайдкара.

use std::path::Path;

use serde::Deserialize;

use crate::providers::transcription::{DiarizedTranscript, TranscriptSegment, TranscriptionError};

use super::hallucination::{is_ambiguous_filler, is_hallucination, MIN_FILLER_CONFIDENCE};
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
    /// [TD-20] Токены сегмента с per-token вероятностью. `--output-json-full`
    /// отдаёт их всегда, до сих пор мы их выбрасывали. `default` — на случай
    /// вывода без `-full` и старых фикстур.
    #[serde(default)]
    tokens: Vec<WhisperToken>,
}

#[derive(Debug, Deserialize)]
struct WhisperOffsets {
    from: i64,
    to: i64,
}

/// [TD-20] Один токен whisper'а. `p` — вероятность, назначенная моделью.
#[derive(Debug, Deserialize)]
struct WhisperToken {
    text: String,
    #[serde(default)]
    p: f64,
}

/// [TD-20] Средняя вероятность «содержательных» токенов сегмента, `None` если
/// их нет.
///
/// Служебные токены (`[_BEG_]`, `[_TT_103]` и прочие тайминг-маркеры) из
/// подсчёта исключаются: их вероятность отражает уверенность в границе
/// сегмента, а не в словах, и на коротких сегментах перевешивает единственное
/// содержательное слово.
///
/// Зачем вообще: на тишине whisper выдаёт `" you"` c p≈0.14, а на реально
/// произнесённом «You» — та же строка с p≈0.52. Текстовых признаков,
/// различающих эти два случая, не существует; вероятность различает их
/// уверенно. Замеры — в описании TD-20.
fn mean_token_probability(tokens: &[WhisperToken]) -> Option<f64> {
    let ps: Vec<f64> = tokens
        .iter()
        .filter(|t| !is_special_token(&t.text))
        .map(|t| t.p)
        .filter(|p| p.is_finite())
        .collect();
    if ps.is_empty() {
        return None;
    }
    Some(ps.iter().sum::<f64>() / ps.len() as f64)
}

/// Служебные токены whisper'а обёрнуты в `[_…_]` — `[_BEG_]`, `[_TT_NNN_]`,
/// `[_EOT_]`. Именно такая форма, а не любые скобки: `[music]` — это уже
/// содержательный (галлюцинированный) вывод, и его вероятность нам нужна.
fn is_special_token(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("[_") && t.ends_with(']')
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
    // [TD-20] Отдельные счётчики по спорным формулам: сколько удалено по
    // низкой уверенности и сколько сохранено благодаря высокой. Без этого
    // сдвиг порога виден только по жалобе пользователя (правило 3).
    let mut filler_dropped: usize = 0;
    let mut filler_kept: usize = 0;
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
            // [TD-20] Уверенность модели в словах сегмента. Считается здесь —
            // это единственное место, где токены вообще доступны; дальше по
            // пайплайну сигнал едет в `TranscriptSegment.confidence`.
            let confidence = mean_token_probability(&seg.tokens);
            // [P12.1] Whisper hallucinates на silence / low-confidence
            // фреймах. Comprehensive filter — exact + substring + shape.
            let ambiguous = is_ambiguous_filler(&text);
            if is_hallucination(&text, confidence) {
                // [security-scan W5] Текст реплики в лог не пишем — это речь
                // из звонка. Для диагностики фильтра хватает длины и
                // уверенности.
                log::debug!(
                    "stt[{track:?}]: hallucination drop: {} симв. (p={confidence:?})",
                    text.chars().count()
                );
                dropped_count += 1;
                if ambiguous {
                    filler_dropped += 1;
                }
                return None;
            }
            if ambiguous {
                filler_kept += 1;
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
                confidence,
            })
        })
        .collect();
    if dropped_count > 0 || empty_count > 0 {
        log::info!(
            "stt[{track:?}]: filter stats — {dropped_count} hallucinations + {empty_count} empty / {total_before} total → {} kept",
            segments.len()
        );
    }
    // [TD-20] Спорные формулы логируются отдельно и всегда, когда встретились:
    // именно по этой строке видно, работает ли порог уверенности.
    if filler_dropped > 0 || filler_kept > 0 {
        log::info!(
            "stt[{track:?}]: спорных формул — {filler_dropped} удалено по низкой уверенности, \
             {filler_kept} сохранено (порог {MIN_FILLER_CONFIDENCE})"
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
                    tokens: Vec::new(),
                },
                WhisperSegment {
                    text: "Как дела?".into(),
                    offsets: WhisperOffsets {
                        from: 1500,
                        to: 3200,
                    },
                    tokens: Vec::new(),
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

    /// Сегмент с одним содержательным токеном заданной вероятности.
    fn seg_with_p(text: &str, from: i64, to: i64, p: f64) -> WhisperSegment {
        WhisperSegment {
            text: text.into(),
            offsets: WhisperOffsets { from, to },
            tokens: vec![
                WhisperToken {
                    text: "[_BEG_]".into(),
                    p: 0.7,
                },
                WhisperToken {
                    text: text.into(),
                    p,
                },
            ],
        }
    }

    #[test]
    fn build_transcript_filters_whisper_hallucinations() {
        // [TD-20] Раньше тест фиксировал, что «you» дропается всегда, а
        // «Thank you.» выживает — и выживало оно случайно, из-за точки:
        // exact-match шёл по голой строке. Теперь решает уверенность.
        let parsed = WhisperJsonFile {
            result: None,
            transcription: vec![
                // leading space — типичный артефакт whisper'а; p как на тишине
                seg_with_p(" you", 0, 300, 0.135),
                seg_with_p("Thank you.", 300, 900, 0.713),
                seg_with_p("real text", 900, 2000, 0.9),
                seg_with_p("(silence)", 2000, 2500, 0.99),
            ],
        };
        let t = build_transcript(parsed, TrackKind::System);
        // «you» дропнут по низкой уверенности, «(silence)» — как артефакт
        // (высокая уверенность его не спасает).
        assert_eq!(t.segments.len(), 2);
        assert_eq!(t.segments[0].text, "Thank you.");
        assert_eq!(t.segments[1].text, "real text");
    }

    #[test]
    fn build_transcript_keeps_confident_short_closing() {
        // Регрессия TD-20: реально произнесённое «You» whisper отдаёт голой
        // строкой без пунктуации — ровно тем же текстом, что галлюцинация на
        // тишине. Отличает их только вероятность (замер: 0.520 против 0.135).
        let parsed = WhisperJsonFile {
            result: None,
            transcription: vec![seg_with_p("you", 0, 800, 0.520)],
        };
        let t = build_transcript(parsed, TrackKind::System);
        assert_eq!(
            t.segments.len(),
            1,
            "произнесённая вслух реплика обязана дожить до расшифровки"
        );
    }

    #[test]
    fn build_transcript_filters_empty_and_invalid_segments() {
        let parsed = WhisperJsonFile {
            result: None,
            transcription: vec![
                WhisperSegment {
                    text: "   ".into(),
                    offsets: WhisperOffsets { from: 0, to: 1000 },
                    tokens: Vec::new(),
                },
                WhisperSegment {
                    text: "bad-range".into(),
                    offsets: WhisperOffsets {
                        from: 5000,
                        to: 1000,
                    },
                    tokens: Vec::new(),
                },
                WhisperSegment {
                    text: "good".into(),
                    offsets: WhisperOffsets {
                        from: 2000,
                        to: 3000,
                    },
                    tokens: Vec::new(),
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
                    tokens: Vec::new(),
                },
                WhisperSegment {
                    text: "early".into(),
                    offsets: WhisperOffsets { from: 0, to: 1000 },
                    tokens: Vec::new(),
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

    // ── [TD-20] confidence из tokens[].p ────────────────────────────────

    /// Реальный вывод whisper-small на 4 секундах чистой тишины. Сегмент —
    /// каноническая галлюцинация `" you"`; служебные токены обрамляют
    /// единственное содержательное слово с p=0.134546.
    const SILENCE_HALLUCINATION_JSON: &str = r#"{
      "result": {"language": "en"},
      "transcription": [{
        "offsets": {"from": 0, "to": 2060},
        "text": " you",
        "tokens": [
          {"text": "[_BEG_]", "offsets": {"from": 0, "to": 0}, "id": 50364, "p": 0.71284},
          {"text": " you", "offsets": {"from": 0, "to": 2060}, "id": 291, "p": 0.134546},
          {"text": "[_TT_103]", "offsets": {"from": 2060, "to": 2060}, "id": 50467, "p": 0.650031}
        ]
      }]
    }"#;

    /// Реальный вывод whisper-small на произнесённом вслух «You». Текст
    /// побайтово совпадает с галлюцинацией выше — различает их только `p`
    /// содержательного токена (0.5199 против 0.1345).
    const SPOKEN_YOU_JSON: &str = r#"{
      "result": {"language": "en"},
      "transcription": [{
        "offsets": {"from": 0, "to": 2000},
        "text": " you",
        "tokens": [
          {"text": "[_BEG_]", "offsets": {"from": 0, "to": 0}, "id": 50364, "p": 0.984292},
          {"text": " you", "offsets": {"from": 0, "to": 2000}, "id": 291, "p": 0.519888},
          {"text": "[_TT_100]", "offsets": {"from": 2000, "to": 2000}, "id": 50464, "p": 0.156107}
        ]
      }]
    }"#;

    #[test]
    fn real_sidecar_fixtures_same_text_opposite_verdicts() {
        // Регрессия TD-20 на настоящих данных сайдкара: одна и та же строка
        // `" you"`, один и тот же формат, одна и та же длительность порядка
        // двух секунд. Никакой текстовой или позиционной эвристики,
        // разделяющей эти два случая, не существует.
        let halluc: WhisperJsonFile = serde_json::from_str(SILENCE_HALLUCINATION_JSON).unwrap();
        let spoken: WhisperJsonFile = serde_json::from_str(SPOKEN_YOU_JSON).unwrap();
        assert_eq!(
            halluc.transcription[0].text, spoken.transcription[0].text,
            "фикстуры обязаны совпадать по тексту, иначе тест ничего не проверяет"
        );

        let h = build_transcript(halluc, TrackKind::System);
        let s = build_transcript(spoken, TrackKind::System);
        assert!(h.segments.is_empty(), "галлюцинация на тишине — удалить");
        assert_eq!(s.segments.len(), 1, "произнесённая реплика — сохранить");
    }

    #[test]
    fn naive_average_over_all_tokens_would_lose_the_signal() {
        // Почему `is_special_token` несущий, а не косметика: среднее по ВСЕМ
        // токенам даёт 0.499 у галлюцинации и 0.553 у речи — оба выше порога
        // 0.30, разделение исчезает. По содержательным токенам — 0.135 против
        // 0.520.
        let halluc: WhisperJsonFile = serde_json::from_str(SILENCE_HALLUCINATION_JSON).unwrap();
        let all: Vec<f64> = halluc.transcription[0].tokens.iter().map(|t| t.p).collect();
        let naive = all.iter().sum::<f64>() / all.len() as f64;
        assert!(
            naive > MIN_FILLER_CONFIDENCE,
            "наивное среднее {naive} не отличило бы галлюцинацию — \
             ровно поэтому служебные токены исключаются"
        );
        let filtered = mean_token_probability(&halluc.transcription[0].tokens).unwrap();
        assert!(filtered < MIN_FILLER_CONFIDENCE, "got {filtered}");
    }

    #[test]
    fn mean_probability_ignores_special_tokens() {
        // Регрессия: наивное среднее по ВСЕМ токенам даёт (0.713+0.135+0.650)/3
        // = 0.499 — служебные маркеры границ вытягивают галлюцинацию почти до
        // уровня реальной речи и сигнал исчезает. Считать надо только по
        // содержательным токенам → 0.135.
        let parsed: WhisperJsonFile = serde_json::from_str(SILENCE_HALLUCINATION_JSON).unwrap();
        let p = mean_token_probability(&parsed.transcription[0].tokens).unwrap();
        assert!(
            (p - 0.134546).abs() < 1e-6,
            "ожидали вероятность единственного содержательного токена, got {p}"
        );
    }

    #[test]
    fn mean_probability_averages_multiple_content_tokens() {
        // Реальный вывод на произнесённом «Thanks, bye.»: 4 содержательных
        // токена, среднее 0.62 — вчетверо выше галлюцинации на тишине.
        let json = r#"{
          "transcription": [{
            "offsets": {"from": 0, "to": 1200},
            "text": " Thanks, bye.",
            "tokens": [
              {"text": "[_BEG_]", "p": 0.9},
              {"text": " Thanks", "p": 0.857},
              {"text": ",", "p": 0.617},
              {"text": " bye", "p": 0.512},
              {"text": ".", "p": 0.495}
            ]
          }]
        }"#;
        let parsed: WhisperJsonFile = serde_json::from_str(json).unwrap();
        let p = mean_token_probability(&parsed.transcription[0].tokens).unwrap();
        assert!((p - 0.62025).abs() < 1e-4, "got {p}");
    }

    #[test]
    fn mean_probability_is_none_without_content_tokens() {
        // Вывод без `--output-json-full` (или старая фикстура) — токенов нет.
        // Сигнала нет, и это должно быть отличимо от «сигнал есть и он низкий».
        let empty: Vec<WhisperToken> = Vec::new();
        assert!(mean_token_probability(&empty).is_none());
        let only_special: WhisperJsonFile = serde_json::from_str(
            r#"{"transcription":[{"offsets":{"from":0,"to":1},"text":"x",
                "tokens":[{"text":"[_BEG_]","p":0.9},{"text":"[_EOT_]","p":0.8}]}]}"#,
        )
        .unwrap();
        assert!(mean_token_probability(&only_special.transcription[0].tokens).is_none());
    }

    #[test]
    fn build_transcript_carries_confidence_into_segments() {
        // Сквозной путь: сигнал обязан доехать до `TranscriptSegment`, иначе
        // ниже по пайплайну (merge, chunk_runner) его неоткуда взять.
        let json = r#"{
          "transcription": [{
            "offsets": {"from": 0, "to": 1200},
            "text": " Реальная реплика.",
            "tokens": [{"text": "[_BEG_]", "p": 0.9}, {"text": " Реальная реплика.", "p": 0.8}]
          }]
        }"#;
        let parsed: WhisperJsonFile = serde_json::from_str(json).unwrap();
        let t = build_transcript(parsed, TrackKind::System);
        assert_eq!(t.segments.len(), 1);
        assert_eq!(t.segments[0].confidence, Some(0.8));
    }

    #[test]
    fn build_transcript_leaves_confidence_none_when_tokens_absent() {
        let t = build_transcript(json_with_two_segments(), TrackKind::System);
        assert!(
            t.segments.iter().all(|s| s.confidence.is_none()),
            "без токенов confidence обязан остаться None, а не 0.0"
        );
    }

    #[test]
    fn legacy_segment_json_without_confidence_still_parses() {
        // [TD-20] Чанки, записанные до этого изменения, лежат в
        // `call_chunks.transcript_json` вообще без ключа `confidence`.
        // Ассамблея перечитывает их — десериализация обязана дать None, а не
        // упасть, иначе старый звонок нельзя будет пересобрать.
        let legacy = r#"{"start":0.0,"end":1.0,"text":"привет","speakerTag":"speaker:0"}"#;
        let seg: TranscriptSegment = serde_json::from_str(legacy).unwrap();
        assert!(seg.confidence.is_none());
    }
}
