//! [M13.2.1] Cross-chunk global speaker re-clustering.
//!
//! Каждый chunk имеет свой local `speaker:0/1/2/...` tag. Физически один и
//! тот же человек в chunk N и chunk M может получить разные local tags
//! (диаризация per-chunk не знает о других chunks). Этот модуль сводит
//! local tags → global IDs через **agglomerative single-link cosine
//! clustering** на WeSpeaker embeddings (cluster mean per speaker per chunk).
//!
//! Threshold 0.75 — WeSpeaker / ECAPA-TDNN embeddings на same-speaker обычно
//! дают cosine ≥ 0.9, на different-speaker ≤ 0.5; 0.75 — safe-middle с
//! tolerance на shorter segments / noise.
//!
//! Invariants:
//! - `pipeline::merge::OWNER_TAG` ("owner") никогда не сливается с другими
//!   speakers (M3.7 hard rule — mic-дорожка = owner детерминированно).
//! - `local_engine::diarization::SPEAKER_UNKNOWN` ("speaker:unknown") —
//!   overflow при > `MAX_LOCAL_SPEAKERS=4`, не сливается ни с чем.
//! - Empty embeddings vector → точка skip'ается, identity mapping для неё.
//! - Output global tags нумеруются детерминированно: `speaker:0`, `speaker:1`,
//!   ... в порядке первого появления в input (stable across runs).
//!
//! Complexity: naive O(n² · m) где n = число точек (= chunks × speakers,
//! обычно ≤ 48), m = embedding dim (256). Для production-scale ничтожно.

use std::collections::HashMap;

use crate::embeddings::cosine_similarity;
use crate::local_engine::diarization::SPEAKER_UNKNOWN;
use crate::pipeline::merge::OWNER_TAG;

/// Одна точка для clustering: (chunk_idx, local_speaker_tag, embedding).
#[derive(Debug, Clone)]
pub struct EmbeddingPoint {
    pub chunk_idx: u32,
    pub local_tag: String,
    /// L2-normalized cluster vector (mean-pooled per-segment embeddings) —
    /// формат который выдаёт `pipeline::clusters::extract_clusters`.
    pub vec: Vec<f32>,
}

/// Default cosine threshold для same-speaker merge. WeSpeaker / ECAPA на
/// same-speaker даёт ≥ 0.9, на different-speaker ≤ 0.5; 0.75 — safe-middle.
pub const DEFAULT_COSINE_THRESHOLD: f32 = 0.75;

/// Минимальная dimension эмбеддинга — короче считаем mal-formed и skip'аем.
/// EMBEDDING_DIM=256 (WeSpeaker), но в тестах используются 4-dim CountingEmbedder
/// vectors; для гибкости берём 2 как floor (3D пространство = можно отличить
/// 2 ortho кластера).
const MIN_EMBEDDING_DIM: usize = 2;

/// Сгруппировать точки в global clusters через agglomerative single-link.
///
/// Возвращает `HashMap<(chunk_idx, local_tag), global_tag>` — где
/// `global_tag` это:
/// - `OWNER_TAG` если original tag == OWNER_TAG (passthrough);
/// - `SPEAKER_UNKNOWN` если original tag == SPEAKER_UNKNOWN (passthrough);
/// - `speaker:0`, `speaker:1`, ... для clustered remap'а.
///
/// Точки с empty embedding или с `local_tag` в `{OWNER_TAG, SPEAKER_UNKNOWN}`
/// не участвуют в clustering — они мапятся 1:1 (identity).
pub fn agglomerative_cluster(
    points: &[EmbeddingPoint],
    cosine_threshold: f32,
) -> HashMap<(u32, String), String> {
    let mut out: HashMap<(u32, String), String> = HashMap::new();

    // Identity для passthrough точек: owner / unknown / empty embedding.
    let mut clusterable_indices: Vec<usize> = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        if p.local_tag == OWNER_TAG || p.local_tag == SPEAKER_UNKNOWN {
            out.insert((p.chunk_idx, p.local_tag.clone()), p.local_tag.clone());
            continue;
        }
        if p.vec.len() < MIN_EMBEDDING_DIM {
            // Empty / too-short embedding — identity mapping, не участвует
            // в clustering. UI получит local tag без remap (degraded ok).
            out.insert((p.chunk_idx, p.local_tag.clone()), p.local_tag.clone());
            continue;
        }
        clusterable_indices.push(i);
    }

    // Union-Find на clusterable points. Inits: каждая точка — свой cluster.
    let n = clusterable_indices.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] == i {
            return i;
        }
        let root = find(parent, parent[i]);
        parent[i] = root; // path compression
        root
    }

    // Single-link: для каждой пары — если cosine ≥ threshold, объединить.
    // Naive O(n²), n ≤ 48 в реалистичных сценариях.
    for a in 0..n {
        for b in (a + 1)..n {
            let pa = &points[clusterable_indices[a]];
            let pb = &points[clusterable_indices[b]];
            // Dim mismatch — skip (можно случиться если embedder вернул
            // empty в B3.x scaffold вместе с real 256d из B3.7).
            if pa.vec.len() != pb.vec.len() {
                continue;
            }
            let sim = cosine_similarity(&pa.vec, &pb.vec);
            if sim >= cosine_threshold {
                let ra = find(&mut parent, a);
                let rb = find(&mut parent, b);
                if ra != rb {
                    parent[ra] = rb;
                }
            }
        }
    }

    // Назначить детерминированные global tags по порядку первого появления.
    // Iterate в исходном order input точек, чтобы global:0 был у первой
    // clusterable точки.
    let mut root_to_tag: HashMap<usize, String> = HashMap::new();
    let mut next_global_id: u32 = 0;
    for (slot, &orig_idx) in clusterable_indices.iter().enumerate() {
        let root = find(&mut parent, slot);
        let tag = root_to_tag.entry(root).or_insert_with(|| {
            let t = format!("speaker:{next_global_id}");
            next_global_id += 1;
            t
        });
        let p = &points[orig_idx];
        out.insert((p.chunk_idx, p.local_tag.clone()), tag.clone());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(chunk_idx: u32, tag: &str, vec: Vec<f32>) -> EmbeddingPoint {
        EmbeddingPoint {
            chunk_idx,
            local_tag: tag.into(),
            vec,
        }
    }

    #[test]
    fn empty_points_returns_empty_map() {
        let out = agglomerative_cluster(&[], DEFAULT_COSINE_THRESHOLD);
        assert!(out.is_empty());
    }

    #[test]
    fn identical_vectors_collapse_to_single_cluster() {
        let v = vec![1.0, 0.0, 0.0];
        let points = vec![
            pt(0, "speaker:0", v.clone()),
            pt(1, "speaker:0", v.clone()),
            pt(2, "speaker:0", v),
        ];
        let out = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
        let tags: std::collections::HashSet<_> = out.values().collect();
        assert_eq!(tags.len(), 1, "expected 1 global cluster, got {tags:?}");
        // Все точки → speaker:0.
        assert_eq!(out.get(&(0, "speaker:0".into())).unwrap(), "speaker:0");
        assert_eq!(out.get(&(1, "speaker:0".into())).unwrap(), "speaker:0");
        assert_eq!(out.get(&(2, "speaker:0".into())).unwrap(), "speaker:0");
    }

    #[test]
    fn orthogonal_vectors_stay_separate() {
        let points = vec![
            pt(0, "speaker:0", vec![1.0, 0.0, 0.0]),
            pt(0, "speaker:1", vec![0.0, 1.0, 0.0]),
            pt(0, "speaker:2", vec![0.0, 0.0, 1.0]),
        ];
        let out = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
        let tags: std::collections::HashSet<_> = out.values().collect();
        assert_eq!(tags.len(), 3, "expected 3 separate clusters, got {tags:?}");
    }

    #[test]
    fn owner_tag_never_merges() {
        // Owner с любым embedding должен мапиться в OWNER_TAG, даже если
        // его вектор идентичен другому speaker.
        let v = vec![1.0, 0.0, 0.0];
        let points = vec![
            pt(0, OWNER_TAG, v.clone()),
            pt(1, OWNER_TAG, v.clone()),
            pt(0, "speaker:0", v),
        ];
        let out = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
        assert_eq!(out.get(&(0, OWNER_TAG.into())).unwrap(), OWNER_TAG);
        assert_eq!(out.get(&(1, OWNER_TAG.into())).unwrap(), OWNER_TAG);
        // speaker:0 получает global tag (speaker:0 после remap'а).
        assert_eq!(out.get(&(0, "speaker:0".into())).unwrap(), "speaker:0");
    }

    #[test]
    fn unknown_tag_never_merges() {
        let v = vec![1.0, 0.0, 0.0];
        let points = vec![pt(0, SPEAKER_UNKNOWN, v.clone()), pt(0, "speaker:0", v)];
        let out = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
        assert_eq!(
            out.get(&(0, SPEAKER_UNKNOWN.into())).unwrap(),
            SPEAKER_UNKNOWN
        );
        assert_eq!(out.get(&(0, "speaker:0".into())).unwrap(), "speaker:0");
    }

    #[test]
    fn empty_embedding_passes_through_with_local_tag() {
        let points = vec![
            pt(0, "speaker:0", vec![]),
            pt(1, "speaker:0", vec![1.0, 0.0, 0.0]),
        ];
        let out = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
        // Empty embedding → identity (passthrough). Точка с реальным embed
        // получает свой global tag (speaker:0 потому что first clusterable).
        assert_eq!(out.get(&(0, "speaker:0".into())).unwrap(), "speaker:0");
        assert_eq!(out.get(&(1, "speaker:0".into())).unwrap(), "speaker:0");
    }

    #[test]
    fn threshold_boundary_above_merges() {
        // Vectors с известным cosine 0.8 — должны merged'нуться при threshold 0.75.
        // u = [1, 0], v = [cos(36°), sin(36°)] ≈ [0.809, 0.588]
        // cosine(u, v) = 0.809 (наш threshold 0.75 — merged).
        let u = vec![1.0_f32, 0.0];
        let v = vec![0.809_f32, 0.588];
        let points = vec![pt(0, "speaker:0", u), pt(1, "speaker:0", v)];
        let out = agglomerative_cluster(&points, 0.75);
        assert_eq!(
            out.get(&(0, "speaker:0".into())).unwrap(),
            out.get(&(1, "speaker:0".into())).unwrap(),
            "vectors с cosine ≈ 0.81 ≥ 0.75 должны merged"
        );
    }

    #[test]
    fn threshold_boundary_below_separates() {
        // u = [1, 0], v = [0.5, 0.866] (60°) → cosine = 0.5. При threshold 0.75 — separate.
        let u = vec![1.0_f32, 0.0];
        let v = vec![0.5_f32, 0.866];
        let points = vec![pt(0, "speaker:0", u), pt(1, "speaker:0", v)];
        let out = agglomerative_cluster(&points, 0.75);
        assert_ne!(
            out.get(&(0, "speaker:0".into())).unwrap(),
            out.get(&(1, "speaker:0".into())).unwrap(),
            "vectors с cosine ≈ 0.5 < 0.75 должны быть в разных clusters"
        );
    }

    #[test]
    fn global_tags_numbered_in_appearance_order() {
        // Первая clusterable точка → speaker:0. Вторая (different cluster) → speaker:1.
        // Owner присутствует, но не получает speaker:N — остаётся OWNER_TAG.
        let points = vec![
            pt(0, OWNER_TAG, vec![1.0, 0.0]),
            pt(0, "speaker:0", vec![0.0, 1.0]), // first clusterable
            pt(0, "speaker:1", vec![1.0, 0.0]), // second clusterable, different
        ];
        let out = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
        assert_eq!(out.get(&(0, OWNER_TAG.into())).unwrap(), OWNER_TAG);
        assert_eq!(out.get(&(0, "speaker:0".into())).unwrap(), "speaker:0");
        assert_eq!(out.get(&(0, "speaker:1".into())).unwrap(), "speaker:1");
    }

    #[test]
    fn cross_chunk_same_speaker_collapses() {
        // Same vector в 3 разных chunks → 1 global cluster, все мапятся в speaker:0.
        let v = vec![0.6, 0.8];
        let points = vec![
            pt(0, "speaker:0", v.clone()),
            pt(1, "speaker:0", v.clone()),
            pt(2, "speaker:0", v),
        ];
        let out = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
        let unique: std::collections::HashSet<_> = out.values().collect();
        assert_eq!(unique.len(), 1, "ожидался 1 global cluster, got {unique:?}");
    }

    #[test]
    fn dim_mismatch_does_not_panic() {
        // Vector len mismatch между точками — cosine_similarity вернёт 0.0
        // → не сливаются, no panic.
        let points = vec![
            pt(0, "speaker:0", vec![1.0, 0.0]),
            pt(1, "speaker:0", vec![1.0, 0.0, 0.0]),
        ];
        let out = agglomerative_cluster(&points, DEFAULT_COSINE_THRESHOLD);
        // Оба остаются clusterable, но в разных clusters (mismatch → cosine=0).
        assert_eq!(out.len(), 2);
        assert_ne!(
            out.get(&(0, "speaker:0".into())).unwrap(),
            out.get(&(1, "speaker:0".into())).unwrap()
        );
    }
}
