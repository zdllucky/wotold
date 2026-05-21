//! [B3.3] Voice cluster extraction после STT.
//!
//! Для каждого `speaker_tag` в merged транскрипте:
//!   1. Собрать сегменты этого тега (filter very short < 0.5s, too noisy)
//!   2. Для каждого сегмента — read WAV chunk через wav_chunker
//!      (owner_tag → mic.wav, иначе system.wav — они уже diarized в
//!      source-track-aware пайплайне)
//!   3. Embedder.extract per chunk → 256-dim vector
//!   4. Mean-pool vectors → cluster vector + L2 normalize
//!
//! Cluster vector затем persists в `call_speakers.cluster_embedding`
//! и используется matching pipeline для suggestion_contact_id.

use std::collections::HashMap;
use std::path::Path;

use crate::audio::wav_chunker::read_wav_segment;
use crate::embeddings::Embedder;
use crate::pipeline::merge::OWNER_TAG;
use crate::providers::transcription::TranscriptSegment;
use crate::AppError;

/// Минимальная длительность сегмента — короче берём шум, embedder возвращает
/// noisy / unstable vectors → искажает cluster.
const MIN_SEGMENT_SEC: f32 = 0.5;
/// Maximum per-segment chunk для embedder — длинные segments cap'ятся (10s
/// достаточно по литературе ECAPA/WeSpeaker для voice ID accuracy).
const MAX_SEGMENT_SEC: f32 = 10.0;
/// Целевой sample rate WeSpeaker / ECAPA — 16kHz. Sidecar пишет ровно это.
const TARGET_SR: u32 = 16_000;

pub type ClusterMap = HashMap<String, Vec<f32>>;

/// Извлечь embedding clusters per speaker_tag из merged транскрипта.
/// owner-segments читаются из `mic_path`, остальные из `system_path`.
pub fn extract_clusters(
    merged: &[TranscriptSegment],
    mic_path: &Path,
    system_path: &Path,
    embedder: &dyn Embedder,
) -> Result<ClusterMap, AppError> {
    let mut by_tag: HashMap<String, Vec<Vec<f32>>> = HashMap::new();

    for seg in merged {
        let tag = seg.speaker_tag.trim();
        if tag.is_empty() {
            continue;
        }
        let dur = (seg.end - seg.start) as f32;
        if dur < MIN_SEGMENT_SEC {
            continue;
        }
        let path = if tag == OWNER_TAG {
            mic_path
        } else {
            system_path
        };
        let seg_end = seg.start as f32 + dur.min(MAX_SEGMENT_SEC);
        let wav = match read_wav_segment(path, seg.start as f32, seg_end) {
            Ok(w) => w,
            Err(e) => {
                log::warn!(
                    "extract_clusters: WAV read fail [{}, {:.2}-{:.2}]: {e}",
                    path.display(),
                    seg.start,
                    seg_end,
                );
                continue;
            }
        };
        if wav.samples.is_empty() {
            continue;
        }
        // Sidecar пишет 16kHz mono, sanity check.
        if wav.sample_rate != TARGET_SR {
            log::warn!(
                "extract_clusters: WAV sr {} != target {} — embedder может потерять точность",
                wav.sample_rate,
                TARGET_SR
            );
        }
        let emb = match embedder.extract(&wav.samples, wav.sample_rate) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("extract_clusters: embedder failed на seg {}: {e}", tag);
                continue;
            }
        };
        if emb.is_empty() {
            continue;
        }
        by_tag.entry(tag.to_string()).or_default().push(emb);
    }

    // Mean-pool per tag + L2 normalize.
    let mut out: ClusterMap = HashMap::new();
    for (tag, vectors) in by_tag {
        if vectors.is_empty() {
            continue;
        }
        let dim = vectors[0].len();
        let mut mean = vec![0.0_f32; dim];
        let mut count = 0_f32;
        for v in &vectors {
            if v.len() != dim {
                continue;
            }
            for (m, x) in mean.iter_mut().zip(v.iter()) {
                *m += *x;
            }
            count += 1.0;
        }
        if count == 0.0 {
            continue;
        }
        for m in mean.iter_mut() {
            *m /= count;
        }
        l2_normalize(&mut mean);
        if mean.iter().any(|v| v.abs() > f32::EPSILON) {
            out.insert(tag, mean);
        }
    }
    Ok(out)
}

fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::Embedder;
    use crate::providers::transcription::TranscriptSegment;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::path::PathBuf;

    /// MockEmbedder: возвращает deterministic vector based на первых
    /// нескольких samples — лёгкая проверка что pipeline передал не пустой
    /// chunk.
    struct CountingEmbedder {
        calls: std::sync::atomic::AtomicUsize,
    }
    impl CountingEmbedder {
        fn new() -> Self {
            Self {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
        fn call_count(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
    }
    impl Embedder for CountingEmbedder {
        fn extract(&self, samples: &[f32], _sr: u32) -> Result<Vec<f32>, AppError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // 4-dim det vector based on basic stats — non-zero чтобы pass
            // normalize check.
            let len = samples.len() as f32;
            let sum: f32 = samples.iter().copied().sum();
            let abs_sum: f32 = samples.iter().map(|x| x.abs()).sum();
            let max = samples.iter().cloned().fold(0.0_f32, f32::max);
            Ok(vec![len.min(1.0), sum, abs_sum, max])
        }
    }

    fn temp_wav(name: &str, secs: f32) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("wotold-cluster-test-{name}.wav"));
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(&p, spec).unwrap();
        let n = (secs * 16_000.0) as usize;
        for i in 0..n {
            w.write_sample(((i as f32 * 0.01).sin() * 10_000.0) as i16)
                .unwrap();
        }
        w.finalize().unwrap();
        p
    }

    fn ts(start: f64, end: f64, tag: &str) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: "x".into(),
            speaker_tag: tag.into(),
            confidence: None,
        }
    }

    #[test]
    fn extracts_per_speaker_tag() {
        let mic = temp_wav("mic-a", 5.0);
        let sys = temp_wav("sys-a", 5.0);
        let embedder = CountingEmbedder::new();
        let merged = vec![
            ts(0.0, 2.0, OWNER_TAG),
            ts(2.0, 4.0, "S1"),
            ts(4.0, 5.0, OWNER_TAG),
        ];
        let clusters = extract_clusters(&merged, &mic, &sys, &embedder).unwrap();
        assert_eq!(clusters.len(), 2);
        assert!(clusters.contains_key(OWNER_TAG));
        assert!(clusters.contains_key("S1"));
        // owner: 2 calls (2 segments), S1: 1 call. Total 3.
        assert_eq!(embedder.call_count(), 3);
        let _ = std::fs::remove_file(&mic);
        let _ = std::fs::remove_file(&sys);
    }

    #[test]
    fn filters_short_segments() {
        let mic = temp_wav("mic-b", 3.0);
        let sys = temp_wav("sys-b", 3.0);
        let embedder = CountingEmbedder::new();
        // 0.2s segments → отбрасываются (< MIN_SEGMENT_SEC).
        let merged = vec![ts(0.0, 0.2, "S1"), ts(0.2, 0.4, "S1")];
        let clusters = extract_clusters(&merged, &mic, &sys, &embedder).unwrap();
        assert!(clusters.is_empty());
        assert_eq!(embedder.call_count(), 0);
        let _ = std::fs::remove_file(&mic);
        let _ = std::fs::remove_file(&sys);
    }

    #[test]
    fn output_l2_normalized() {
        let mic = temp_wav("mic-c", 3.0);
        let sys = temp_wav("sys-c", 3.0);
        let embedder = CountingEmbedder::new();
        let merged = vec![ts(0.0, 2.0, "S1")];
        let clusters = extract_clusters(&merged, &mic, &sys, &embedder).unwrap();
        let cluster = clusters.get("S1").unwrap();
        let norm: f32 = cluster.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm = {norm}");
        let _ = std::fs::remove_file(&mic);
        let _ = std::fs::remove_file(&sys);
    }
}
