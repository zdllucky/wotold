//! Аудио чтение для voice matching (#25 / M3.2).
//!
//! M1.5 (#17): WAV запись chunked в `mic.wav` / `system.wav` (16 kHz mono i16).
//! Этот модуль читает обратно и режет на сегменты по таймштампам STT для
//! per-speaker embedding extraction.
//!
//! Без `hound::WavReader::open(path)` мы бы перетягивали `symphonia` ради
//! одного формата — у нас на write уже фиксирован 16 kHz mono i16 WAV.

use std::path::Path;

use crate::AppError;

/// Декодированный аудио-фрагмент: PCM float сэмплы [-1.0..1.0], mono, и sample rate.
#[derive(Debug, Clone)]
pub struct AudioClip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Прочитать всё WAV-файл целиком. Конвертит i16 → f32 нормализованно.
pub fn read_wav(path: &Path) -> Result<AudioClip, AppError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| AppError::Other(format!("wav open: {e}")))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(AppError::Other(format!(
            "expected mono WAV, got {} channels",
            spec.channels
        )));
    }
    let samples: Result<Vec<f32>, _> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|r| r.map(|s| s as f32 / i16::MAX as f32))
            .collect(),
        hound::SampleFormat::Float => reader.samples::<f32>().collect(),
    };
    let samples = samples.map_err(|e| AppError::Other(format!("wav decode: {e}")))?;
    Ok(AudioClip {
        samples,
        sample_rate: spec.sample_rate,
    })
}

/// Вырезает фрагмент `[start_sec, end_sec)` из WAV. Используется per-speaker
/// для embedding extraction. Возвращает clip с тем же sample_rate.
///
/// Не оптимально для batch-вызова — открывает + декодирует WAV целиком
/// каждый раз. Для batch (#25 pipeline 50+ сегментов одного звонка)
/// используй `extract_segments_batch` — один open и slice.
pub fn extract_segment(path: &Path, start_sec: f64, end_sec: f64) -> Result<AudioClip, AppError> {
    if end_sec <= start_sec {
        return Err(AppError::Other(format!(
            "extract_segment: end_sec {end_sec} <= start_sec {start_sec}"
        )));
    }
    let clip = read_wav(path)?;
    Ok(slice_clip(&clip, start_sec, end_sec))
}

/// [B16 audit P2] Batch slicing — открывает WAV один раз, режет все
/// требуемые segments. Возвращает Vec в том же порядке что входной.
/// Для 100 segments экономит 99 декодирований (linear win).
/// Не вызывается из production до #25 ONNX wire-up — пока scaffold для будущего.
#[allow(dead_code)]
pub fn extract_segments_batch(
    path: &Path,
    ranges: &[(f64, f64)],
) -> Result<Vec<AudioClip>, AppError> {
    for &(s, e) in ranges {
        if e <= s {
            return Err(AppError::Other(format!(
                "extract_segments_batch: end_sec {e} <= start_sec {s}"
            )));
        }
    }
    let clip = read_wav(path)?;
    Ok(ranges
        .iter()
        .map(|&(s, e)| slice_clip(&clip, s, e))
        .collect())
}

fn slice_clip(clip: &AudioClip, start_sec: f64, end_sec: f64) -> AudioClip {
    let start = ((start_sec * clip.sample_rate as f64).max(0.0) as usize).min(clip.samples.len());
    let end = ((end_sec * clip.sample_rate as f64) as usize).min(clip.samples.len());
    AudioClip {
        samples: clip.samples[start..end].to_vec(),
        sample_rate: clip.sample_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_wav(path: &Path, samples: &[i16], sample_rate: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn read_wav_mono_i16_normalizes_to_f32() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let samples: Vec<i16> = vec![0, i16::MAX, i16::MIN, 0];
        write_test_wav(&path, &samples, 16_000);

        let clip = read_wav(&path).unwrap();
        assert_eq!(clip.sample_rate, 16_000);
        assert_eq!(clip.samples.len(), 4);
        assert!((clip.samples[1] - 1.0).abs() < 1e-3);
        assert!((clip.samples[2] - (-1.0)).abs() < 1e-3);
    }

    #[test]
    fn read_wav_rejects_stereo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        writer.write_sample(0_i16).unwrap();
        writer.write_sample(0_i16).unwrap();
        writer.finalize().unwrap();

        let err = read_wav(&path).unwrap_err();
        assert!(matches!(err, AppError::Other(_)));
    }

    #[test]
    fn extract_segment_clips_correct_range() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        // 16000 samples × 1 sec = 1 second of audio. Mark with [0, 1, 2, ...] чтобы видеть offset.
        let samples: Vec<i16> = (0..16_000_i32)
            .map(|i| (i % i16::MAX as i32) as i16)
            .collect();
        write_test_wav(&path, &samples, 16_000);

        let clip = extract_segment(&path, 0.25, 0.5).unwrap();
        // 0.25 → sample 4000, 0.5 → sample 8000 → 4000 samples
        assert_eq!(clip.samples.len(), 4000);
        assert_eq!(clip.sample_rate, 16_000);
    }

    #[test]
    fn extract_segment_clamps_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let samples: Vec<i16> = vec![0; 1000];
        write_test_wav(&path, &samples, 16_000);

        // end_sec за пределами — clamp до end of file (1000 samples = 0.0625s)
        let clip = extract_segment(&path, 0.0, 10.0).unwrap();
        assert_eq!(clip.samples.len(), 1000);
    }

    #[test]
    fn extract_segment_rejects_inverted_range() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        write_test_wav(&path, &[0; 100], 16_000);
        let err = extract_segment(&path, 0.5, 0.25).unwrap_err();
        assert!(matches!(err, AppError::Other(_)));
    }

    #[test]
    fn extract_segments_batch_returns_clips_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        let samples: Vec<i16> = (0..16_000_i32)
            .map(|i| (i % i16::MAX as i32) as i16)
            .collect();
        write_test_wav(&path, &samples, 16_000);

        let clips = extract_segments_batch(&path, &[(0.0, 0.25), (0.5, 0.75), (0.75, 1.0)]).unwrap();
        assert_eq!(clips.len(), 3);
        assert_eq!(clips[0].samples.len(), 4000);
        assert_eq!(clips[1].samples.len(), 4000);
        assert_eq!(clips[2].samples.len(), 4000);
    }

    #[test]
    fn extract_segments_batch_rejects_any_inverted_range() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        write_test_wav(&path, &[0; 16_000], 16_000);
        let err = extract_segments_batch(&path, &[(0.0, 0.5), (0.6, 0.4)]).unwrap_err();
        assert!(matches!(err, AppError::Other(_)));
    }
}
