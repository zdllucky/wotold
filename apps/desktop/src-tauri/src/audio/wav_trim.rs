//! [T4/R15] Подрезка тихого хвоста WAV.
//!
//! # Зачем
//!
//! Авто-стоп по тишине знает точку, после которой в записи заведомо ничего нет
//! (см. `silence_watch`). Оставить хвост на диске — значит гонять его через
//! whisper (минуты процессорного времени и галлюцинации на пустоте) и показать
//! в плеере трёхчасовой звонок вместо двадцатиминутного.
//!
//! # Как
//!
//! Потоково: hound читает сэмплы итератором, мы пишем нужное количество в
//! `*.wav.tmpN` и делаем `fs::rename` — та же атомарная схема, что у
//! [`crate::pipeline::audio_merger`]. Читать файл целиком нельзя: два часа
//! 16 kHz mono — это ~460 МБ во `Vec<f32>`.
//!
//! Вызывать только из `spawn_blocking` (инженерное правило 5): чтение и запись
//! файла — синхронный I/O, на tokio-воркере он держит поток.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hound::{WavReader, WavWriter};

/// Уникализатор tmp-имени в пределах процесса — как в `audio_merger`: две
/// подрезки одного файла не должны делить временный путь.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, thiserror::Error)]
pub enum TrimError {
    #[error("wav read failed at {0}: {1}")]
    Read(PathBuf, String),
    #[error("wav write failed at {0}: {1}")]
    Write(PathBuf, String),
    /// Формат не 16-бит PCM. Весь конвейер пишет 16 kHz mono i16
    /// (`WAVWriter.swift`); чужой формат — повод остановиться, а не угадывать.
    #[error("unsupported wav format at {path}: {bits} bit {format:?}")]
    UnsupportedFormat {
        path: PathBuf,
        bits: u16,
        format: hound::SampleFormat,
    },
    /// `keep_ms` даёт ноль фреймов. Пустая дорожка сломала бы STT молча —
    /// лучше явная ошибка: значит точка реза посчитана неверно.
    #[error("refusing to trim {0} to zero frames (keep_ms={1})")]
    WouldBeEmpty(PathBuf, u64),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Что получилось. `before_ms`/`after_ms` — длительность дорожки до и после.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimReport {
    pub before_ms: u64,
    pub after_ms: u64,
}

/// Обрезать дорожку до `keep_ms` от начала.
///
/// `Ok(None)` — резать нечего: файл уже короче (или ровно), либо его нет.
/// Отсутствие файла не ошибка: system-дорожка легально пустует, когда в
/// звонке не было системного звука, а вызывающему нечего с этим делать.
///
/// Идемпотентно: повторный вызов с тем же `keep_ms` вернёт `Ok(None)`. Это
/// обязательное свойство — пайплайн переобрабатывает звонки и пере-STT'ит
/// дорожки при смене языка.
pub fn trim_wav_tail(path: &Path, keep_ms: u64) -> Result<Option<TrimReport>, TrimError> {
    if !path.exists() {
        return Ok(None);
    }

    let reader = WavReader::open(path).map_err(|e| TrimError::Read(path.into(), e.to_string()))?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
        return Err(TrimError::UnsupportedFormat {
            path: path.into(),
            bits: spec.bits_per_sample,
            format: spec.sample_format,
        });
    }
    let channels = spec.channels.max(1) as u64;
    let sample_rate = spec.sample_rate.max(1) as u64;
    let total_frames = reader.len() as u64 / channels;
    let before_ms = total_frames * 1_000 / sample_rate;

    let keep_frames = keep_ms.saturating_mul(sample_rate) / 1_000;
    if keep_frames >= total_frames {
        return Ok(None);
    }
    if keep_frames == 0 {
        return Err(TrimError::WouldBeEmpty(path.into(), keep_ms));
    }

    let tmp_path = path.with_extension(format!(
        "wav.trimtmp{}-{}",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let outcome = write_prefix(reader, &tmp_path, spec, keep_frames * channels);
    if let Err(e) = outcome {
        // Частично записанный tmp не должен переживать сбой: следующая попытка
        // наткнулась бы на битый WAV с валидным именем.
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    fs::rename(&tmp_path, path)?;

    let after_ms = keep_frames * 1_000 / sample_rate;
    Ok(Some(TrimReport {
        before_ms,
        after_ms,
    }))
}

/// Скопировать первые `samples` сэмплов в новый WAV. Отдельной функцией,
/// чтобы `?` не разбегался с очисткой tmp у вызывающего.
fn write_prefix(
    reader: WavReader<std::io::BufReader<fs::File>>,
    tmp_path: &Path,
    spec: hound::WavSpec,
    samples: u64,
) -> Result<(), TrimError> {
    let mut writer = WavWriter::create(tmp_path, spec)
        .map_err(|e| TrimError::Write(tmp_path.into(), e.to_string()))?;
    let mut reader = reader;
    for sample in reader.samples::<i16>().take(samples as usize) {
        let sample = sample.map_err(|e| TrimError::Read(tmp_path.into(), e.to_string()))?;
        writer
            .write_sample(sample)
            .map_err(|e| TrimError::Write(tmp_path.into(), e.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|e| TrimError::Write(tmp_path.into(), e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(channels: u16) -> hound::WavSpec {
        hound::WavSpec {
            channels,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        }
    }

    /// Записать WAV длиной `ms`: первая половина «звук», вторая — нули, чтобы
    /// тесты могли отличить сохранённый префикс от отрезанного хвоста.
    fn write_wav(path: &Path, ms: u64, channels: u16) {
        let frames = ms * 16_000 / 1_000;
        let mut w = WavWriter::create(path, spec(channels)).expect("create");
        for frame in 0..frames {
            let value = if frame < frames / 2 { 1_000i16 } else { 0 };
            for _ in 0..channels {
                w.write_sample(value).expect("write");
            }
        }
        w.finalize().expect("finalize");
    }

    fn duration_ms(path: &Path) -> u64 {
        let r = WavReader::open(path).expect("open");
        let ch = r.spec().channels as u64;
        r.len() as u64 / ch * 1_000 / r.spec().sample_rate as u64
    }

    #[test]
    fn trims_to_requested_length() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        write_wav(&path, 10_000, 1);

        let report = trim_wav_tail(&path, 4_000)
            .expect("trim")
            .expect("что-то отрезали");
        assert_eq!(
            report,
            TrimReport {
                before_ms: 10_000,
                after_ms: 4_000
            }
        );
        assert_eq!(duration_ms(&path), 4_000);
    }

    #[test]
    fn keeps_the_head_samples_not_the_tail() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        write_wav(&path, 10_000, 1);
        trim_wav_tail(&path, 4_000).expect("trim").expect("trimmed");

        let mut r = WavReader::open(&path).expect("open");
        let samples: Vec<i16> = r.samples::<i16>().map(|s| s.expect("sample")).collect();
        assert_eq!(samples.len(), 4_000 * 16);
        assert!(
            samples.iter().all(|&s| s == 1_000),
            "оставить обязаны начало (звук), а не хвост (нули)"
        );
    }

    #[test]
    fn noop_when_already_shorter() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        write_wav(&path, 3_000, 1);
        assert_eq!(trim_wav_tail(&path, 5_000).expect("trim"), None);
        assert_eq!(duration_ms(&path), 3_000, "файл не тронут");
    }

    #[test]
    fn second_call_is_a_noop() {
        // Идемпотентность: пайплайн переобрабатывает звонки, и рез не должен
        // отъедать по куску на каждом прогоне.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        write_wav(&path, 10_000, 1);
        assert!(trim_wav_tail(&path, 4_000).expect("first").is_some());
        assert_eq!(trim_wav_tail(&path, 4_000).expect("second"), None);
        assert_eq!(duration_ms(&path), 4_000);
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Легальный случай: в звонке не было системного звука.
        let path = dir.path().join("system.wav");
        assert_eq!(trim_wav_tail(&path, 4_000).expect("trim"), None);
    }

    #[test]
    fn refuses_to_produce_empty_track() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        write_wav(&path, 10_000, 1);
        let err = trim_wav_tail(&path, 0).expect_err("нулевой рез обязан быть ошибкой");
        assert!(matches!(err, TrimError::WouldBeEmpty(..)), "{err:?}");
        assert_eq!(duration_ms(&path), 10_000, "файл не тронут");
    }

    #[test]
    fn broken_header_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        fs::write(&path, b"this is definitely not a RIFF header").expect("write");
        let err = trim_wav_tail(&path, 4_000).expect_err("битый header");
        assert!(matches!(err, TrimError::Read(..)), "{err:?}");
    }

    #[test]
    fn rejects_non_16bit_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        let mut w = WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .expect("create");
        for _ in 0..16_000 {
            w.write_sample(0.5f32).expect("write");
        }
        w.finalize().expect("finalize");

        let err = trim_wav_tail(&path, 500).expect_err("чужой формат");
        assert!(
            matches!(err, TrimError::UnsupportedFormat { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn handles_multichannel_frames() {
        // Дорожки конвейера моно, но арифметика фреймов не должна зависеть от
        // этого допущения: сэмплы считаются по каналам, кадры — нет.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        write_wav(&path, 10_000, 2);
        let report = trim_wav_tail(&path, 4_000).expect("trim").expect("trimmed");
        assert_eq!(report.after_ms, 4_000);
        assert_eq!(duration_ms(&path), 4_000);
    }

    #[test]
    fn leaves_no_tmp_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mic.wav");
        write_wav(&path, 10_000, 1);
        trim_wav_tail(&path, 4_000).expect("trim").expect("trimmed");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("trimtmp"))
            .collect();
        assert!(leftovers.is_empty(), "остался tmp: {leftovers:?}");
    }
}
