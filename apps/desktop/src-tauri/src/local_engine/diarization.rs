//! [M12.2] LocalDiarizer — internal интерфейс диаризации.
//!
//! В отличие от cloud-провайдеров (Soniox/Gladia делают STT+диаризацию в
//! одном вызове), local движок разделяет: STT (M12.1) → отдельная диаризация
//! (этот модуль) → merge timestamps (PRD §M12.2.3).
//!
//! Реализация — sherpa-onnx `OfflineSpeakerDiarization`:
//! - Segmentation: pyannote-segmentation-3-0 (~6 MB, MODEL_CATALOG entry
//!   `pyannote-segmentation`).
//! - Embedding: WeSpeaker (каталожная запись `voice-embedder`, ~26 MB).
//! - Clustering: `FastClusteringConfig`, число кластеров определяет sherpa.
//! - Потолок числа спикеров — санитарный, см. `MAX_LOCAL_SPEAKERS`.
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
    /// `speaker:N`, где N — индекс кластера. Сверх `MAX_LOCAL_SPEAKERS` →
    /// `SPEAKER_UNKNOWN`.
    pub speaker_tag: String,
}

/// Санитарный потолок числа спикеров. Индексы сверх него схлопываются в
/// `speaker_unknown`.
///
/// Это не ограничение движка: sherpa-onnx верхней границы не имеет, а в
/// авто-режиме сам решает, сколько кластеров выделить. Потолок держит разбег
/// кластеризации в пределах осмысленного — за десятком различимых голосов в
/// одном звонке пер-спикерная атрибуция всё равно превращается в шум.
///
/// [P14.3] Раньше здесь стояло 3 — «четвёртый кластер на микрофоне почти
/// всегда фантом». Побочный эффект оказался хуже причины: реальные четвёртый
/// и пятый собеседники не просто не показывались, а через
/// `reassign_unknown_to_neighbors` молча приписывались тем, кто говорил рядом
/// по времени. Фантомы тройка всё равно не предотвращала — только ограничивала
/// их количество тремя. Настоящий рычаг против переразбиения — порог
/// кластеризации в `FastClusteringConfig`, а не переименование постфактум.
pub const MAX_LOCAL_SPEAKERS: usize = 10;

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
    /// Прогнать диаризацию по WAV. Индексы сверх `MAX_LOCAL_SPEAKERS` →
    /// `SPEAKER_UNKNOWN`.
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
    /// [Q] call_id для QueueMonitor: чей звонок держит/ждёт диаризацию.
    queue_call_id: Option<String>,
}

impl SortformerDiarizer {
    /// Конструктор требует оба пути. Pipeline resolves их из MODEL_CATALOG +
    /// `embeddings::voice_embedder_path` для WeSpeaker.
    pub fn new(segmentation_path: PathBuf, embedding_path: PathBuf) -> Self {
        Self {
            segmentation_path,
            embedding_path,
            queue_call_id: None,
        }
    }

    /// [Q] Привязать call_id к очереди диаризации (QueueMonitor).
    pub fn with_call(mut self, call_id: impl Into<String>) -> Self {
        self.queue_call_id = Some(call_id.into());
        self
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
    /// 4. Санитарный потолок + map в SpeakerSegment.
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

        // [Q] Очередь диаризации: CPU-bound ONNX, permit=1. Permit ПЕРЕЕЗЖАЕТ
        // внутрь blocking-closure: abort task'а НЕ прерывает spawn_blocking,
        // поэтому ресурс честно числится busy до реального конца sherpa —
        // раннее освобождение дало бы две параллельные диаризации.
        let queue_permit = crate::pipeline::resource_queue::acquire(
            crate::pipeline::resource_queue::Resource::Diarization,
            self.queue_call_id.as_deref(),
        )
        .await;

        // sherpa-onnx APIs синхронные и могут блокировать долго (минута+
        // на большом файле). Запускаем на blocking pool чтобы не залипать
        // в async runtime.
        let segments = tokio::task::spawn_blocking(move || {
            let _q = queue_permit;
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
            // [Bug-fix] Default threshold 0.5 (cosine distance) слишком высокий
            // для коротких mic-записей с похожими голосами — sortformer
            // сливает 2 спикеров в 1 кластер. 0.5 → 0.4 — более агрессивный
            // split при сохранении устойчивости к шуму в одной паузе. Это и
            // есть рычаг против переразбиения: число кластеров задаёт порог, а
            // не ручное «форсировать N».
            //
            // `num_clusters: -1` — авто-режим sherpa. Санитарный потолок
            // применяется после инференса, в `cap_speaker_tag`.
            config.clustering = FastClusteringConfig {
                num_clusters: -1,
                threshold: 0.4,
            };

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

/// [P14.3] Reassign `SPEAKER_UNKNOWN` сегменты к ближайшему по timestamp
/// named-спикеру в пределах `window_sec`. Снижает визуальный шум
/// в ParticipantsRow / транскрипте, когда sortformer выделил больше
/// `MAX_LOCAL_SPEAKERS` кластеров и overflow ушёл в unknown.
///
/// **Heuristic.** Для каждого unknown сегмента ищем neighbor (не unknown)
/// с минимальным временным расстоянием. Если в `window_sec` (по обе
/// стороны) такой neighbor найден — наследуем его tag. Иначе оставляем
/// unknown (вероятно реальный «дополнительный голос», не overflow noise).
///
/// **Idempotent.** Повторный вызов не меняет результат (нет unknown'ов
/// для reassign'а после первого прохода + window не растягивается).
///
/// **Single pass.** Решение fix'ится по input snapshot чтобы избежать
/// race effect (если сосед unknown reassign'ился к speaker:0, не хотим
/// чтобы следующий unknown через него же подхватил speaker:0 не имея
/// прямого соседства).
///
/// Returns: количество reassigned segments.
pub fn reassign_unknown_to_neighbors(segments: &mut [SpeakerSegment], window_sec: f64) -> usize {
    if segments.is_empty() {
        return 0;
    }
    // Snapshot tags ДО mutation — predicate fixed на input state.
    let original_tags: Vec<String> = segments.iter().map(|s| s.speaker_tag.clone()).collect();
    let mut reassigned = 0usize;
    for i in 0..segments.len() {
        if original_tags[i] != SPEAKER_UNKNOWN {
            continue;
        }
        let center_start = segments[i].start;
        let center_end = segments[i].end;
        let mut best: Option<(f64, String)> = None;
        for (j, j_tag) in original_tags.iter().enumerate() {
            if i == j || j_tag == SPEAKER_UNKNOWN {
                continue;
            }
            // Минимальное расстояние между интервалами [start,end]. Три кейса:
            // - j заканчивается до начала i → gap = center.start - j.end
            // - j начинается после конца i → gap = j.start - center.end
            // - overlap (или один внутри другого) → 0
            let dist = if segments[j].end < center_start {
                center_start - segments[j].end
            } else if segments[j].start > center_end {
                segments[j].start - center_end
            } else {
                0.0
            };
            if dist <= window_sec && best.as_ref().map(|(d, _)| dist < *d).unwrap_or(true) {
                best = Some((dist, j_tag.clone()));
            }
        }
        if let Some((_, tag)) = best {
            segments[i].speaker_tag = tag;
            reassigned += 1;
        }
    }
    if reassigned > 0 {
        log::info!(
            "reassign_unknown_to_neighbors: {reassigned}/{} unknown → neighbor tag (window={}s)",
            segments.len(),
            window_sec
        );
    }
    reassigned
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Индексы до потолка остаются настоящими спикерами.
    ///
    /// Проверка идёт по конкретным номерам, а не по константе: смысл теста —
    /// поймать возврат к прежней тройке, когда реальные четвёртый и пятый
    /// собеседники молча приписывались тем, кто говорил рядом по времени.
    #[test]
    fn cap_keeps_tags_below_the_ceiling() {
        assert_eq!(cap_speaker_tag(0), "speaker:0");
        assert_eq!(cap_speaker_tag(2), "speaker:2");
        assert_eq!(cap_speaker_tag(4), "speaker:4");
        assert_eq!(cap_speaker_tag(9), "speaker:9");
    }

    #[test]
    fn cap_at_or_above_the_ceiling_maps_to_unknown() {
        assert_eq!(cap_speaker_tag(MAX_LOCAL_SPEAKERS), SPEAKER_UNKNOWN);
        assert_eq!(cap_speaker_tag(42), SPEAKER_UNKNOWN);
    }

    #[test]
    fn sortformer_stores_both_paths() {
        let d = SortformerDiarizer::new("/tmp/seg.onnx".into(), "/tmp/emb.onnx".into());
        assert_eq!(d.segmentation_path(), Path::new("/tmp/seg.onnx"));
        assert_eq!(d.embedding_path(), Path::new("/tmp/emb.onnx"));
    }

    // [P14.3] reassign_unknown_to_neighbors — overflow noise mitigation.

    fn seg(start: f64, end: f64, tag: &str) -> SpeakerSegment {
        SpeakerSegment {
            start,
            end,
            speaker_tag: tag.into(),
        }
    }

    #[test]
    fn reassign_unknown_to_nearest_named() {
        let mut segs = vec![
            seg(0.0, 5.0, "speaker:0"),
            seg(5.0, 6.0, SPEAKER_UNKNOWN), // прямо рядом с speaker:0
            seg(6.0, 10.0, "speaker:1"),
        ];
        let n = reassign_unknown_to_neighbors(&mut segs, 2.0);
        assert_eq!(n, 1);
        // Дистанция 0 в обе стороны — ties → first wins (speaker:0).
        assert_eq!(segs[1].speaker_tag, "speaker:0");
    }

    #[test]
    fn reassign_unknown_keeps_when_no_neighbor_in_window() {
        let mut segs = vec![
            seg(0.0, 5.0, "speaker:0"),
            seg(100.0, 101.0, SPEAKER_UNKNOWN), // далеко
        ];
        let n = reassign_unknown_to_neighbors(&mut segs, 2.0);
        assert_eq!(n, 0);
        assert_eq!(segs[1].speaker_tag, SPEAKER_UNKNOWN);
    }

    #[test]
    fn reassign_unknown_skips_when_only_unknowns() {
        let mut segs = vec![
            seg(0.0, 1.0, SPEAKER_UNKNOWN),
            seg(2.0, 3.0, SPEAKER_UNKNOWN),
        ];
        let n = reassign_unknown_to_neighbors(&mut segs, 5.0);
        assert_eq!(n, 0);
        assert!(segs.iter().all(|s| s.speaker_tag == SPEAKER_UNKNOWN));
    }

    #[test]
    fn reassign_unknown_idempotent() {
        let mut segs = vec![seg(0.0, 5.0, "speaker:0"), seg(5.0, 6.0, SPEAKER_UNKNOWN)];
        reassign_unknown_to_neighbors(&mut segs, 2.0);
        let n = reassign_unknown_to_neighbors(&mut segs, 2.0);
        assert_eq!(n, 0, "повторный вызов — no-op");
    }

    #[test]
    fn reassign_unknown_picks_closer_when_two_neighbors() {
        // speaker:0 ближе (gap=1s) чем speaker:1 (gap=10s).
        let mut segs = vec![
            seg(0.0, 4.0, "speaker:0"),
            seg(5.0, 6.0, SPEAKER_UNKNOWN),
            seg(16.0, 20.0, "speaker:1"),
        ];
        let n = reassign_unknown_to_neighbors(&mut segs, 30.0);
        assert_eq!(n, 1);
        assert_eq!(segs[1].speaker_tag, "speaker:0");
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
