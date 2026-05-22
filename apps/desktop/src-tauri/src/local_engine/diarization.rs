//! [M12.2] LocalDiarizer — internal интерфейс диаризации.
//!
//! В отличие от cloud-провайдеров (Soniox/Gladia делают STT+диаризацию в
//! одном вызове), local движок разделяет: STT (M12.1) → отдельная диаризация
//! (этот модуль) → merge timestamps (PRD §M12.2.3).
//!
//! Реализация — sherpa-onnx `OfflineSpeakerDiarization`:
//! - Segmentation: pyannote-segmentation-3-0 (~6 MB, MODEL_CATALOG entry
//!   `pyannote-segmentation`).
//! - Embedding: WeSpeaker (`voice_model.rs`, ~26 MB, B3.7c reuse).
//! - Clustering: `FastClusteringConfig` дефолт (k auto-detected).
//! - Cap = 4 спикера (R12 / PRD §M12.2.5).
//!
//! Real wire-up за `#[cfg(feature = "voice-onnx")]` чтобы default build
//! не тянул heavy ONNX runtime (~30 МБ static lib).
//!
//! # Owner-bind (M3.7, PRD §M12.2.4)
//!
//! Mic-дорожка не диаризуется — это всегда `speaker:owner`. В пайплайне
//! только system-дорожка попадает сюда. Owner-bind происходит на merge step.

use std::path::{Path, PathBuf};

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

/// Sherpa-onnx-based diarizer. Конструируется в pipeline после resolve
/// преsеta + presence check моделей.
///
/// Real implementation за `voice-onnx` feature. Без feature `diarize()`
/// возвращает `NotImplemented` — pipeline должен фолбэк'нуться (для local
/// route это означает «single-bucket system track», degraded но рабочий).
pub struct SortformerDiarizer {
    segmentation_path: PathBuf,
    embedding_path: PathBuf,
}

impl SortformerDiarizer {
    /// Конструктор требует оба пути. Pipeline resolves их из MODEL_CATALOG +
    /// `voice_model::model_path` для WeSpeaker.
    pub fn new(segmentation_path: PathBuf, embedding_path: PathBuf) -> Self {
        Self {
            segmentation_path,
            embedding_path,
        }
    }

    /// Доступ к paths для тестов / диагностики.
    #[allow(dead_code)]
    pub fn segmentation_path(&self) -> &Path {
        &self.segmentation_path
    }

    #[allow(dead_code)]
    pub fn embedding_path(&self) -> &Path {
        &self.embedding_path
    }
}

#[async_trait]
impl Diarizer for SortformerDiarizer {
    async fn diarize(&self, _audio: &Path) -> Result<Vec<SpeakerSegment>, DiarizerError> {
        #[cfg(feature = "voice-onnx")]
        {
            return self.diarize_real(_audio).await;
        }
        #[cfg(not(feature = "voice-onnx"))]
        {
            Err(DiarizerError::NotImplemented)
        }
    }
}

#[cfg(feature = "voice-onnx")]
impl SortformerDiarizer {
    /// Real sherpa-onnx wire-up. Шаги:
    /// 1. Wave::read(audio) → samples f32 mono 16 kHz.
    /// 2. OfflineSpeakerDiarization::create(config) с paths к pyannote + WeSpeaker.
    /// 3. .process(samples) → result.sort_by_start_time().
    /// 4. Cap = 4 + map в SpeakerSegment.
    async fn diarize_real(&self, audio: &Path) -> Result<Vec<SpeakerSegment>, DiarizerError> {
        use sherpa_onnx::{
            FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
            OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
            SpeakerEmbeddingExtractorConfig, Wave,
        };

        // Pre-flight: оба файла должны быть на диске.
        if !self.segmentation_path.exists() {
            return Err(DiarizerError::ModelNotFound(
                self.segmentation_path.display().to_string(),
            ));
        }
        if !self.embedding_path.exists() {
            return Err(DiarizerError::ModelNotFound(
                self.embedding_path.display().to_string(),
            ));
        }

        let audio_str = audio
            .to_str()
            .ok_or_else(|| DiarizerError::Provider("non-utf8 audio path".into()))?
            .to_string();
        let seg_str = self
            .segmentation_path
            .to_str()
            .ok_or_else(|| DiarizerError::Provider("non-utf8 segmentation path".into()))?
            .to_string();
        let emb_str = self
            .embedding_path
            .to_str()
            .ok_or_else(|| DiarizerError::Provider("non-utf8 embedding path".into()))?
            .to_string();

        // sherpa-onnx APIs синхронные и могут блокировать долго (минута+
        // на большом файле). Запускаем на blocking pool чтобы не залипать
        // в async runtime.
        let segments = tokio::task::spawn_blocking(move || {
            let wave = Wave::read(&audio_str).ok_or_else(|| {
                DiarizerError::Provider(format!("Wave::read failed for {audio_str}"))
            })?;

            let mut config = OfflineSpeakerDiarizationConfig::default();
            config.segmentation = OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: Some(seg_str),
                },
                ..Default::default()
            };
            config.embedding = SpeakerEmbeddingExtractorConfig {
                model: Some(emb_str),
                ..Default::default()
            };
            config.clustering = FastClusteringConfig::default();

            let diar = OfflineSpeakerDiarization::create(&config).ok_or_else(|| {
                DiarizerError::Provider(
                    "OfflineSpeakerDiarization::create returned None (model load failed)".into(),
                )
            })?;

            let result = diar.process(wave.samples()).ok_or_else(|| {
                DiarizerError::Provider("OfflineSpeakerDiarization::process returned None".into())
            })?;

            let raw_segments: Vec<SpeakerSegment> = result
                .sort_by_start_time()
                .into_iter()
                .map(|s| SpeakerSegment {
                    start: s.start as f64,
                    end: s.end as f64,
                    speaker_tag: cap_speaker_tag(s.speaker as usize),
                })
                .collect();

            Ok::<Vec<SpeakerSegment>, DiarizerError>(raw_segments)
        })
        .await
        .map_err(|e| DiarizerError::Provider(format!("blocking task join: {e}")))??;

        Ok(segments)
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
            // [Review L2] `unwrap_or_else` evaluates clone только когда
            // parse_speaker_index вернул None — `unwrap_or` всегда клонировал
            // даже на успешном parse.
            let cap_tag = parse_speaker_index(&s.speaker_tag)
                .map(cap_speaker_tag)
                .unwrap_or_else(|| s.speaker_tag.clone());
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

    #[test]
    fn sortformer_stores_both_paths() {
        let d = SortformerDiarizer::new("/tmp/seg.onnx".into(), "/tmp/emb.onnx".into());
        assert_eq!(d.segmentation_path(), Path::new("/tmp/seg.onnx"));
        assert_eq!(d.embedding_path(), Path::new("/tmp/emb.onnx"));
    }

    #[cfg(not(feature = "voice-onnx"))]
    #[tokio::test]
    async fn sortformer_stub_returns_not_implemented_without_feature() {
        // Default build (no voice-onnx) — diarize всегда NotImplemented.
        let d = SortformerDiarizer::new("/tmp/seg.onnx".into(), "/tmp/emb.onnx".into());
        let err = d
            .diarize(Path::new("/tmp/no.wav"))
            .await
            .expect_err("stub must error");
        assert!(matches!(err, DiarizerError::NotImplemented));
    }

    #[cfg(feature = "voice-onnx")]
    #[tokio::test]
    async fn diarize_real_fails_on_missing_segmentation_model() {
        // Real path: первая проверка — наличие model файлов. На fake
        // путях возвращаем ModelNotFound, не ONNX panic.
        let d = SortformerDiarizer::new(
            "/tmp/does-not-exist-seg.onnx".into(),
            "/tmp/does-not-exist-emb.onnx".into(),
        );
        let err = d.diarize(Path::new("/tmp/no.wav")).await.expect_err("err");
        assert!(matches!(err, DiarizerError::ModelNotFound(_)));
    }
}
