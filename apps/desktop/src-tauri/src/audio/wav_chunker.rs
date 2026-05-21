//! [B3.2] WAV chunk reader для voice embedding extraction.
//!
//! Sidecar пишет 16kHz mono int16 PCM WAV (см. `WAVWriter` в Swift).
//! Этот модуль читает кусок WAV по `[start_sec, end_sec]` interval и
//! возвращает f32 PCM samples нормализованные `-1.0..1.0`.
//!
//! Не resample'ит — sidecar уже даёт 16kHz, что target для WeSpeaker /
//! ECAPA. Если WAV в другом sample rate (legacy / external) — caller
//! должен resample отдельно.

use std::path::Path;

use hound::WavReader;

use crate::AppError;

/// Прочитать кусок WAV в `[start_sec, end_sec]`, вернуть mono f32 samples.
/// Multi-channel WAV свернут в mono через avg по channels.
///
/// Errors:
///  - WAV не открывается (path missing / corrupt)
///  - Spec не PCM (мы пишем PCM int16, другие формы — fail-fast)
///  - end_sec < start_sec (контракт)
pub fn read_wav_segment(path: &Path, start_sec: f32, end_sec: f32) -> Result<WavSegment, AppError> {
    if end_sec < start_sec {
        return Err(AppError::Other(format!(
            "wav_chunker: end_sec ({end_sec}) < start_sec ({start_sec})"
        )));
    }
    let mut reader = WavReader::open(path)
        .map_err(|e| AppError::Other(format!("wav open {}: {e}", path.display())))?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int {
        return Err(AppError::Other(format!(
            "wav_chunker: unsupported sample format {:?} (need PCM int16)",
            spec.sample_format
        )));
    }
    if spec.bits_per_sample != 16 {
        return Err(AppError::Other(format!(
            "wav_chunker: unsupported bits_per_sample {} (need 16)",
            spec.bits_per_sample
        )));
    }
    let channels = spec.channels as usize;
    if channels == 0 {
        return Err(AppError::Other("wav_chunker: 0-channel WAV".into()));
    }
    let sample_rate = spec.sample_rate;

    // start_frame, end_frame в frames (не samples — для multi-channel WAV
    // 1 frame = `channels` samples).
    let total_frames = reader.duration() as u64;
    let start_frame = ((start_sec.max(0.0) as f64) * sample_rate as f64).round() as u64;
    let end_frame = ((end_sec.max(0.0) as f64) * sample_rate as f64).round() as u64;
    let start_frame = start_frame.min(total_frames);
    let end_frame = end_frame.min(total_frames);
    if end_frame <= start_frame {
        return Ok(WavSegment {
            sample_rate,
            samples: Vec::new(),
        });
    }

    // Seek to start_frame. hound WavReader умеет seek по frames.
    reader
        .seek(start_frame as u32)
        .map_err(|e| AppError::Other(format!("wav seek: {e}")))?;

    let frame_count = (end_frame - start_frame) as usize;
    let sample_count = frame_count * channels;
    let mut samples_i16: Vec<i16> = Vec::with_capacity(sample_count);
    for s in reader.samples::<i16>().take(sample_count) {
        let v = s.map_err(|e| AppError::Other(format!("wav read sample: {e}")))?;
        samples_i16.push(v);
    }

    // Convert i16 → f32 normalize, sum channels if multi-channel.
    let mut samples_f32 = Vec::with_capacity(frame_count);
    if channels == 1 {
        for v in samples_i16 {
            samples_f32.push((v as f32) / 32768.0);
        }
    } else {
        for chunk in samples_i16.chunks_exact(channels) {
            let sum: f32 = chunk.iter().map(|v| (*v as f32) / 32768.0).sum();
            samples_f32.push(sum / channels as f32);
        }
    }

    Ok(WavSegment {
        sample_rate,
        samples: samples_f32,
    })
}

#[derive(Debug, Clone)]
pub struct WavSegment {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::WavWriter;
    use std::path::PathBuf;

    fn write_test_wav(path: &Path, sample_rate: u32, samples: &[i16]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = WavWriter::create(path, spec).unwrap();
        for s in samples {
            w.write_sample(*s).unwrap();
        }
        w.finalize().unwrap();
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("wotold-wav-test-{name}.wav"));
        p
    }

    #[test]
    fn reads_full_segment() {
        let path = temp_path("full");
        let samples: Vec<i16> = (0..16_000)
            .map(|i| ((i as f32 * 0.01).sin() * 16_000.0) as i16)
            .collect();
        write_test_wav(&path, 16_000, &samples);

        let seg = read_wav_segment(&path, 0.0, 1.0).unwrap();
        assert_eq!(seg.sample_rate, 16_000);
        assert_eq!(seg.samples.len(), 16_000);
        // f32 normalized to ±0.5 range roughly.
        let max = seg.samples.iter().cloned().fold(0.0_f32, f32::max);
        assert!(max > 0.3 && max < 0.6);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reads_middle_segment() {
        let path = temp_path("middle");
        let samples: Vec<i16> = (0..32_000).map(|i| (i as i16) % 1000).collect();
        write_test_wav(&path, 16_000, &samples);

        // Read [0.5s, 1.5s] = 16000 frames.
        let seg = read_wav_segment(&path, 0.5, 1.5).unwrap();
        assert_eq!(seg.samples.len(), 16_000);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_segment_returns_empty() {
        let path = temp_path("empty");
        write_test_wav(&path, 16_000, &vec![0; 16_000]);

        let seg = read_wav_segment(&path, 0.5, 0.5).unwrap();
        assert!(seg.samples.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn out_of_bounds_clamped() {
        let path = temp_path("clamp");
        write_test_wav(&path, 16_000, &vec![0; 16_000]);

        // Request 5s window over 1s WAV — должно вернуть только то что есть.
        let seg = read_wav_segment(&path, 0.0, 5.0).unwrap();
        assert_eq!(seg.samples.len(), 16_000);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn end_before_start_errors() {
        let path = temp_path("bad-range");
        write_test_wav(&path, 16_000, &vec![0; 1000]);

        let r = read_wav_segment(&path, 1.0, 0.5);
        assert!(r.is_err());
        let _ = std::fs::remove_file(&path);
    }
}
