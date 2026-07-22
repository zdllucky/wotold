//! [M15.9] Текст-эмбеддер ассистента — семантический канал retrieval Ph2.
//!
//! Архитектура (план Ph2): трейт `TextEmbedder` живёт ВНЕ feature-гейта,
//! `assistant-embed` включает только ONNX-реализацию (ort + tokenizers,
//! multilingual-e5-small qint8 из MODEL_CATALOG). Нет feature или файлов
//! модели → `try_load_embedder` = `None` → retrieval деградирует до чистого
//! BM25 (PRD §6.3). Все даунстрим-тесты (indexer / cache / fusion) работают
//! на `MockEmbedder` без модели и feature.
//!
//! Префиксы `query:` / `passage:` обязательны для качества e5 (в PRD не
//! зафиксированы — решение спайка M15.9) и инкапсулированы здесь: вызывающий
//! передаёт сырой текст.

// Публичный API модуля подключается в M15.10 (embed-hook индексера) и
// M15.11 (векторный канал retrieval) — до тех пор dead_code allow, как
// делал types.rs в Ph1. Снять при врезке.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use crate::AppError;

/// Префикс запроса (e5, asymmetric retrieval).
pub const QUERY_PREFIX: &str = "query: ";
/// Префикс пассажа (e5).
pub const PASSAGE_PREFIX: &str = "passage: ";
/// Размерность multilingual-e5-small (подтверждено спайком M15.9).
pub const EMBED_DIM: usize = 384;

/// Провайдер текстовых эмбеддингов. Возвращает L2-нормализованные вектора —
/// cosine downstream считается как dot.
pub trait TextEmbedder: Send + Sync {
    /// Вектора пассажей, порядок соответствует входу. Префикс `passage:`
    /// добавляется внутри.
    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, AppError>;
    /// Вектор вопроса. Префикс `query:` добавляется внутри.
    fn embed_query(&self, question: &str) -> Result<Vec<f32>, AppError>;
    /// Размерность выходных векторов.
    fn dim(&self) -> usize;
}

/// L2-нормализация на месте. Нулевой вектор остаётся нулевым (guard 1e-12).
pub(crate) fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Загрузить ONNX-эмбеддер, если собран feature `assistant-embed` и обе
/// записи каталога (модель + tokenizer) скачаны. Иначе `None` — вызывающий
/// обязан деградировать до BM25, не ошибаться.
pub async fn try_load_embedder(app_data_dir: &Path) -> Option<Arc<dyn TextEmbedder>> {
    #[cfg(feature = "assistant-embed")]
    {
        onnx::try_load(app_data_dir).await
    }
    #[cfg(not(feature = "assistant-embed"))]
    {
        let _ = app_data_dir;
        None
    }
}

#[cfg(feature = "assistant-embed")]
mod onnx {
    // [M15.9 шаг 9.3] OnnxTextEmbedder — приходит следующим коммитом.
    use super::*;

    pub(super) async fn try_load(app_data_dir: &Path) -> Option<Arc<dyn TextEmbedder>> {
        let _ = app_data_dir;
        None
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! `MockEmbedder` — детерминированные hash-вектора для тестов indexer /
    //! cache / retrieval без реальной модели. Одинаковый текст → одинаковый
    //! вектор; префиксы применяются как в реальной реализации, поэтому
    //! query- и passage-вектор одного текста различаются.

    use super::*;

    pub struct MockEmbedder;

    /// FNV-1a → xorshift64: детерминированный псевдослучайный вектор.
    fn mock_vec(seed_text: &str) -> Vec<f32> {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in seed_text.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        let mut x = h.max(1);
        let mut v: Vec<f32> = (0..EMBED_DIM)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x as f64 / u64::MAX as f64) as f32 - 0.5
            })
            .collect();
        l2_normalize(&mut v);
        v
    }

    impl TextEmbedder for MockEmbedder {
        fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, AppError> {
            Ok(texts
                .iter()
                .map(|t| mock_vec(&format!("{PASSAGE_PREFIX}{t}")))
                .collect())
        }

        fn embed_query(&self, question: &str) -> Result<Vec<f32>, AppError> {
            Ok(mock_vec(&format!("{QUERY_PREFIX}{question}")))
        }

        fn dim(&self) -> usize {
            EMBED_DIM
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockEmbedder;
    use super::*;

    fn norm(v: &[f32]) -> f32 {
        v.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    #[test]
    fn query_and_passage_vectors_differ_for_same_text() {
        let m = MockEmbedder;
        let q = m.embed_query("что решили по срокам").unwrap();
        let p = &m.embed_passages(&["что решили по срокам"]).unwrap()[0];
        assert_ne!(&q, p, "префиксы query:/passage: обязаны менять вектор");
    }

    #[test]
    fn batch_preserves_order_and_is_deterministic() {
        let m = MockEmbedder;
        let batch = m.embed_passages(&["a", "b", "a"]).unwrap();
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0], batch[2], "одинаковый текст → одинаковый вектор");
        assert_ne!(batch[0], batch[1]);
        assert_eq!(batch[0].len(), EMBED_DIM);
        assert_eq!(m.dim(), EMBED_DIM);
    }

    #[test]
    fn vectors_are_l2_normalized() {
        let m = MockEmbedder;
        let q = m.embed_query("нормализация").unwrap();
        assert!((norm(&q) - 1.0).abs() < 1e-5, "norm = {}", norm(&q));
    }

    #[test]
    fn l2_normalize_keeps_zero_vector() {
        let mut v = vec![0.0f32; 4];
        l2_normalize(&mut v);
        assert_eq!(v, vec![0.0; 4], "нулевой вектор не должен дать NaN");
    }

    #[tokio::test]
    async fn try_load_returns_none_without_model_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(try_load_embedder(dir.path()).await.is_none());
    }
}
