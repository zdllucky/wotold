//! [M15.11] Reciprocal Rank Fusion — слияние ранк-листов BM25 и cosine
//! (PRD §6.3: RRF k=60 над top-30 + top-30).
//!
//! `score(id) = Σ по листам 1/(k + rank)`, rank 1-based. Скоры каналов
//! несравнимы напрямую (bm25 — отрицательный лог, cosine — [-1..1]),
//! RRF работает только с позициями — поэтому листы приходят уже
//! отсортированными best-first, значения скоров не передаются.

/// Канон PRD §6.3.
pub const RRF_K: f64 = 60.0;

/// Слить два ранк-листа. Выход — best-first, скор = сумма reciprocal ranks.
/// Тай-брейк детерминированный (score desc, затем passage_id asc) — иначе
/// нумерация источников [1..N] в промпте плавала бы между запусками.
pub fn rrf_fuse(bm25_ranked: &[i64], cosine_ranked: &[i64], k: f64) -> Vec<(i64, f64)> {
    let mut scores: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for list in [bm25_ranked, cosine_ranked] {
        for (i, id) in list.iter().enumerate() {
            *scores.entry(*id).or_insert(0.0) += 1.0 / (k + (i as f64) + 1.0);
        }
    }
    let mut out: Vec<(i64, f64)> = scores.into_iter().collect();
    out.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_in_both_lists_beats_singles() {
        // 2 — второй в BM25 и первый в cosine; 1 и 3 — по одному листу.
        let fused = rrf_fuse(&[1, 2], &[2, 3], RRF_K);
        assert_eq!(fused[0].0, 2, "пересечение каналов должно быть первым");
    }

    #[test]
    fn tie_break_is_deterministic_by_id() {
        // Оба id на rank 1 своего листа → равный скор → меньший id раньше.
        let fused = rrf_fuse(&[7], &[5], RRF_K);
        assert_eq!(fused[0].0, 5);
        assert_eq!(fused[1].0, 7);
        assert!((fused[0].1 - fused[1].1).abs() < 1e-12);
    }

    #[test]
    fn empty_channel_is_identity_of_other() {
        let fused = rrf_fuse(&[4, 2, 9], &[], RRF_K);
        let ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![4, 2, 9], "пустой канал не меняет порядок другого");
    }

    #[test]
    fn both_empty_is_empty() {
        assert!(rrf_fuse(&[], &[], RRF_K).is_empty());
    }

    #[test]
    fn manual_numeric_case_k60() {
        // bm25: [10, 20] → 10: 1/61, 20: 1/62. cosine: [20] → 20: +1/61.
        // 20 = 1/62 + 1/61 > 1/61 = 10 → порядок [20, 10].
        let fused = rrf_fuse(&[10, 20], &[20], 60.0);
        assert_eq!(fused[0].0, 20);
        assert!((fused[0].1 - (1.0 / 62.0 + 1.0 / 61.0)).abs() < 1e-12);
        assert_eq!(fused[1].0, 10);
        assert!((fused[1].1 - 1.0 / 61.0).abs() < 1e-12);
    }

    #[test]
    fn duplicate_id_within_one_list_is_not_double_counted_badly() {
        // Дубликат в одном листе (не должен возникать, но и не ломает):
        // суммирует оба вхождения — порядок остаётся детерминированным.
        let fused = rrf_fuse(&[1, 1], &[], RRF_K);
        assert_eq!(fused.len(), 1);
    }
}
