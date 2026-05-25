//! [Tech-debt P0.1] Audio merger — конкатенирует per-chunk `mic.wav` /
//! `system.wav` файлы в единый root-level WAV для каждой дорожки.
//!
//! ## Зачем
//!
//! После M13.1.5d (chunked recording) live sidecar пишет аудио в
//! `calls/{call_id}/chunks/{idx}/mic.wav` каждые ~10 мин. Корневые
//! `calls/{call_id}/mic.wav` + `system.wav` остаются от первого chunk'а или
//! отсутствуют — `AudioScrubber.tsx` рендерит только короткий фрагмент
//! вместо полной записи.
//!
//! Этот модуль вызывается post-pipeline (после успешной обработки всех
//! chunks) и склеивает существующие chunk WAV-файлы в root. Failed chunks
//! без файла на диске — пропускаются (audio merge независим от STT
//! status, файл может существовать даже когда STT упал).
//!
//! ## Ограничения
//!
//! - Все chunks должны иметь одинаковый WAV spec (sample_rate, channels,
//!   bits_per_sample). Иначе merge fail с подробным `MergeError`.
//! - Hound load/save буферизует всё в RAM (`Vec<i16>`); для 1+ часа аудио
//!   на 16kHz mono ≈ 115 MB. Acceptable для desktop; для multi-hour record
//!   позже придётся stream'ить.
//! - Merge idempotent — пересоздание root WAV каждый раз ок.

use std::fs;
use std::path::{Path, PathBuf};

use hound::{WavReader, WavSpec, WavWriter};
use thiserror::Error;

/// Канал для merge — mic-дорожка или system-дорожка. Имя файла внутри
/// chunk-директории определяется этим enum'ом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Mic,
    System,
}

impl TrackKind {
    fn filename(self) -> &'static str {
        match self {
            TrackKind::Mic => "mic.wav",
            TrackKind::System => "system.wav",
        }
    }
}

#[derive(Debug, Error)]
pub enum MergeError {
    #[error("no chunk wav files found for {0:?} in {1}")]
    NoChunks(TrackKind, PathBuf),
    #[error("wav read failed at {0}: {1}")]
    Read(PathBuf, String),
    #[error("wav write failed at {0}: {1}")]
    Write(PathBuf, String),
    #[error("spec mismatch: expected {expected:?}, got {got:?} at {path}")]
    SpecMismatch {
        expected: WavSpec,
        got: WavSpec,
        path: PathBuf,
    },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Результат merge — где лежит итоговый WAV + сколько chunks реально склеено
/// + сколько пропущено. Поля используются для логирования + future telemetry.
#[derive(Debug)]
pub struct MergeReport {
    pub output_path: PathBuf,
    pub chunks_merged: usize,
    pub chunks_skipped: usize,
    pub total_samples: u64,
    /// WAV spec из first chunk'а (используется как ground truth для всех остальных).
    #[allow(dead_code)] // read in tests + future telemetry
    pub spec: WavSpec,
}

/// Найти все chunk WAV-файлы для трэка, отсортированные по chunk_idx
/// (numeric, не lexicographic). Скан filesystem'а — не зависит от
/// `db::chunks` (audio merge должен работать даже если DB row отсутствует
/// для chunk'а).
///
/// Layout: `chunks_dir/{idx}/{filename}`. `chunks_dir` typically
/// `calls/{call_id}/chunks`.
fn list_chunk_wavs(chunks_dir: &Path, kind: TrackKind) -> Vec<(u32, PathBuf)> {
    let entries = match fs::read_dir(chunks_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut found: Vec<(u32, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let idx: u32 = name.parse().ok()?;
            let wav = entry.path().join(kind.filename());
            if wav.exists() {
                Some((idx, wav))
            } else {
                None
            }
        })
        .collect();
    found.sort_by_key(|(idx, _)| *idx);
    found
}

/// Склеить все chunk WAV-файлы данного трека в один root WAV.
///
/// - `chunks_dir` — `calls/{call_id}/chunks`.
/// - `output_path` — куда писать merged WAV (обычно `calls/{call_id}/mic.wav`).
/// - На пустую коллекцию (ни одного chunk'а) → `MergeError::NoChunks`.
/// - На spec mismatch → `MergeError::SpecMismatch` (без partial write —
///   удаляем недописанный файл).
pub fn merge_track(
    chunks_dir: &Path,
    output_path: &Path,
    kind: TrackKind,
) -> Result<MergeReport, MergeError> {
    // [P6] Promote root WAV → chunks/0/ on first merge. Sidecar пишет
    // first chunk (0-10 мин до first rotate) в root `mic.wav`, не в
    // `chunks/0/mic.wav` — иначе AudioScrubber играл бы пустой root до
    // окончания pipeline. На rotate sidecar переключается на chunks/N/.
    // audio_merger же сканирует chunks/{idx}/ → missing chunk 0 → output
    // = chunks 1+2+3 only (player «21:55» вместо real 31:56).
    //
    // Fix: если chunks/0/{filename} отсутствует но root WAV существует
    // и chunks/1/ есть — move root → chunks/0/. Idempotent: на reprocess
    // chunks/0/ уже на месте, skip move.
    let chunks_idx0 = chunks_dir.join("0").join(kind.filename());
    let chunks_idx1 = chunks_dir.join("1").join(kind.filename());
    if !chunks_idx0.exists() && output_path.exists() && chunks_idx1.exists() {
        if let Err(e) = fs::create_dir_all(chunks_dir.join("0")) {
            log::warn!("audio_merger: failed to create chunks/0/: {e}");
        } else if let Err(e) = fs::rename(output_path, &chunks_idx0) {
            log::warn!(
                "audio_merger: failed to promote root {} → chunks/0/: {e}",
                output_path.display()
            );
        } else {
            log::info!(
                "audio_merger: promoted root WAV → {} (first-merge fix)",
                chunks_idx0.display()
            );
        }
    }

    let chunks = list_chunk_wavs(chunks_dir, kind);
    if chunks.is_empty() {
        return Err(MergeError::NoChunks(kind, chunks_dir.to_path_buf()));
    }

    // Header первого chunk'а определяет spec для всего output.
    let first_path = &chunks[0].1;
    let first_reader = WavReader::open(first_path)
        .map_err(|e| MergeError::Read(first_path.clone(), e.to_string()))?;
    let spec = first_reader.spec();
    drop(first_reader);

    // Подготовка writer'а. Создаём parent dir если ещё нет (на reprocess'ах
    // редко, но safer).
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Temp-file pattern: пишем в `output_path.tmp`, потом rename → атомарный
    // swap. Защищает от partial WAV при interrupted merge (next reprocess
    // увидит either old-truncated или new-full, не corrupt-half).
    let tmp_path = output_path.with_extension("wav.tmp");
    let mut writer = WavWriter::create(&tmp_path, spec)
        .map_err(|e| MergeError::Write(tmp_path.clone(), e.to_string()))?;

    let mut merged = 0usize;
    let mut skipped = 0usize;
    let mut total_samples = 0u64;

    for (idx, path) in &chunks {
        let mut reader = match WavReader::open(path) {
            Ok(r) => r,
            Err(e) => {
                log::warn!(
                    "audio_merger: skip chunk {idx} ({}): read failed: {e}",
                    path.display()
                );
                skipped += 1;
                continue;
            }
        };
        let chunk_spec = reader.spec();
        if chunk_spec != spec {
            // Не fail с partial output — очищаем tmp и возвращаем ошибку.
            drop(writer);
            let _ = fs::remove_file(&tmp_path);
            return Err(MergeError::SpecMismatch {
                expected: spec,
                got: chunk_spec,
                path: path.clone(),
            });
        }
        let mut chunk_samples = 0u64;
        let copy_result = match spec.sample_format {
            hound::SampleFormat::Int => {
                let samples = reader.samples::<i16>();
                let mut err: Option<hound::Error> = None;
                for s in samples {
                    match s {
                        Ok(sample) => {
                            if let Err(e) = writer.write_sample(sample) {
                                err = Some(e);
                                break;
                            }
                            chunk_samples += 1;
                        }
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                err
            }
            hound::SampleFormat::Float => {
                let samples = reader.samples::<f32>();
                let mut err: Option<hound::Error> = None;
                for s in samples {
                    match s {
                        Ok(sample) => {
                            if let Err(e) = writer.write_sample(sample) {
                                err = Some(e);
                                break;
                            }
                            chunk_samples += 1;
                        }
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                err
            }
        };
        if let Some(e) = copy_result {
            // Per-chunk read/write failure — пропускаем, не fail всю merge
            // (1 corrupt chunk не должен убивать остальные).
            log::warn!(
                "audio_merger: skip chunk {idx} ({}): {chunk_samples} samples written before error: {e}",
                path.display()
            );
            skipped += 1;
            continue;
        }
        merged += 1;
        total_samples += chunk_samples;
    }

    writer
        .finalize()
        .map_err(|e| MergeError::Write(tmp_path.clone(), e.to_string()))?;

    if merged == 0 {
        let _ = fs::remove_file(&tmp_path);
        return Err(MergeError::NoChunks(kind, chunks_dir.to_path_buf()));
    }

    // Атомарный swap tmp → output_path. На Unix rename перезаписывает.
    fs::rename(&tmp_path, output_path)
        .map_err(|e| MergeError::Write(output_path.to_path_buf(), e.to_string()))?;

    Ok(MergeReport {
        output_path: output_path.to_path_buf(),
        chunks_merged: merged,
        chunks_skipped: skipped,
        total_samples,
        spec,
    })
}

/// Convenience: склеить оба трека (mic + system) одним вызовом. Failed
/// per-track → log::warn + продолжаем; root caller получает оба report'а
/// (Some на успехе, None на failure).
///
/// Используется в `pipeline::run_local_inner` после успешного `chunk_assembly`.
pub fn merge_both_tracks(
    chunks_dir: &Path,
    call_dir: &Path,
) -> (Option<MergeReport>, Option<MergeReport>) {
    let mic_out = call_dir.join("mic.wav");
    let sys_out = call_dir.join("system.wav");
    let mic_report = match merge_track(chunks_dir, &mic_out, TrackKind::Mic) {
        Ok(r) => {
            log::info!(
                "audio_merger[mic]: {} chunks merged, {} skipped, {} samples → {}",
                r.chunks_merged,
                r.chunks_skipped,
                r.total_samples,
                r.output_path.display()
            );
            Some(r)
        }
        Err(e) => {
            log::warn!("audio_merger[mic]: {e}");
            None
        }
    };
    let sys_report = match merge_track(chunks_dir, &sys_out, TrackKind::System) {
        Ok(r) => {
            log::info!(
                "audio_merger[system]: {} chunks merged, {} skipped, {} samples → {}",
                r.chunks_merged,
                r.chunks_skipped,
                r.total_samples,
                r.output_path.display()
            );
            Some(r)
        }
        Err(e) => {
            log::warn!("audio_merger[system]: {e}");
            None
        }
    };
    (mic_report, sys_report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    fn write_stub_wav(path: &Path, spec: WavSpec, samples: &[i16]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut w = WavWriter::create(path, spec).unwrap();
        for s in samples {
            w.write_sample(*s).unwrap();
        }
        w.finalize().unwrap();
    }

    fn spec_16k_mono_i16() -> WavSpec {
        WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        }
    }

    #[test]
    fn merge_three_chunks_concatenates_samples() {
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec = spec_16k_mono_i16();
        write_stub_wav(&chunks_dir.join("0/mic.wav"), spec, &[1, 2, 3]);
        write_stub_wav(&chunks_dir.join("1/mic.wav"), spec, &[4, 5]);
        write_stub_wav(&chunks_dir.join("2/mic.wav"), spec, &[6, 7, 8, 9]);
        let out = dir.path().join("mic.wav");
        let report = merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap();
        assert_eq!(report.chunks_merged, 3);
        assert_eq!(report.chunks_skipped, 0);
        assert_eq!(report.total_samples, 9);

        // Verify file content matches concatenation order.
        let reader = WavReader::open(&out).unwrap();
        let samples: Vec<i16> = reader
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(samples, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn merge_sorts_chunks_numerically_not_lexicographically() {
        // 10 lexicographically < 2; numeric sort должен дать [2, 10].
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec = spec_16k_mono_i16();
        write_stub_wav(&chunks_dir.join("2/mic.wav"), spec, &[2]);
        write_stub_wav(&chunks_dir.join("10/mic.wav"), spec, &[10]);
        let out = dir.path().join("mic.wav");
        merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap();
        let reader = WavReader::open(&out).unwrap();
        let samples: Vec<i16> = reader
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(samples, vec![2, 10]);
    }

    #[test]
    fn merge_skips_missing_files_no_error() {
        // Chunk 1 dir существует, но без mic.wav. Chunk 0 и 2 имеют файлы.
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec = spec_16k_mono_i16();
        write_stub_wav(&chunks_dir.join("0/mic.wav"), spec, &[1]);
        fs::create_dir_all(chunks_dir.join("1")).unwrap();
        write_stub_wav(&chunks_dir.join("2/mic.wav"), spec, &[3]);
        let out = dir.path().join("mic.wav");
        let report = merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap();
        // Chunk 1 dir не имеет mic.wav → не попадает в list_chunk_wavs вообще.
        assert_eq!(report.chunks_merged, 2);
        assert_eq!(report.chunks_skipped, 0);
    }

    #[test]
    fn merge_empty_dir_returns_no_chunks_err() {
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        fs::create_dir_all(&chunks_dir).unwrap();
        let out = dir.path().join("mic.wav");
        let err = merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap_err();
        assert!(matches!(err, MergeError::NoChunks(TrackKind::Mic, _)));
        // Tmp file должен быть очищен.
        assert!(!out.with_extension("wav.tmp").exists());
    }

    #[test]
    fn merge_spec_mismatch_returns_err_and_cleans_tmp() {
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec_a = spec_16k_mono_i16();
        let spec_b = WavSpec {
            sample_rate: 44_100,
            ..spec_a
        };
        write_stub_wav(&chunks_dir.join("0/mic.wav"), spec_a, &[1, 2]);
        write_stub_wav(&chunks_dir.join("1/mic.wav"), spec_b, &[3, 4]);
        let out = dir.path().join("mic.wav");
        let err = merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap_err();
        assert!(matches!(err, MergeError::SpecMismatch { .. }));
        assert!(!out.with_extension("wav.tmp").exists());
    }

    #[test]
    fn merge_both_tracks_independent_outcomes() {
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec = spec_16k_mono_i16();
        // Только mic — system отсутствует во всех chunks.
        write_stub_wav(&chunks_dir.join("0/mic.wav"), spec, &[1]);
        write_stub_wav(&chunks_dir.join("1/mic.wav"), spec, &[2]);
        let (mic, sys) = merge_both_tracks(&chunks_dir, dir.path());
        assert!(mic.is_some());
        assert!(sys.is_none()); // NoChunks для system — это OK.
    }

    #[test]
    fn merge_overwrites_existing_root_atomically() {
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec = spec_16k_mono_i16();
        // Старый root mic.wav — должен быть заменён merged version.
        write_stub_wav(&dir.path().join("mic.wav"), spec, &[99]);
        write_stub_wav(&chunks_dir.join("0/mic.wav"), spec, &[1, 2]);
        write_stub_wav(&chunks_dir.join("1/mic.wav"), spec, &[3, 4]);
        let out = dir.path().join("mic.wav");
        merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap();
        let reader = WavReader::open(&out).unwrap();
        let samples: Vec<i16> = reader
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        // Старое содержимое [99] заменено на [1, 2, 3, 4].
        assert_eq!(samples, vec![1, 2, 3, 4]);
    }
}
