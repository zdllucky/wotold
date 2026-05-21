//! Voice embeddings foundation (M3.1, #24).
//!
//! # Архитектурное решение
//!
//! Выбран **ONNX Runtime** (`ort` crate) + **WeSpeaker ResNet34-LM** или
//! **ECAPA-TDNN** (256-dim embeddings, Apache-2.0, ~14-30MB ONNX-экспорт).
//!
//! Альтернативы рассмотрены и отклонены:
//! - **Python sidecar (Resemblyzer / pyannote)** — тянет Python runtime,
//!   ломает local-first инвариант, увеличивает install footprint.
//! - **candle crate** — pure Rust но малый каталог pretrained speaker-моделей.
//! - **Swift CoreML** — нет canonical CoreML speaker-embedding моделей в open access.
//!
//! ORT даёт промышленный standard + хороший Rust binding + WeSpeaker
//! зачастую публикует ONNX-экспорт напрямую.
//!
//! # Скоуп этой задачи (#24)
//!
//! Этот модуль закрывает **foundation**:
//! - Трейт `Embedder` для абстракции over backends
//! - `cosine_similarity` — pure math для matching (M3.2)
//! - `embedding_to_bytes` / `bytes_to_embedding` — BLOB serde под
//!   `voice_samples.embedding` (раздел 6.2 паспорта, столбец `float32[]`)
//!
//! Реальный `OnnxEmbedder` + загрузка модели + per-segment audio decode
//! вынесены в **#25 (M3.2-3.4 matching pipeline)** — там embedding впервые
//! нужен для cosine-матчинга против `voice_samples` и слияния с LLM-подсказкой.
//!
//! # Правила хранения (M3.6, O4)
//!
//! - На один контакт хранится N последних качественных семплов (N=5 default, конфиг settings).
//! - `voice_samples.quality REAL` — score качества (SNR / clip count / segment length).
//! - При confirm-привязке кластера к контакту — embedding кластера записывается;
//!   старые отбрасываются если N превышен.
//! - Mic-дорожка (M3.7) → owner contact автоматически, эмбеддинг
//!   из её сегментов идёт в `voice_samples` владельца без UI confirm.

use crate::AppError;

/// Размерность эмбеддинга, ожидаемая от ONNX-модели. WeSpeaker ResNet34
/// и ECAPA-TDNN дают 256-dim вектор. Изменение требует миграции
/// существующих BLOB-семплов.
pub const EMBEDDING_DIM: usize = 256;

/// Абстракция над backend'ом эмбеддингов. Production impl — `OnnxEmbedder`
/// в #25. Test impl — `MockEmbedder` (см. tests).
pub trait Embedder: Send + Sync {
    /// Извлечь embedding из mono PCM f32 samples с указанным sample rate.
    /// Реализация должна сама resample к target rate модели (16kHz для WeSpeaker).
    fn extract(&self, samples: &[f32], sample_rate: u32) -> Result<Vec<f32>, AppError>;
}

/// Косинусная похожесть `[-1.0, 1.0]`. Чем ближе к 1.0, тем больше совпадение.
/// Для одного контакта со многими `voice_samples` берётся max similarity
/// (matching pipeline в M3.2).
///
/// Возвращает 0.0 для разной длины, NaN/Inf, либо если хоть один вектор нулевой —
/// безопасный дефолт «не похожи». Никаких panic на edge cases.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a <= f32::EPSILON || norm_b <= f32::EPSILON {
        return 0.0;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    let sim = dot / denom;
    if sim.is_finite() {
        sim
    } else {
        0.0
    }
}

/// [B3.3] Stub embedder — no-op pre-B3.6. Returns empty Vec → pipeline
/// extract_clusters обнаруживает empty embedding → не persist'ит cluster.
/// Заменяется на `OnnxEmbedder` в B3.6 когда WeSpeaker model bundled.
#[derive(Default)]
pub struct StubEmbedder;

impl Embedder for StubEmbedder {
    fn extract(&self, _samples: &[f32], _sample_rate: u32) -> Result<Vec<f32>, AppError> {
        Ok(Vec::new())
    }
}

/// Сериализовать embedding в little-endian f32 байты для записи в `voice_samples.embedding BLOB`.
pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(embedding.len() * 4);
    for v in embedding {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Десериализовать BLOB → Vec<f32>. Возвращает ошибку если длина не кратна 4.
/// Не проверяет EMBEDDING_DIM — старые семплы с другим dim не должны падать
/// при чтении (#25 решает что делать с mismatch).
pub fn bytes_to_embedding(bytes: &[u8]) -> Result<Vec<f32>, AppError> {
    if bytes.len() % 4 != 0 {
        return Err(AppError::Other(format!(
            "embedding blob length {} not multiple of 4",
            bytes.len()
        )));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    // [B16] Manual array build вместо chunk.try_into().expect() —
    // chunks_exact(4) гарантирует size 4, но try_into даёт unnecessary
    // runtime check. Manual array idiomatic + zero-cost.
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockEmbedder {
        fixed: Vec<f32>,
    }

    impl Embedder for MockEmbedder {
        fn extract(&self, _samples: &[f32], _sample_rate: u32) -> Result<Vec<f32>, AppError> {
            Ok(self.fixed.clone())
        }
    }

    #[test]
    fn cosine_identical_vectors_returns_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_vectors_returns_zero() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_opposite_vectors_returns_minus_one() {
        let a = vec![1.0, 2.0];
        let b = vec![-1.0, -2.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_different_lengths_returns_zero() {
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0, 2.0, 3.0]), 0.0);
    }

    #[test]
    fn cosine_empty_vectors_returns_zero() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn cosine_zero_vector_returns_zero_safely() {
        let zero = vec![0.0, 0.0, 0.0];
        let v = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&zero, &v), 0.0);
        assert_eq!(cosine_similarity(&v, &zero), 0.0);
    }

    #[test]
    fn cosine_handles_nan_safely() {
        let nan_v = vec![f32::NAN, 0.0, 0.0];
        let v = vec![1.0, 0.0, 0.0];
        // NaN из dot/norm → not finite → returns 0.0.
        assert_eq!(cosine_similarity(&nan_v, &v), 0.0);
    }

    #[test]
    fn embedding_blob_roundtrip() {
        let emb = vec![0.1, -0.2, 3.5, 0.0, f32::MIN_POSITIVE];
        let bytes = embedding_to_bytes(&emb);
        assert_eq!(bytes.len(), emb.len() * 4);
        let back = bytes_to_embedding(&bytes).unwrap();
        assert_eq!(emb, back);
    }

    #[test]
    fn empty_embedding_roundtrip() {
        let emb: Vec<f32> = vec![];
        let bytes = embedding_to_bytes(&emb);
        assert!(bytes.is_empty());
        assert_eq!(bytes_to_embedding(&bytes).unwrap(), emb);
    }

    #[test]
    fn bytes_with_wrong_length_returns_error() {
        // 5 байт = не кратно 4.
        let err = bytes_to_embedding(&[1, 2, 3, 4, 5]).unwrap_err();
        assert!(matches!(err, AppError::Other(_)));
    }

    #[test]
    fn embedder_trait_through_mock() {
        let fixed = vec![0.5_f32; EMBEDDING_DIM];
        let e: Box<dyn Embedder> = Box::new(MockEmbedder {
            fixed: fixed.clone(),
        });
        let out = e.extract(&[0.0_f32; 16000], 16000).unwrap();
        assert_eq!(out, fixed);
        assert_eq!(out.len(), EMBEDDING_DIM);
    }
}
