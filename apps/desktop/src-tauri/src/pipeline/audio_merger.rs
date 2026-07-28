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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hound::{WavReader, WavSpec, WavWriter};
use thiserror::Error;

/// [B28.1] Уникальный суффикс tmp-файла. Раньше все merge писали в один
/// `mic.wav.tmp` — параллельные вызовы (плеер запрашивает mic+system,
/// pipeline step 1, ретраи UI) делили tmp: победитель rename'ил, остальные
/// падали ENOENT (живой кейс: 7 «wav write failed» на звонке 3df01365).
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Stale tmp старше этого возраста — мусор от краша посреди merge (штатный
/// merge живёт секунды и убирает за собой). Свежие не трогаем: их может
/// писать параллельный merge.
const STALE_TMP_AGE: Duration = Duration::from_secs(3600);

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
    /// Каталог звонка исчез, пока шла склейка (обычно — пользователь удалил
    /// звонок). Создавать его заново нельзя: получился бы каталог с аудио без
    /// строки в базе.
    #[error("call dir gone: {0}")]
    CallDirGone(PathBuf),
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
pub(crate) fn list_chunk_wavs(chunks_dir: &Path, kind: TrackKind) -> Vec<(u32, PathBuf)> {
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

/// [P6+P7] Promote root WAV → `chunks/0/` если `chunks/0/{filename}` отсутствует,
/// но root WAV существует и есть **любой** `chunks/{N≥1}/{filename}`.
///
/// **CRITICAL DATA-LOSS GUARD.** Sidecar пишет first chunk (0-10 мин до first
/// rotate) в root `mic.wav`, не в `chunks/0/mic.wav`. Без promotion merge
/// перезаписал бы root выходом из chunks/{1..N}/, уничтожив аудио первых 10
/// минут (regression на call fd4b3380, 2026-05-25).
///
/// [Sentinel] `chunks/.merged` marker предотвращает double-promote на reprocess:
/// после успешного merge root WAV содержит merged result (не original chunk 0).
///
/// [M13 fix] Вынесено из `merge_track` в отдельную функцию — `chunk_recovery`
/// зовёт её ДО реконструкции `call_chunks` строк, чтобы chunk 0 оказался в
/// `chunks/0/` и `run_chunk(0)` его нашёл. Возвращает `true` если promote сделан.
pub(crate) fn promote_root_to_chunk0(
    chunks_dir: &Path,
    output_path: &Path,
    kind: TrackKind,
) -> bool {
    let merged_sentinel = chunks_dir.join(".merged");
    let chunks_idx0 = chunks_dir.join("0").join(kind.filename());
    let has_any_other_chunk = list_chunk_wavs(chunks_dir, kind)
        .iter()
        .any(|(idx, _)| *idx >= 1);
    if !merged_sentinel.exists()
        && !chunks_idx0.exists()
        && output_path.exists()
        && has_any_other_chunk
    {
        if let Err(e) = fs::create_dir_all(chunks_dir.join("0")) {
            log::warn!("audio_merger: failed to create chunks/0/: {e}");
            false
        } else if let Err(e) = fs::rename(output_path, &chunks_idx0) {
            log::warn!(
                "audio_merger: failed to promote root {} → chunks/0/: {e}",
                output_path.display()
            );
            false
        } else {
            log::info!(
                "audio_merger: promoted root WAV → {} (first-merge fix)",
                chunks_idx0.display()
            );
            true
        }
    } else {
        if merged_sentinel.exists() {
            log::debug!(
                "audio_merger: skip pre-promote (.merged sentinel exists at {})",
                merged_sentinel.display()
            );
        }
        false
    }
}

/// [B28.1] Удалить осиротевшие `<track>.wav.tmp*`-файлы старше часа —
/// остатки merge, прерванного крашем. Свежие пропускаем: их может писать
/// параллельный merge прямо сейчас.
fn cleanup_stale_tmps(output_path: &Path, kind: TrackKind) {
    let Some(parent) = output_path.parent() else {
        return;
    };
    let prefix = format!("{}.tmp", kind.filename());
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(&prefix) {
            continue;
        }
        let is_stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE_TMP_AGE);
        if is_stale {
            if let Err(e) = fs::remove_file(entry.path()) {
                log::warn!("audio_merger: stale tmp cleanup failed ({name}): {e}");
            } else {
                log::info!("audio_merger: removed stale tmp {name}");
            }
        }
    }
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
    // [P6+P7] Promote root WAV → chunks/0/ на first merge (см. doc-comment
    // `promote_root_to_chunk0`). Sentinel `.merged` (пишется в конце) защищает
    // от double-promote на reprocess.
    promote_root_to_chunk0(chunks_dir, output_path, kind);

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

    // Подготовка writer'а. Каталог звонка обязан уже существовать — мы его не
    // создаём.
    //
    // Почему не `create_dir_all`: склейка идёт в `spawn_blocking`, а его нельзя
    // отменить — `JoinHandle::abort` роняет только внешнюю задачу, замыкание
    // доживает на своём потоке. Если в этот момент звонок удалили, создание
    // каталога воскрешало бы удалённое: каталог с аудио и без строки в базе
    // (тот самый мусор, который потом подметает `orphan_reconcile`, TD-50).
    // Каталог существует во всех легальных путях — запись, переобработка,
    // восстановление и запрос аудио плеером идут по живому звонку.
    if let Some(parent) = output_path.parent() {
        if !parent.is_dir() {
            return Err(MergeError::CallDirGone(parent.to_path_buf()));
        }
    }
    // Temp-file pattern: пишем во временный файл, потом rename → атомарный
    // swap. Защищает от partial WAV при interrupted merge (next reprocess
    // увидит either old-truncated или new-full, не corrupt-half).
    // [B28.1] Имя УНИКАЛЬНО per-вызов (pid+seq): параллельные merge одного
    // трека больше не делят tmp — каждый rename'ит свой, last-writer-wins
    // (результат идентичен — идемпотентно). Заодно подметаем stale tmp от
    // прошлых крашей.
    cleanup_stale_tmps(output_path, kind);
    let tmp_path = output_path.with_extension(format!(
        "wav.tmp{}-{}",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
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

    // [P6 sentinel] Write `.merged` marker — pre-promote logic skip'ает
    // root WAV move на следующем reprocess. Без marker'а двойной merge
    // удвоил бы audio (chunks 0(=prev_merge)+1+2+3 = 2× chunks 1+2+3).
    if let Err(e) = fs::write(chunks_dir.join(".merged"), b"v1") {
        log::warn!(
            "audio_merger: failed to write .merged sentinel в {}: {e}",
            chunks_dir.display()
        );
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
    // [M13 fix] Promote root→chunks/0 для ОБОИХ треков ДО первого merge_track.
    // merge_track пишет shared `chunks/.merged` sentinel в конце успешного
    // merge — если mic смержится первым, sentinel заблокирует promote system'а
    // и его chunk-0 (первые ~10 мин собеседника) потеряется. Promote здесь
    // (до любого merge, пока sentinel'а нет) фиксит это; внутренний promote в
    // merge_track станет no-op (chunks/0 уже на месте).
    promote_root_to_chunk0(chunks_dir, &mic_out, TrackKind::Mic);
    promote_root_to_chunk0(chunks_dir, &sys_out, TrackKind::System);
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

    /// [B28.1] Ни одного tmp-огрызка в директории output'а.
    fn assert_no_tmp_leftovers(dir: &Path) {
        let leftovers: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "tmp leftovers: {leftovers:?}");
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
        assert_no_tmp_leftovers(dir.path());
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
        assert_no_tmp_leftovers(dir.path());
    }

    // [B28.1] Регресс гонки звонка 3df01365: N параллельных merge одного
    // трека — все успешны (уникальные tmp), output корректен, tmp не остаются.
    #[test]
    fn concurrent_merges_of_same_track_all_succeed() {
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec = spec_16k_mono_i16();
        write_stub_wav(&chunks_dir.join("0/mic.wav"), spec, &[1, 2, 3]);
        write_stub_wav(&chunks_dir.join("1/mic.wav"), spec, &[4, 5]);
        let out = dir.path().join("mic.wav");

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let cd = chunks_dir.clone();
                let o = out.clone();
                std::thread::spawn(move || {
                    merge_track(&cd, &o, TrackKind::Mic).map(|r| r.total_samples)
                })
            })
            .collect();
        for h in handles {
            // Раньше 7 из 8 падали MergeError::Write (ENOENT на shared tmp).
            assert_eq!(h.join().unwrap().unwrap(), 5);
        }
        let samples: Vec<i16> = WavReader::open(&out)
            .unwrap()
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(samples, vec![1, 2, 3, 4, 5]);
        assert_no_tmp_leftovers(dir.path());
    }

    // [B28.1] Stale tmp (моложе часа НЕ трогаем, старше — подметаем).
    #[test]
    fn cleanup_removes_only_old_tmps() {
        let dir = tempdir().unwrap();
        let fresh = dir.path().join("mic.wav.tmp999-0");
        fs::write(&fresh, b"fresh").unwrap();
        cleanup_stale_tmps(&dir.path().join("mic.wav"), TrackKind::Mic);
        assert!(fresh.exists(), "свежий tmp параллельного merge не трогаем");
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
    fn pre_promote_root_to_chunk_zero_when_chunk_zero_missing() {
        // [P6] Root WAV есть (sidecar first chunk до ротации), chunks/0/
        // нет, chunks/1/ есть → merger должен promote root → chunks/0/
        // ДО merge, иначе chunk 0 audio теряется.
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec = spec_16k_mono_i16();
        write_stub_wav(&dir.path().join("mic.wav"), spec, &[100, 101, 102]);
        write_stub_wav(&chunks_dir.join("1/mic.wav"), spec, &[200, 201]);
        let out = dir.path().join("mic.wav");
        let report = merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap();
        assert_eq!(report.chunks_merged, 2);
        // chunks/0/mic.wav теперь содержит original root audio.
        let preserved: Vec<i16> = WavReader::open(chunks_dir.join("0/mic.wav"))
            .unwrap()
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(preserved, vec![100, 101, 102]);
        // Merged root содержит chunks 0+1.
        let merged: Vec<i16> = WavReader::open(&out)
            .unwrap()
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(merged, vec![100, 101, 102, 200, 201]);
    }

    #[test]
    fn pre_promote_works_when_only_chunk_two_exists() {
        // [P7] Прежняя версия требовала именно chunks/1/. Если первая
        // rotation ушла в failed и dir удалили — promote не срабатывал.
        // Теперь триггер — любой chunks/{N≥1}/.
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        let spec = spec_16k_mono_i16();
        write_stub_wav(&dir.path().join("mic.wav"), spec, &[10, 11]);
        write_stub_wav(&chunks_dir.join("2/mic.wav"), spec, &[20, 21]);
        let out = dir.path().join("mic.wav");
        let report = merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap();
        assert_eq!(report.chunks_merged, 2);
        let preserved: Vec<i16> = WavReader::open(chunks_dir.join("0/mic.wav"))
            .unwrap()
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(preserved, vec![10, 11]);
    }

    #[test]
    fn pre_promote_skipped_when_sentinel_present() {
        // [P6] После успешного merge .merged sentinel выставлен. На
        // reprocess root WAV содержит merged output (не chunk 0).
        // Pre-promote НЕ должен запуститься — иначе двойной audio.
        let dir = tempdir().unwrap();
        let chunks_dir = dir.path().join("chunks");
        fs::create_dir_all(&chunks_dir).unwrap();
        fs::write(chunks_dir.join(".merged"), b"v1").unwrap();
        let spec = spec_16k_mono_i16();
        // Root содержит merged-from-prev-run [1,2,3,4].
        write_stub_wav(&dir.path().join("mic.wav"), spec, &[1, 2, 3, 4]);
        write_stub_wav(&chunks_dir.join("0/mic.wav"), spec, &[1, 2]);
        write_stub_wav(&chunks_dir.join("1/mic.wav"), spec, &[3, 4]);
        let out = dir.path().join("mic.wav");
        merge_track(&chunks_dir, &out, TrackKind::Mic).unwrap();
        // chunks/0/ остался [1,2] не [1,2,3,4].
        let preserved: Vec<i16> = WavReader::open(chunks_dir.join("0/mic.wav"))
            .unwrap()
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(preserved, vec![1, 2]);
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

    #[test]
    fn merge_does_not_recreate_a_deleted_call_dir() {
        // Склейка идёт в spawn_blocking, который нельзя отменить: если звонок
        // удалили посреди неё, `create_dir_all` воскресил бы каталог с аудио и
        // без строки в базе. Каталог мы не создаём — падаем с CallDirGone.
        let tmp = tempdir().unwrap();
        let chunks_dir = tmp.path().join("chunks");
        write_stub_wav(
            &chunks_dir.join("0").join("mic.wav"),
            spec_16k_mono_i16(),
            &[1, 2, 3, 4],
        );
        // Каталог звонка «удалён»: пишем в путь, родителя которого нет.
        let output = tmp.path().join("gone").join("mic.wav");

        let err = merge_track(&chunks_dir, &output, TrackKind::Mic).unwrap_err();
        assert!(
            matches!(err, MergeError::CallDirGone(_)),
            "ожидали CallDirGone, получили {err:?}"
        );
        assert!(
            !output.parent().unwrap().exists(),
            "каталог не должен появиться"
        );
    }
}
