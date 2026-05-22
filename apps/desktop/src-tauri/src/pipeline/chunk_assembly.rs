//! [M13.1.5d] Assembly per-chunk transcripts из DB обратно в две
//! `DiarizedTranscript` (mic + system) с timestamp-offset.
//!
//! Контекст: за время записи [`crate::pipeline::chunk_runner`] складывает
//! per-chunk транскрипты в таблицу `call_chunks` (одна строка на 10-мин
//! chunk, два JSON-blob'а — mic + system). При `stop_recording` если
//! `chunks_completed > 0`, [`crate::pipeline::run_local_inner`] вместо
//! полного STT на full-file WAV'ах вызывает этот модуль и продолжает
//! pipeline (`merge_tracks` → cluster → recap) на собранных дорожках.
//!
//! Возвращает `None` если done-chunks нет — caller остаётся на full-file
//! path (cloud engine + любой случай где chunked-pipeline не активировался).
//!
//! Phase 1 — простая concat + offset. Phase 2 (M13.2.x) подключит global
//! speaker re-clustering поверх per-chunk diarization.

use sqlx::SqlitePool;

use crate::providers::transcription::{DiarizedTranscript, TranscriptSegment};
use crate::{db, AppError};

/// Идентификатор provider'а для assembled транскрипта. Используется при
/// debug-логировании + recap-промпте; ничего критичного.
const ASSEMBLED_PROVIDER: &str = "local-chunked";

/// Загружает done-chunks для звонка и собирает две `DiarizedTranscript`
/// (mic, system) с применением timestamp-offset (`chunk.start_ms / 1000`).
/// Возвращает `Ok(None)` если done-chunks нет.
///
/// Semantics:
/// - mic_t всегда непуст если done-chunks > 0 (mic = критичный track, без
///   него chunk помечается `failed` в [`chunk_runner`]).
/// - sys_t может содержать сегменты только из подмножества chunks, где
///   system_transcript_json не NULL (degraded ok для system STT failures
///   per-chunk).
/// - `lang_detected` берётся из первого chunk'а где он Some (mic priority).
/// - `duration_sec` = max end (s) среди всех собранных сегментов.
pub async fn load_chunked_transcripts(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Option<(DiarizedTranscript, DiarizedTranscript)>, AppError> {
    let rows = db::chunks::list_chunks_by_call(pool, call_id).await?;
    let done: Vec<_> = rows
        .into_iter()
        .filter(|r| r.status == "done" && r.transcript_json.is_some())
        .collect();
    if done.is_empty() {
        return Ok(None);
    }

    let mut mic_segments: Vec<TranscriptSegment> = Vec::new();
    let mut sys_segments: Vec<TranscriptSegment> = Vec::new();
    let mut lang_detected: Option<String> = None;

    for row in &done {
        // [M13 review fix] Защита от corrupt/legacy row с отрицательным
        // start_ms — каст в f64 даст negative offset, segments shift'нутся
        // в прошлое. SQLite не enforce'ит CHECK на этой колонке.
        if row.start_ms < 0 {
            log::warn!(
                "chunk_assembly: skip chunk {}/{} with negative start_ms={}",
                row.call_id,
                row.chunk_idx,
                row.start_ms
            );
            continue;
        }
        let offset_sec = row.start_ms as f64 / 1000.0;

        // mic — guaranteed Some (filter выше), но clippy не любит unwrap.
        let Some(mic_json) = row.transcript_json.as_deref() else {
            continue;
        };
        let mic: DiarizedTranscript = serde_json::from_str(mic_json).map_err(|e| {
            AppError::Other(format!(
                "chunk_assembly: deserialize mic chunk {}/{}: {e}",
                row.call_id, row.chunk_idx
            ))
        })?;
        if lang_detected.is_none() {
            lang_detected = mic.lang_detected.clone();
        }
        for mut seg in mic.segments {
            seg.start += offset_sec;
            seg.end += offset_sec;
            mic_segments.push(seg);
        }

        if let Some(sys_json) = row.system_transcript_json.as_deref() {
            let sys: DiarizedTranscript = serde_json::from_str(sys_json).map_err(|e| {
                AppError::Other(format!(
                    "chunk_assembly: deserialize system chunk {}/{}: {e}",
                    row.call_id, row.chunk_idx
                ))
            })?;
            if lang_detected.is_none() {
                lang_detected = sys.lang_detected.clone();
            }
            for mut seg in sys.segments {
                seg.start += offset_sec;
                seg.end += offset_sec;
                sys_segments.push(seg);
            }
        }
    }

    // [M13 review fix] Authoritative duration = max chunk.end_ms across done
    // chunks. Fallback на max segment.end если end_ms NULL (legacy/partial row).
    // Без этого если последний chunk имел пустой mic+system (короткая фраза),
    // duration был бы меньше реального call duration (≤10 min undercount).
    let duration_from_chunks = done
        .iter()
        .filter_map(|r| r.end_ms)
        .max()
        .map(|ms| ms as f64 / 1000.0)
        .unwrap_or(0.0);
    let duration_from_segments = mic_segments
        .iter()
        .chain(sys_segments.iter())
        .map(|s| s.end)
        .fold(0.0_f64, f64::max);
    let duration_sec = duration_from_chunks.max(duration_from_segments);

    let mic_t = DiarizedTranscript {
        version: 1,
        lang_detected: lang_detected.clone(),
        duration_sec,
        provider: ASSEMBLED_PROVIDER.into(),
        segments: mic_segments,
    };
    let sys_t = DiarizedTranscript {
        version: 1,
        lang_detected,
        duration_sec,
        provider: ASSEMBLED_PROVIDER.into(),
        segments: sys_segments,
    };

    Ok(Some((mic_t, sys_t)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use std::path::PathBuf;

    fn fake_transcript(start: f64, end: f64, text: &str, speaker: &str) -> DiarizedTranscript {
        DiarizedTranscript {
            version: 1,
            lang_detected: Some("ru".into()),
            duration_sec: end - start,
            provider: "mock".into(),
            segments: vec![TranscriptSegment {
                start,
                end,
                text: text.into(),
                speaker_tag: speaker.into(),
                confidence: None,
            }],
        }
    }

    async fn insert_call(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 'recording', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn add_done_chunk(
        pool: &SqlitePool,
        call_id: &str,
        idx: u32,
        start_ms: u64,
        end_ms: u64,
        mic: &DiarizedTranscript,
        sys: Option<&DiarizedTranscript>,
    ) {
        db::chunks::insert_chunk(
            pool,
            call_id,
            idx,
            start_ms,
            &PathBuf::from(format!("/m{idx}")),
            &PathBuf::from(format!("/s{idx}")),
        )
        .await
        .unwrap();
        db::chunks::mark_chunk_processing(pool, call_id, idx)
            .await
            .unwrap();
        let mic_json = serde_json::to_string(mic).unwrap();
        let sys_json = sys.map(|t| serde_json::to_string(t).unwrap());
        db::chunks::mark_chunk_done(pool, call_id, idx, end_ms, &mic_json, sys_json.as_deref())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn returns_none_when_no_chunks() {
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;
        let res = load_chunked_transcripts(&db_t.pool, "c1").await.unwrap();
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn returns_none_when_only_pending_chunks() {
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;
        // Insert pending (NOT done) chunk.
        db::chunks::insert_chunk(
            &db_t.pool,
            "c1",
            0,
            0,
            &PathBuf::from("/m"),
            &PathBuf::from("/s"),
        )
        .await
        .unwrap();
        let res = load_chunked_transcripts(&db_t.pool, "c1").await.unwrap();
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn single_chunk_no_offset() {
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;
        let mic = fake_transcript(1.5, 3.0, "Привет", "owner");
        let sys = fake_transcript(2.0, 4.0, "Hi", "speaker:0");
        add_done_chunk(&db_t.pool, "c1", 0, 0, 600_000, &mic, Some(&sys)).await;

        let (mic_t, sys_t) = load_chunked_transcripts(&db_t.pool, "c1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mic_t.segments.len(), 1);
        assert!((mic_t.segments[0].start - 1.5).abs() < 1e-9);
        assert_eq!(mic_t.segments[0].speaker_tag, "owner");
        assert_eq!(sys_t.segments.len(), 1);
        assert!((sys_t.segments[0].start - 2.0).abs() < 1e-9);
        assert_eq!(sys_t.provider, "local-chunked");
    }

    #[tokio::test]
    async fn multi_chunk_offset_applied() {
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;
        let mic0 = fake_transcript(1.0, 2.0, "first", "owner");
        let mic1 = fake_transcript(1.5, 2.5, "second", "owner");
        add_done_chunk(&db_t.pool, "c1", 0, 0, 600_000, &mic0, None).await;
        add_done_chunk(&db_t.pool, "c1", 1, 600_000, 1_200_000, &mic1, None).await;

        let (mic_t, _) = load_chunked_transcripts(&db_t.pool, "c1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mic_t.segments.len(), 2);
        assert!((mic_t.segments[0].start - 1.0).abs() < 1e-9);
        // chunk_1 start_ms=600_000 → offset 600.0 sec.
        assert!((mic_t.segments[1].start - 601.5).abs() < 1e-9);
        assert!((mic_t.segments[1].end - 602.5).abs() < 1e-9);
        // [M13 review fix] duration_sec = max(chunk.end_ms / 1000) если он >
        // max segment.end. chunk_1.end_ms = 1_200_000 → 1200s, segments max
        // = 602.5s → берётся 1200.0 (authoritative).
        assert!((mic_t.duration_sec - 1200.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn mixed_mic_only_and_dual_track() {
        // chunk_0 dual-track, chunk_1 mic-only (system failed degraded).
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;
        let mic0 = fake_transcript(0.0, 5.0, "mic0", "owner");
        let sys0 = fake_transcript(2.0, 4.0, "sys0", "speaker:0");
        let mic1 = fake_transcript(0.0, 5.0, "mic1", "owner");
        add_done_chunk(&db_t.pool, "c1", 0, 0, 600_000, &mic0, Some(&sys0)).await;
        add_done_chunk(&db_t.pool, "c1", 1, 600_000, 1_200_000, &mic1, None).await;

        let (mic_t, sys_t) = load_chunked_transcripts(&db_t.pool, "c1")
            .await
            .unwrap()
            .unwrap();
        // mic: 2 chunks contribute → 2 сегмента.
        assert_eq!(mic_t.segments.len(), 2);
        // sys: только chunk_0 contribute → 1 сегмент.
        assert_eq!(sys_t.segments.len(), 1);
        assert!((sys_t.segments[0].start - 2.0).abs() < 1e-9);
    }
}
