//! [M13 fix] Recovery старых/сломанных chunked-записей.
//!
//! До фикса chunk-0 писался в root `mic.wav` (не `chunks/0/`), поэтому
//! `run_chunk(0)` не находил аудио → chunk 0 `failed` → halt-gate валил весь
//! pipeline. Плюс финальный (открытый на stop) chunk никогда не обрабатывался.
//! В итоге на диске остаётся: root `mic.wav` = chunk 0, `chunks/1..N/` =
//! остальные, но `call_chunks` содержит failed chunk 0 + отсутствующий
//! финальный chunk.
//!
//! Этот модуль реконструирует `call_chunks` из **on-disk** chunk WAV'ов:
//! 1. Promote root → `chunks/0/` (оба трека), чтобы раскладка стала
//!    единообразной `chunks/{idx}/`.
//! 2. Скан присутствующих chunk-индексов (mic-трек).
//! 3. Кумулятивные `start_ms`/`end_ms` из реальных длительностей WAV
//!    (корректно учитывает короткий финальный chunk).
//! 4. Для каждого non-done chunk'а: delete stale row + insert fresh `pending`.
//!
//! Возвращает список chunk'ов, которым нужен STT-прогон (`to_run`). Дальше
//! caller (`commands::recording::spawn_recover_chunked`) STT'ит их через
//! `run_chunk`, затем `spawn_initial` — `run_local_inner` сам мержит chunks в
//! root, собирает транскрипт и recap. НЕ `spawn_reprocess`: он требует root
//! WAV, который мы только что промоутнули в `chunks/0/`.

use std::path::Path;

use hound::WavReader;
use sqlx::SqlitePool;

use crate::call_store::CallStore;
use crate::pipeline::audio_merger::{self, TrackKind};
use crate::{db, AppError};

/// Длительность WAV в мс из header: `frames / sample_rate`, где
/// `frames = total_samples / channels`. Wotold пишет 16kHz mono, но делим на
/// channels для robustness.
pub(crate) fn wav_duration_ms(path: &Path) -> Result<u64, AppError> {
    let reader = WavReader::open(path)
        .map_err(|e| AppError::Other(format!("wav_duration open {}: {e}", path.display())))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as u64;
    let sr = spec.sample_rate.max(1) as u64;
    let frames = reader.len() as u64 / channels;
    Ok(frames * 1000 / sr)
}

/// Один chunk, которому нужен STT-прогон при recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecoveryChunk {
    pub idx: u32,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Реконструировать `call_chunks` строки из on-disk chunk WAV'ов. Promote'ит
/// legacy root chunk-0, пересчитывает cumulative offsets, пересоздаёт non-done
/// строки как `pending`. Уже-`done` chunk'и не трогает (их транскрипт валиден).
/// Возвращает chunk'и, которым нужен STT-прогон.
pub(crate) async fn reconstruct_chunk_rows(
    pool: &SqlitePool,
    store: &CallStore,
    call_id: &str,
) -> Result<Vec<RecoveryChunk>, AppError> {
    let chunks_dir = store.chunks_dir(call_id);
    let call_dir = store.call_dir(call_id);

    // 1. Promote legacy root chunk-0 → chunks/0/ (оба трека). No-op если
    //    chunks/0/ уже есть либо merge уже прошёл (.merged sentinel).
    audio_merger::promote_root_to_chunk0(&chunks_dir, &call_dir.join("mic.wav"), TrackKind::Mic);
    audio_merger::promote_root_to_chunk0(
        &chunks_dir,
        &call_dir.join("system.wav"),
        TrackKind::System,
    );

    // 2. Скан присутствующих chunk-индексов (mic = authoritative; всегда есть).
    let present = audio_merger::list_chunk_wavs(&chunks_dir, TrackKind::Mic);
    if present.is_empty() {
        return Err(AppError::Other(format!(
            "recover: no chunk WAVs on disk for call {call_id} (nothing to recover)"
        )));
    }

    let existing = db::chunks::list_chunks_by_call(pool, call_id).await?;
    let mut to_run: Vec<RecoveryChunk> = Vec::new();
    let mut cum_ms: u64 = 0;

    for (idx, mic_path) in &present {
        let idx = *idx;
        let dur = wav_duration_ms(mic_path).unwrap_or(0);
        let row = existing.iter().find(|r| r.chunk_idx == idx);
        let is_done = row.map(|r| r.status == "done").unwrap_or(false);

        if is_done {
            // Keep done row; advance cumulative by stored end_ms (fallback
            // disk dur) чтобы последующие chunk'и offset'ились корректно.
            let end = row
                .and_then(|r| r.end_ms)
                .map(|e| e.max(0) as u64)
                .unwrap_or(cum_ms + dur);
            cum_ms = end.max(cum_ms);
            continue;
        }

        let start_ms = cum_ms;
        let end_ms = cum_ms + dur;
        cum_ms = end_ms;

        // Reset stale (failed/pending/processing) row → delete + fresh pending.
        if row.is_some() {
            db::chunks::delete_chunk(pool, call_id, idx).await?;
        }
        let sys_path = store.chunk_system_path(call_id, idx);
        db::chunks::insert_chunk(pool, call_id, idx, start_ms, mic_path, &sys_path).await?;
        to_run.push(RecoveryChunk {
            idx,
            start_ms,
            end_ms,
        });
    }

    Ok(to_run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::path::Path;

    fn spec_16k_mono() -> WavSpec {
        WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        }
    }

    /// Пишет mono i16 WAV длиной `ms` миллисекунд (16kHz).
    fn write_wav_ms(path: &Path, ms: u64) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let samples = (16_000 * ms / 1000) as usize;
        let mut w = WavWriter::create(path, spec_16k_mono()).unwrap();
        for _ in 0..samples {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
    }

    async fn insert_call(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 'failed', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn wav_duration_ms_matches_samples() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        write_wav_ms(&p, 600_000); // 10 min
        let ms = wav_duration_ms(&p).unwrap();
        assert_eq!(ms, 600_000);

        let p2 = dir.path().join("b.wav");
        write_wav_ms(&p2, 298_000);
        assert_eq!(wav_duration_ms(&p2).unwrap(), 298_000);
    }

    #[tokio::test]
    async fn promote_legacy_root_to_chunk0() {
        // Root mic.wav = chunk 0 (легаси), chunks/1 присутствует, chunks/0 нет.
        let dir = tempfile::tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;

        let call_dir = store.call_dir("c1");
        write_wav_ms(&call_dir.join("mic.wav"), 600_000);
        write_wav_ms(&call_dir.join("system.wav"), 600_000);
        write_wav_ms(&store.chunk_mic_path("c1", 1), 600_000);
        write_wav_ms(&store.chunk_system_path("c1", 1), 600_000);

        let to_run = reconstruct_chunk_rows(&db_t.pool, &store, "c1")
            .await
            .unwrap();
        // chunk 0 promoted from root + chunk 1 present → оба needing STT.
        assert!(store.chunk_mic_path("c1", 0).exists());
        assert_eq!(to_run.iter().map(|r| r.idx).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[tokio::test]
    async fn reconstruct_rows_computes_cumulative_offsets() {
        // chunks/0 (10min) + chunks/1 (10min) + chunks/2 (5min) на диске, все
        // без DB строк. Offsets должны быть кумулятивными.
        let dir = tempfile::tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;

        write_wav_ms(&store.chunk_mic_path("c1", 0), 600_000);
        write_wav_ms(&store.chunk_system_path("c1", 0), 600_000);
        write_wav_ms(&store.chunk_mic_path("c1", 1), 600_000);
        write_wav_ms(&store.chunk_system_path("c1", 1), 600_000);
        write_wav_ms(&store.chunk_mic_path("c1", 2), 298_000);
        write_wav_ms(&store.chunk_system_path("c1", 2), 298_000);

        let to_run = reconstruct_chunk_rows(&db_t.pool, &store, "c1")
            .await
            .unwrap();
        assert_eq!(to_run.len(), 3);
        assert_eq!(
            to_run[0],
            RecoveryChunk {
                idx: 0,
                start_ms: 0,
                end_ms: 600_000
            }
        );
        assert_eq!(
            to_run[1],
            RecoveryChunk {
                idx: 1,
                start_ms: 600_000,
                end_ms: 1_200_000
            }
        );
        assert_eq!(
            to_run[2],
            RecoveryChunk {
                idx: 2,
                start_ms: 1_200_000,
                end_ms: 1_498_000
            }
        );
        // Все строки в DB как pending.
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, "c1")
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.status == "pending"));
    }

    #[tokio::test]
    async fn reconstruct_keeps_done_chunk_and_reruns_failed() {
        // Сценарий call 12b4e564: chunk 0 failed, chunk 1 done, chunk 2 отсутств.
        let dir = tempfile::tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;

        // Диск: chunk 0 (root legacy), chunk 1, chunk 2.
        let call_dir = store.call_dir("c1");
        write_wav_ms(&call_dir.join("mic.wav"), 600_000);
        write_wav_ms(&call_dir.join("system.wav"), 600_000);
        write_wav_ms(&store.chunk_mic_path("c1", 1), 600_000);
        write_wav_ms(&store.chunk_system_path("c1", 1), 600_000);
        write_wav_ms(&store.chunk_mic_path("c1", 2), 298_000);
        write_wav_ms(&store.chunk_system_path("c1", 2), 298_000);

        // DB: chunk 0 failed, chunk 1 done (start 600130, end 1200092).
        db::chunks::insert_chunk(
            &db_t.pool,
            "c1",
            0,
            0,
            &store.chunk_mic_path("c1", 0),
            &store.chunk_system_path("c1", 0),
        )
        .await
        .unwrap();
        db::chunks::mark_chunk_failed(&db_t.pool, "c1", 0, "legacy path miss")
            .await
            .unwrap();
        db::chunks::insert_chunk(
            &db_t.pool,
            "c1",
            1,
            600_130,
            &store.chunk_mic_path("c1", 1),
            &store.chunk_system_path("c1", 1),
        )
        .await
        .unwrap();
        db::chunks::mark_chunk_processing(&db_t.pool, "c1", 1)
            .await
            .unwrap();
        db::chunks::mark_chunk_done(
            &db_t.pool,
            "c1",
            1,
            1_200_092,
            r#"{"segments":[]}"#,
            None,
            None,
        )
        .await
        .unwrap();

        let to_run = reconstruct_chunk_rows(&db_t.pool, &store, "c1")
            .await
            .unwrap();
        // chunk 0 (reset failed) + chunk 2 (new) нуждаются в STT; chunk 1 done — нет.
        assert_eq!(to_run.iter().map(|r| r.idx).collect::<Vec<_>>(), vec![0, 2]);
        // chunk 2 start = после done chunk1.end_ms (1_200_092).
        let c2 = to_run.iter().find(|r| r.idx == 2).unwrap();
        assert_eq!(c2.start_ms, 1_200_092);
        assert_eq!(c2.end_ms, 1_200_092 + 298_000);
        // chunk 1 остался done.
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, "c1")
            .await
            .unwrap();
        let r1 = rows.iter().find(|r| r.chunk_idx == 1).unwrap();
        assert_eq!(r1.status, "done");
    }
}
