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

/// Прочитать WAV-файл целиком, mono f32 в `[-1.0, 1.0)`.
///
/// [TD-38] Делегирует в `audio::wav_chunker` — тот же декодер, что режет
/// сегменты для voice-эмбеддингов. До слияния это были два независимых ридера
/// одного и того же формата, расходившихся в поведении: здесь делили на
/// `i16::MAX` и падали на stereo, там делили на 32768 и сворачивали каналы
/// усреднением. Осталась одна реализация-надмножество; этот модуль отвечает
/// только за нарезку по таймштампам.
pub fn read_wav(path: &Path) -> Result<AudioClip, AppError> {
    let seg = crate::audio::wav_chunker::read_wav_full(path)?;
    Ok(AudioClip {
        samples: seg.samples,
        sample_rate: seg.sample_rate,
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
    fn read_wav_folds_stereo_to_mono_by_averaging() {
        // [TD-38] Поведение изменилось намеренно: раньше `read_wav` падал на
        // stereo, а `wav_chunker` (тот, что реально кормит voice-эмбеддинги)
        // усреднял каналы. Ридер теперь один — усреднение победило: для
        // биометрии два канала полезнее ошибки.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        // Один фрейм: L = +полная шкала, R = 0 → mono ≈ 0.5.
        writer.write_sample(i16::MAX).unwrap();
        writer.write_sample(0_i16).unwrap();
        writer.finalize().unwrap();

        let clip = read_wav(&path).unwrap();
        assert_eq!(clip.samples.len(), 1, "2 канала → 1 mono-фрейм");
        assert!(
            (clip.samples[0] - 0.5).abs() < 1e-3,
            "got {}",
            clip.samples[0]
        );
    }

    #[test]
    fn read_wav_and_read_wav_segment_agree_on_same_file() {
        // [TD-38] Главный инвариант слияния: оба публичных API дают
        // побитово те же сэмплы. До фикса они делили на разные константы
        // (i16::MAX против 32768) и расходились в амплитуде.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agree.wav");
        let samples: Vec<i16> = vec![0, i16::MAX, i16::MIN, 1234, -4321];
        write_test_wav(&path, &samples, 16_000);

        let full = read_wav(&path).unwrap();
        let seg = crate::audio::wav_chunker::read_wav_segment(&path, 0.0, 1.0).unwrap();
        assert_eq!(full.sample_rate, seg.sample_rate);
        assert_eq!(full.samples, seg.samples);
    }

    #[test]
    fn read_wav_normalizes_i16_min_without_clipping() {
        // [TD-38] Делитель 32768, не i16::MAX: на 32767 минимум i16 дал бы
        // -1.00003, то есть выход за пределы [-1.0, 1.0].
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("min.wav");
        write_test_wav(&path, &[i16::MIN], 16_000);

        let clip = read_wav(&path).unwrap();
        assert_eq!(clip.samples[0], -1.0);
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

        let clips =
            extract_segments_batch(&path, &[(0.0, 0.25), (0.5, 0.75), (0.75, 1.0)]).unwrap();
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
