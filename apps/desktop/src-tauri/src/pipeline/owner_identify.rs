//! [M13 follow-up] Определить какой из локальных speaker-tag'ов на mic
//! принадлежит владельцу устройства (M3.7 invariant). До этого helper'а
//! mic-дорожка force-tegged'илась как OWNER целиком (`force_owner_track`).
//! С включённой mic diarization (см. `pipeline::diarize_mic_track`) нужен
//! способ выбрать конкретный speaker:N tag, который реально принадлежит
//! владельцу, чтобы перетегать его в OWNER_TAG, а остальные оставить
//! как speaker:N (часть Phase 2 global remap).
//!
//! Two-stage strategy:
//!
//! 1. **Biometric** — если у owner ≥1 voice_sample (накопленные через
//!    M3.5 confirm flow), для каждого mic-speaker'а берём его cluster
//!    embedding из chunk_assembly + cosine match через
//!    `matching::rank_candidates` против owner embeddings. Best match
//!    с score ≥ threshold (0.5) → owner.
//!
//! 2. **Fallback (no voice_samples ИЛИ biometric ниже threshold)** —
//!    primary speaker by total duration: эмпирически владелец доминирует
//!    в собственной записи (записывает свою сторону разговора через
//!    свой микрофон). Возвращаем local_tag с максимальной суммарной
//!    длительностью segments на mic.
//!
//! Returns `None` если на mic ноль валидных speaker tags → caller
//! применяет старое `force_owner_track` (whole-track-as-owner).

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::db::voice_samples;
use crate::matching::{rank_candidates, ContactSamples};
use crate::providers::transcription::TranscriptSegment;
use crate::AppError;

/// Cosine threshold для biometric owner-match. Ниже — переход на duration
/// fallback. 0.5 совпадает с production threshold для contact matching
/// (см. `matching::rank_candidates` callsite в pipeline/mod.rs).
const OWNER_BIOMETRIC_THRESHOLD: f32 = 0.5;

/// Определить какой local speaker tag = владелец устройства на mic-дорожке.
///
/// Args:
/// - `pool` — DB pool (читаем owner voice_samples).
/// - `mic_segments` — список mic-track segments **с уже применённым global
///   remap** (Phase 2 reclustering). Tags могут быть `"speaker:0"`,
///   `"speaker:1"`, ..., `"speaker:unknown"`. OWNER_TAG в этом списке не
///   ожидается (mic diarization выключила force_owner_track).
/// - `cluster_embeddings` — `HashMap<global_tag, embedding[256]>` собранный
///   в chunk_assembly из per-chunk `embeddings_json`. Используется для
///   biometric match. Может быть пустой (legacy chunks / voice-onnx off)
///   → переключаемся на duration fallback.
///
/// Returns:
/// - `Some(local_tag)` — этот tag надо перетегать в `OWNER_TAG`.
/// - `None` — нет speakers на mic (пустой transcript / corrupt).
pub async fn identify_owner_speaker(
    pool: &SqlitePool,
    mic_segments: &[TranscriptSegment],
    cluster_embeddings: &HashMap<String, Vec<f32>>,
) -> Result<Option<String>, AppError> {
    // Distinct tags + per-tag duration (для fallback и для filter'а).
    let mut duration_by_tag: HashMap<String, f64> = HashMap::new();
    for seg in mic_segments {
        let tag = seg.speaker_tag.trim();
        if tag.is_empty() {
            continue;
        }
        let dur = (seg.end - seg.start).max(0.0);
        *duration_by_tag.entry(tag.to_string()).or_insert(0.0) += dur;
    }
    if duration_by_tag.is_empty() {
        return Ok(None);
    }

    // Stage 1: biometric match (если есть owner samples + cluster embeddings).
    let owner_embeddings = voice_samples::list_owner_embeddings(pool).await?;
    if !owner_embeddings.is_empty() && !cluster_embeddings.is_empty() {
        let owner_contact = vec![ContactSamples {
            contact_id: "__owner__".into(),
            display_name: "owner".into(),
            embeddings: owner_embeddings,
        }];
        let mut best: Option<(String, f32)> = None;
        for tag in duration_by_tag.keys() {
            let Some(emb) = cluster_embeddings.get(tag) else {
                continue;
            };
            let candidates = rank_candidates(emb, &owner_contact, OWNER_BIOMETRIC_THRESHOLD, 1);
            if let Some(c) = candidates.first() {
                if best.as_ref().map(|(_, s)| c.score > *s).unwrap_or(true) {
                    best = Some((tag.clone(), c.score));
                }
            }
        }
        if let Some((tag, score)) = best {
            log::info!("identify_owner_speaker: biometric match {tag} (score={score:.3})");
            return Ok(Some(tag));
        }
    }

    // Stage 2: fallback — primary speaker by total duration.
    let primary = duration_by_tag
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(tag, _)| tag.clone());
    if let Some(ref tag) = primary {
        log::info!("identify_owner_speaker: duration fallback → {tag} (no biometric match)");
    }
    Ok(primary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use crate::embeddings;

    fn ts(start: f64, end: f64, tag: &str) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: "x".into(),
            speaker_tag: tag.into(),
            confidence: None,
        }
    }

    async fn insert_owner_with_samples(pool: &SqlitePool, embeddings_list: &[Vec<f32>]) {
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES ('owner-c', 'Owner', 1, '{}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(pool)
        .await
        .unwrap();
        for (i, emb) in embeddings_list.iter().enumerate() {
            let blob = embeddings::embedding_to_bytes(emb);
            sqlx::query(
                "INSERT INTO voice_samples
                    (id, contact_id, source_call, quality, embedding, created_at)
                 VALUES (?1, 'owner-c', NULL, 1.0, ?2, CURRENT_TIMESTAMP)",
            )
            .bind(format!("vs-{i}"))
            .bind(blob)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn empty_mic_segments_returns_none() {
        let db = fresh_db().await;
        let cluster_embeddings = HashMap::new();
        let out = identify_owner_speaker(&db.pool, &[], &cluster_embeddings)
            .await
            .unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn single_speaker_returns_that_speaker() {
        // Один local tag на mic → fallback duration вернёт его (нет owner samples).
        let db = fresh_db().await;
        let segs = vec![ts(0.0, 5.0, "speaker:0"), ts(5.0, 10.0, "speaker:0")];
        let out = identify_owner_speaker(&db.pool, &segs, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(out.as_deref(), Some("speaker:0"));
    }

    #[tokio::test]
    async fn primary_speaker_wins_on_duration_fallback() {
        // speaker:0 = 7s, speaker:1 = 3s → owner = speaker:0 (нет voice samples).
        let db = fresh_db().await;
        let segs = vec![ts(0.0, 7.0, "speaker:0"), ts(7.0, 10.0, "speaker:1")];
        let out = identify_owner_speaker(&db.pool, &segs, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(out.as_deref(), Some("speaker:0"));
    }

    #[tokio::test]
    async fn biometric_match_overrides_duration() {
        // speaker:0 говорит 30%, speaker:1 — 70%. Но owner voice_sample
        // соответствует speaker:0 (identical embedding). Biometric выигрывает.
        let db = fresh_db().await;
        let owner_emb = vec![1.0_f32, 0.0, 0.0];
        insert_owner_with_samples(&db.pool, std::slice::from_ref(&owner_emb)).await;

        let mut cluster_embeddings = HashMap::new();
        cluster_embeddings.insert("speaker:0".to_string(), owner_emb);
        cluster_embeddings.insert("speaker:1".to_string(), vec![0.0_f32, 1.0, 0.0]);

        let segs = vec![
            ts(0.0, 3.0, "speaker:0"),  // 30%
            ts(3.0, 10.0, "speaker:1"), // 70%
        ];
        let out = identify_owner_speaker(&db.pool, &segs, &cluster_embeddings)
            .await
            .unwrap();
        // Biometric → speaker:0, не доминирующий speaker:1.
        assert_eq!(out.as_deref(), Some("speaker:0"));
    }

    #[tokio::test]
    async fn biometric_below_threshold_falls_back_to_duration() {
        // owner voice_sample orthogonal обоим speakers (cosine ≈ 0)
        // → biometric не сработает → fallback на duration. Primary = speaker:1.
        let db = fresh_db().await;
        let owner_emb = vec![0.0_f32, 0.0, 1.0]; // orthogonal обоим
        insert_owner_with_samples(&db.pool, &[owner_emb]).await;

        let mut cluster_embeddings = HashMap::new();
        cluster_embeddings.insert("speaker:0".to_string(), vec![1.0_f32, 0.0, 0.0]);
        cluster_embeddings.insert("speaker:1".to_string(), vec![0.0_f32, 1.0, 0.0]);

        let segs = vec![
            ts(0.0, 3.0, "speaker:0"),
            ts(3.0, 10.0, "speaker:1"), // dominates
        ];
        let out = identify_owner_speaker(&db.pool, &segs, &cluster_embeddings)
            .await
            .unwrap();
        assert_eq!(out.as_deref(), Some("speaker:1"));
    }

    #[tokio::test]
    async fn no_owner_contact_falls_back_to_duration() {
        // Owner contact не существует → list_owner_embeddings вернёт пустой →
        // biometric пропускается, primary by duration.
        let db = fresh_db().await;
        let mut cluster_embeddings = HashMap::new();
        cluster_embeddings.insert("speaker:0".to_string(), vec![1.0_f32, 0.0]);

        let segs = vec![ts(0.0, 5.0, "speaker:0")];
        let out = identify_owner_speaker(&db.pool, &segs, &cluster_embeddings)
            .await
            .unwrap();
        assert_eq!(out.as_deref(), Some("speaker:0"));
    }

    #[tokio::test]
    async fn list_owner_embeddings_works_in_isolation() {
        // Sanity: пустая БД → пустой Vec.
        let db = fresh_db().await;
        let out = voice_samples::list_owner_embeddings(&db.pool)
            .await
            .unwrap();
        assert!(out.is_empty());

        // С owner + 2 samples → возвращаются оба.
        insert_owner_with_samples(&db.pool, &[vec![1.0_f32, 0.0], vec![0.0_f32, 1.0]]).await;
        let out = voice_samples::list_owner_embeddings(&db.pool)
            .await
            .unwrap();
        assert_eq!(out.len(), 2);
    }
}
