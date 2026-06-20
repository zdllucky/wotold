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

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::pipeline::merge::OWNER_TAG;
use crate::pipeline::owner_identify::identify_owner_speaker;
use crate::pipeline::speaker_reclustering::{
    agglomerative_cluster, EmbeddingPoint, DEFAULT_COSINE_THRESHOLD,
};
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
    // [M13.2.1] Per-chunk per-segment chunk_idx — нужен для остаточного remap'а
    // после `agglomerative_cluster`. Не храним прямо в TranscriptSegment'е (он
    // shared контракт TS), а параллельным Vec'ом 1-к-1 с mic_segments + sys_segments.
    let mut mic_segment_chunk_idx: Vec<u32> = Vec::new();
    let mut sys_segment_chunk_idx: Vec<u32> = Vec::new();
    // [M13.2.1] Собираем EmbeddingPoint'ы из всех done-chunks. None embeddings_json
    // — pre-Phase 2 legacy row, не участвует в clustering (identity passthrough).
    let mut points: Vec<EmbeddingPoint> = Vec::new();

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
            mic_segment_chunk_idx.push(row.chunk_idx);
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
                sys_segment_chunk_idx.push(row.chunk_idx);
            }
        }

        // [M13.2.1] Embeddings JSON → EmbeddingPoint'ы для cross-chunk clustering.
        if let Some(emb_json) = row.embeddings_json.as_deref() {
            match serde_json::from_str::<HashMap<String, Vec<f32>>>(emb_json) {
                Ok(map) => {
                    for (tag, vec) in map {
                        points.push(EmbeddingPoint {
                            chunk_idx: row.chunk_idx,
                            local_tag: tag,
                            vec,
                        });
                    }
                }
                Err(e) => {
                    // Corrupt embeddings JSON — degraded ok, identity remap для
                    // этого chunk'а (его tags не меняются).
                    log::warn!(
                        "chunk_assembly: parse embeddings_json chunk {}/{} failed: {e}",
                        row.call_id,
                        row.chunk_idx
                    );
                }
            }
        }
    }

    // [M13.2.1] Global cross-chunk speaker remap. Если points пустой (legacy
    // chunks без embeddings_json) — agglomerative_cluster вернёт empty map →
    // remap_segment_tag будет identity (segment.speaker_tag не меняется).
    let global_map = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
    apply_global_remap(&mut mic_segments, &mic_segment_chunk_idx, &global_map);
    apply_global_remap(&mut sys_segments, &sys_segment_chunk_idx, &global_map);

    // [M13 follow-up] Owner identification: после global remap у mic-сегментов
    // могут быть `speaker:N` tags (если в chunk_runner работала mic diarization).
    // Сводим их к OWNER_TAG для speaker'а который реально — владелец.
    // Cross-track reflection: если owner отражается в system-дорожке и Phase 2
    // reclustering объединил их (один global tag), relabel'им и system tagged.
    //
    // Если mic_diarization была OFF — все mic-сегменты уже OWNER_TAG'd
    // в провайдере / force_owner_track выше; identify вернёт "owner" → no-op.
    // Если identify вернёт None (пустой mic) — пропускаем.
    if let Ok(Some(owner_local_tag)) =
        identify_owner_speaker_from_cluster_map(pool, &mic_segments, &points).await
    {
        if owner_local_tag != OWNER_TAG {
            for seg in mic_segments.iter_mut() {
                if seg.speaker_tag == owner_local_tag {
                    seg.speaker_tag = OWNER_TAG.to_string();
                }
            }
            for seg in sys_segments.iter_mut() {
                if seg.speaker_tag == owner_local_tag {
                    seg.speaker_tag = OWNER_TAG.to_string();
                }
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

/// [M13.2.1] Применить global remap inplace: для каждого segment'а ищем
/// `(chunk_idx, local_tag)` в `global_map` и заменяем `speaker_tag` на global.
/// Если ключ отсутствует (например legacy chunk без embeddings) — identity
/// (tag сохраняется).
fn apply_global_remap(
    segments: &mut [TranscriptSegment],
    chunk_idx_per_segment: &[u32],
    global_map: &HashMap<(u32, String), String>,
) {
    debug_assert_eq!(segments.len(), chunk_idx_per_segment.len());
    for (seg, &chunk_idx) in segments.iter_mut().zip(chunk_idx_per_segment.iter()) {
        if let Some(global) = global_map.get(&(chunk_idx, seg.speaker_tag.clone())) {
            if global != &seg.speaker_tag {
                seg.speaker_tag = global.clone();
            }
        }
    }
}

/// [M13 follow-up] Wrapper над `owner_identify::identify_owner_speaker` —
/// собирает per-global-tag cluster embeddings (mean-pool) из всех Phase 2
/// EmbeddingPoint'ов, затем вызывает identification. Mean-pool нужен потому
/// что один global tag может приходить из N chunks (после reclustering).
async fn identify_owner_speaker_from_cluster_map(
    pool: &SqlitePool,
    mic_segments: &[TranscriptSegment],
    points: &[EmbeddingPoint],
) -> Result<Option<String>, AppError> {
    // Aggregate per-global cluster (mean-pool across chunks). Здесь по сути
    // ещё раз mean-pool после Phase 2 — но т.к. points содержат local tags,
    // нужно сначала сопоставить с tags в mic_segments. Самый простой путь:
    // взять каждую точку с local_tag совпадающим с одним из tags на mic, и
    // pool by tag. Этого достаточно для biometric matching (cosine на
    // mean-pool similar к single-chunk mean).
    let mut cluster_embeddings: HashMap<String, Vec<Vec<f32>>> = HashMap::new();
    let mic_tags: std::collections::HashSet<&str> = mic_segments
        .iter()
        .map(|s| s.speaker_tag.as_str())
        .collect();
    for p in points {
        if mic_tags.contains(p.local_tag.as_str()) && !p.vec.is_empty() {
            cluster_embeddings
                .entry(p.local_tag.clone())
                .or_default()
                .push(p.vec.clone());
        }
    }
    let mean_embeddings: HashMap<String, Vec<f32>> = cluster_embeddings
        .into_iter()
        .filter_map(|(tag, vecs)| {
            if vecs.is_empty() {
                return None;
            }
            let dim = vecs[0].len();
            let mut mean = vec![0.0_f32; dim];
            let mut count = 0_f32;
            for v in &vecs {
                if v.len() != dim {
                    continue;
                }
                for (m, x) in mean.iter_mut().zip(v.iter()) {
                    *m += *x;
                }
                count += 1.0;
            }
            if count == 0.0 {
                return None;
            }
            for m in mean.iter_mut() {
                *m /= count;
            }
            Some((tag, mean))
        })
        .collect();

    identify_owner_speaker(pool, mic_segments, &mean_embeddings).await
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
        add_done_chunk_full(pool, call_id, idx, start_ms, end_ms, mic, sys, None).await;
    }

    /// Расширенный helper: дополнительно сериализованный
    /// `HashMap<String, Vec<f32>>` для per-chunk embeddings_json (M13.2.1).
    #[allow(clippy::too_many_arguments)]
    async fn add_done_chunk_full(
        pool: &SqlitePool,
        call_id: &str,
        idx: u32,
        start_ms: u64,
        end_ms: u64,
        mic: &DiarizedTranscript,
        sys: Option<&DiarizedTranscript>,
        embeddings_json: Option<&str>,
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
        db::chunks::mark_chunk_done(
            pool,
            call_id,
            idx,
            end_ms,
            &mic_json,
            sys_json.as_deref(),
            embeddings_json,
        )
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

    // ============================================================
    // [M13.2.1] Cross-chunk speaker re-clustering tests
    // ============================================================

    /// Helper: JSON-сериализовать `{speaker_tag → embedding}` для embeddings_json.
    fn embed_json(pairs: &[(&str, Vec<f32>)]) -> String {
        let map: std::collections::HashMap<String, Vec<f32>> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();
        serde_json::to_string(&map).unwrap()
    }

    #[tokio::test]
    async fn cross_chunk_same_speaker_collapses_to_one_global() {
        // 2 chunks, оба с "speaker:0" но **различными local meaning'ами** —
        // assembly должен через cluster mean cosine merge оба chunk:0 в один
        // global tag.
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;
        let sys0 = fake_transcript(0.0, 5.0, "first", "speaker:0");
        let sys1 = fake_transcript(0.0, 5.0, "second", "speaker:0");
        // Same embedding vector → cosine = 1.0 → merged.
        let same_emb = vec![1.0_f32, 0.0, 0.0];
        let emb0 = embed_json(&[("speaker:0", same_emb.clone())]);
        let emb1 = embed_json(&[("speaker:0", same_emb)]);
        add_done_chunk_full(
            &db_t.pool,
            "c1",
            0,
            0,
            600_000,
            &fake_transcript(0.0, 5.0, "mic0", "owner"),
            Some(&sys0),
            Some(&emb0),
        )
        .await;
        add_done_chunk_full(
            &db_t.pool,
            "c1",
            1,
            600_000,
            1_200_000,
            &fake_transcript(0.0, 5.0, "mic1", "owner"),
            Some(&sys1),
            Some(&emb1),
        )
        .await;

        let (mic_t, sys_t) = load_chunked_transcripts(&db_t.pool, "c1")
            .await
            .unwrap()
            .unwrap();
        // Owner — passthrough.
        assert!(mic_t.segments.iter().all(|s| s.speaker_tag == "owner"));
        // Sys: оба chunk'а получают same global tag.
        assert_eq!(sys_t.segments.len(), 2);
        let tag0 = &sys_t.segments[0].speaker_tag;
        let tag1 = &sys_t.segments[1].speaker_tag;
        assert_eq!(
            tag0, tag1,
            "same-speaker должен collapse: got {tag0} vs {tag1}"
        );
        // И это global tag (а не legacy "speaker:0"), хотя по детерминизму
        // первый clusterable получает "speaker:0", так что совпадает.
        assert_eq!(tag0, "speaker:0");
    }

    #[tokio::test]
    async fn cross_chunk_different_speakers_stay_separate() {
        // 2 chunks с орthogonal embedding'ами — разные global tags.
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;
        let sys0 = fake_transcript(0.0, 5.0, "first", "speaker:0");
        let sys1 = fake_transcript(0.0, 5.0, "second", "speaker:0");
        let emb0 = embed_json(&[("speaker:0", vec![1.0_f32, 0.0, 0.0])]);
        let emb1 = embed_json(&[("speaker:0", vec![0.0_f32, 1.0, 0.0])]);
        add_done_chunk_full(
            &db_t.pool,
            "c1",
            0,
            0,
            600_000,
            &fake_transcript(0.0, 5.0, "mic0", "owner"),
            Some(&sys0),
            Some(&emb0),
        )
        .await;
        add_done_chunk_full(
            &db_t.pool,
            "c1",
            1,
            600_000,
            1_200_000,
            &fake_transcript(0.0, 5.0, "mic1", "owner"),
            Some(&sys1),
            Some(&emb1),
        )
        .await;

        let (_mic_t, sys_t) = load_chunked_transcripts(&db_t.pool, "c1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sys_t.segments.len(), 2);
        assert_ne!(
            sys_t.segments[0].speaker_tag, sys_t.segments[1].speaker_tag,
            "orthogonal embeddings должны быть в разных global clusters"
        );
    }

    #[tokio::test]
    async fn mixed_chunks_with_and_without_embeddings_degraded_ok() {
        // chunk_0 имеет embeddings, chunk_1 — None (legacy / extract failed).
        // Assembly не падает; chunk_1 segments сохраняют local tag.
        let db_t = fresh_db().await;
        insert_call(&db_t.pool, "c1").await;
        let sys0 = fake_transcript(0.0, 5.0, "first", "speaker:0");
        let sys1 = fake_transcript(0.0, 5.0, "second", "speaker:0");
        let emb0 = embed_json(&[("speaker:0", vec![1.0_f32, 0.0, 0.0])]);
        add_done_chunk_full(
            &db_t.pool,
            "c1",
            0,
            0,
            600_000,
            &fake_transcript(0.0, 5.0, "mic0", "owner"),
            Some(&sys0),
            Some(&emb0),
        )
        .await;
        add_done_chunk_full(
            &db_t.pool,
            "c1",
            1,
            600_000,
            1_200_000,
            &fake_transcript(0.0, 5.0, "mic1", "owner"),
            Some(&sys1),
            None,
        )
        .await;

        let (_mic_t, sys_t) = load_chunked_transcripts(&db_t.pool, "c1")
            .await
            .unwrap()
            .unwrap();
        // chunk_0 remap'ится в global "speaker:0" (first clusterable),
        // chunk_1 — identity (без embeddings) — local "speaker:0".
        // Оба совпадают по строке, не conflict'ят.
        assert_eq!(sys_t.segments[0].speaker_tag, "speaker:0");
        assert_eq!(sys_t.segments[1].speaker_tag, "speaker:0");
    }
}
