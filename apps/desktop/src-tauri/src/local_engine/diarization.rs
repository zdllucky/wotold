//! [M12.2] LocalDiarizer — internal интерфейс диаризации.
//!
//! В отличие от cloud-провайдеров (Soniox/Gladia делают STT+диаризацию в
//! одном вызове), local движок разделяет: STT (M12.1) → отдельная диаризация
//! (этот модуль) → merge timestamps (PRD §M12.2.3).
//!
//! Реализация — sherpa-onnx sortformer (3D-Speaker модель). Cap = 4 спикера
//! (R12 / PRD §M12.2.2). Stub сейчас, реальный wire-up — после §14 pre-flight.
//!
//! # Owner-bind (M3.7, PRD §M12.2.4)
//!
//! Mic-дорожка не диаризуется — это всегда `speaker:owner`. В пайплайне
//! только system-дорожка попадает сюда. Owner-bind происходит на merge step.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Сегмент диаризации — таймкод + speaker tag. Совместим со схемой
/// `DiarizedTranscript::segments` (без текста — текст из STT word-timestamps
/// мерджится в [`super::merge`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeakerSegment {
    pub start: f64,
    pub end: f64,
    /// `speaker:N` где N — индекс кластера (0..4). Cap = 4 (PRD §M12.2.5).
    pub speaker_tag: String,
}

/// Hard cap на число спикеров в local-режиме. Лишние объединяются в
/// `speaker_unknown` (PRD §M12.2.5).
pub const MAX_LOCAL_SPEAKERS: usize = 4;

/// Public tag для речи без определённого спикера.
pub const SPEAKER_UNKNOWN: &str = "speaker:unknown";

#[derive(Debug, thiserror::Error)]
pub enum DiarizerError {
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("provider: {0}")]
    Provider(String),
    #[error("not implemented")]
    NotImplemented,
}

/// Diarizer trait. Используется только в local-engine (cloud-провайдеры
/// делают диаризацию сами, как часть STT). См. PRD §M12.2.1.
#[async_trait]
pub trait Diarizer: Send + Sync {
    /// Прогнать диаризацию по WAV. Cap = 4 спикера, лишние → `SPEAKER_UNKNOWN`.
    async fn diarize(&self, audio: &Path) -> Result<Vec<SpeakerSegment>, DiarizerError>;
}

/// Sherpa-onnx sortformer stub. Реальный wire-up — после §14 pre-flight.
pub struct SortformerDiarizer {
    #[allow(dead_code)]
    model_path: std::path::PathBuf,
}

impl SortformerDiarizer {
    pub fn new(model_path: std::path::PathBuf) -> Self {
        Self { model_path }
    }
}

#[async_trait]
impl Diarizer for SortformerDiarizer {
    async fn diarize(&self, _audio: &Path) -> Result<Vec<SpeakerSegment>, DiarizerError> {
        Err(DiarizerError::NotImplemented)
    }
}

/// Свести speaker indices к стабильным тэгам с cap'ом. Лишние (`> MAX_LOCAL_SPEAKERS`)
/// → `SPEAKER_UNKNOWN`. Pure-fn для unit-тестов merge / cap логики.
pub fn cap_speaker_tag(speaker_index: usize) -> String {
    if speaker_index >= MAX_LOCAL_SPEAKERS {
        SPEAKER_UNKNOWN.to_string()
    } else {
        format!("speaker:{speaker_index}")
    }
}

/// Применить cap к произвольному вектору сегментов. Идемпотентно.
pub fn apply_speaker_cap(segments: Vec<SpeakerSegment>) -> Vec<SpeakerSegment> {
    segments
        .into_iter()
        .map(|s| {
            let cap_tag = parse_speaker_index(&s.speaker_tag)
                .map(cap_speaker_tag)
                .unwrap_or(s.speaker_tag.clone());
            SpeakerSegment {
                speaker_tag: cap_tag,
                ..s
            }
        })
        .collect()
}

fn parse_speaker_index(tag: &str) -> Option<usize> {
    tag.strip_prefix("speaker:")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_under_max_keeps_tag() {
        assert_eq!(cap_speaker_tag(0), "speaker:0");
        assert_eq!(cap_speaker_tag(3), "speaker:3");
    }

    #[test]
    fn cap_at_or_above_max_maps_to_unknown() {
        assert_eq!(cap_speaker_tag(4), SPEAKER_UNKNOWN);
        assert_eq!(cap_speaker_tag(7), SPEAKER_UNKNOWN);
    }

    #[test]
    fn apply_speaker_cap_maps_excess_to_unknown() {
        let input = vec![
            SpeakerSegment {
                start: 0.0,
                end: 1.0,
                speaker_tag: "speaker:0".into(),
            },
            SpeakerSegment {
                start: 1.0,
                end: 2.0,
                speaker_tag: "speaker:5".into(),
            },
            SpeakerSegment {
                start: 2.0,
                end: 3.0,
                speaker_tag: "speaker:3".into(),
            },
        ];
        let out = apply_speaker_cap(input);
        assert_eq!(out[0].speaker_tag, "speaker:0");
        assert_eq!(out[1].speaker_tag, SPEAKER_UNKNOWN);
        assert_eq!(out[2].speaker_tag, "speaker:3");
    }

    #[test]
    fn apply_speaker_cap_preserves_non_indexed_tags() {
        // Гарантия: уже cap'нутые / unknown сегменты не падают на parse.
        let input = vec![SpeakerSegment {
            start: 0.0,
            end: 1.0,
            speaker_tag: SPEAKER_UNKNOWN.into(),
        }];
        let out = apply_speaker_cap(input);
        assert_eq!(out[0].speaker_tag, SPEAKER_UNKNOWN);
    }

    #[tokio::test]
    async fn sortformer_stub_returns_not_implemented() {
        let d = SortformerDiarizer::new("/tmp/no.onnx".into());
        let err = d
            .diarize(Path::new("/tmp/no.wav"))
            .await
            .expect_err("stub must error");
        assert!(matches!(err, DiarizerError::NotImplemented));
    }
}
