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

/// KV-ключ: id модели, которой посчитаны вектора в `assistant_embeddings`.
pub const SETTING_EMBED_MODEL_ID: &str = "assistant.embed_model_id";

/// Актуальный id модели эмбеддера в каталоге.
pub fn current_embed_model_id() -> &'static str {
    crate::local_engine::models::ModelId::E5_SMALL_QINT8.as_str()
}

/// [M15.10.3] Инвалидация при смене модели эмбеддера: вектора разных моделей
/// несравнимы (даже при равном dim), поэтому сохранённый id != текущему →
/// полный `DELETE FROM assistant_embeddings`, backfill пересчитает новой
/// моделью. Идемпотентно; вызывается на старте перед embed-backfill'ом.
///
/// Инвариант (rust-review Ph2): на query-путь этот вызов НЕ ставится
/// намеренно — смена модели всегда означает новый catalog id → новый файл
/// на диске, до его скачивания `try_load` = None и гибрид не активен;
/// после скачивания старт-ап backfill приводит вектора в порядок. Если
/// когда-то каталожная запись начнёт переиспользовать id при смене
/// артефакта — этот вызов обязан появиться и на query-пути.
pub async fn ensure_embed_model_current(pool: &sqlx::SqlitePool) -> Result<(), AppError> {
    let current = current_embed_model_id();
    let stored = crate::db::get_setting(pool, SETTING_EMBED_MODEL_ID).await?;
    if stored.as_deref() == Some(current) {
        return Ok(());
    }
    if let Some(old) = &stored {
        log::info!("assistant embedder model changed ({old} -> {current}): clearing vectors");
    }
    crate::db::assistant_embeddings::clear_embeddings(pool).await?;
    crate::db::set_setting(pool, SETTING_EMBED_MODEL_ID, current).await?;
    Ok(())
}

/// Процессный shared-эмбеддер. `Some` кэшируется навсегда (сессия ~120MB,
/// выгрузка не нужна); `None` перепроверяется на каждом вызове — юзер может
/// докачать модель без рестарта, а перепроверка = два stat'а
/// (`check_status_fast`). Мьютекс держится на время загрузки (~250мс один
/// раз) — параллельные вызовы не грузят модель дважды.
static SHARED_EMBEDDER: std::sync::OnceLock<tokio::sync::Mutex<Option<Arc<dyn TextEmbedder>>>> =
    std::sync::OnceLock::new();

pub async fn shared(app_data_dir: &Path) -> Option<Arc<dyn TextEmbedder>> {
    let slot = SHARED_EMBEDDER.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut guard = slot.lock().await;
    if let Some(e) = guard.as_ref() {
        return Some(e.clone());
    }
    let loaded = try_load_embedder(app_data_dir).await?;
    *guard = Some(loaded.clone());
    Some(loaded)
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

/// [M15.12] Реэкспорт загрузчика для eval-harness (реальная модель из env).
#[cfg(all(test, feature = "assistant-embed"))]
pub(crate) use onnx::load_from_dir as onnx_load_from_dir;

#[cfg(feature = "assistant-embed")]
mod onnx {
    //! OnnxTextEmbedder — multilingual-e5-small qint8 через ort + tokenizers.
    //! Порт валидированного спайка M15.9: mean-pooling по attention mask +
    //! L2; inputs Xenova/intfloat-экспорта: input_ids + attention_mask +
    //! token_type_ids (нулями). ~5мс короткий пассаж / ~90мс 350-ток на
    //! M1 Pro — вызывающие оборачивают в spawn_blocking, отдельный слот
    //! resource_queue не нужен (замер спайка).

    use std::path::PathBuf;
    use std::sync::Mutex;

    use ndarray::{Axis, Ix3};
    use ort::session::{builder::GraphOptimizationLevel, Session};
    use ort::value::TensorRef;
    use tokenizers::Tokenizer;

    use super::*;
    use crate::local_engine::models::{check_status_fast, model_path, ModelId, ModelStatus};

    /// Лимит контекста e5 (512 позиций XLM-R, минус спецтокены).
    const MAX_TOKENS: usize = 512;

    pub(super) struct OnnxTextEmbedder {
        /// `Session::run` требует `&mut` — трейт даёт `&self`, сериализуем
        /// инференс мьютексом (один вызов ~5-90мс, конкуренции почти нет).
        session: Mutex<Session>,
        tokenizer: Tokenizer,
        needs_token_type_ids: bool,
    }

    impl OnnxTextEmbedder {
        /// Загрузка из явных путей — используется и продовым `try_load`
        /// (пути каталога), и `#[ignore]` reference-тестом (env-директория).
        pub(super) fn load_from_paths(
            model: &PathBuf,
            tokenizer_json: &PathBuf,
        ) -> Result<Self, AppError> {
            // `ort::Error<T>` в rc.12 генерик (возвращает builder при
            // ошибке) — замыкание на каждый шаг, единый helper не типизируется.
            let session = Session::builder()
                .map_err(|e| AppError::Other(format!("embedder session: {e}")))?
                .with_optimization_level(GraphOptimizationLevel::Level1)
                .map_err(|e| AppError::Other(format!("embedder session: {e}")))?
                .with_intra_threads(2)
                .map_err(|e| AppError::Other(format!("embedder session: {e}")))?
                .commit_from_file(model)
                .map_err(|e| AppError::Other(format!("embedder session: {e}")))?;
            let mut tokenizer = Tokenizer::from_file(tokenizer_json)
                .map_err(|e| AppError::Other(format!("embedder tokenizer: {e}")))?;
            tokenizer
                .with_truncation(Some(tokenizers::TruncationParams {
                    max_length: MAX_TOKENS,
                    ..Default::default()
                }))
                .map_err(|e| AppError::Other(format!("embedder truncation: {e}")))?;
            let needs_token_type_ids = session
                .inputs()
                .iter()
                .any(|i| i.name() == "token_type_ids");
            Ok(Self {
                session: Mutex::new(session),
                tokenizer,
                needs_token_type_ids,
            })
        }

        fn embed_one(&self, prefixed_text: &str) -> Result<Vec<f32>, AppError> {
            let enc = self
                .tokenizer
                .encode(prefixed_text, true)
                .map_err(|e| AppError::Other(format!("embedder encode: {e}")))?;
            let ids: Vec<i64> = enc.get_ids().iter().map(|&i| i64::from(i)).collect();
            let mask: Vec<i64> = enc
                .get_attention_mask()
                .iter()
                .map(|&i| i64::from(i))
                .collect();
            let len = ids.len();
            if len == 0 {
                return Ok(vec![0.0; EMBED_DIM]);
            }

            let mut session = self
                .session
                .lock()
                .map_err(|_| AppError::Other("embedder mutex poisoned".into()))?;

            let ids_t = TensorRef::from_array_view(([1usize, len], &*ids))
                .map_err(|e| AppError::Other(format!("embedder tensor: {e}")))?;
            let mask_t = TensorRef::from_array_view(([1usize, len], &*mask))
                .map_err(|e| AppError::Other(format!("embedder tensor: {e}")))?;
            let tt: Vec<i64>;
            let outputs = if self.needs_token_type_ids {
                tt = vec![0; len];
                let tt_t = TensorRef::from_array_view(([1usize, len], &*tt))
                    .map_err(|e| AppError::Other(format!("embedder tensor: {e}")))?;
                session.run(ort::inputs![
                    "input_ids" => ids_t,
                    "attention_mask" => mask_t,
                    "token_type_ids" => tt_t
                ])
            } else {
                session.run(ort::inputs![
                    "input_ids" => ids_t,
                    "attention_mask" => mask_t
                ])
            }
            .map_err(|e| AppError::Other(format!("embedder run: {e}")))?;

            let hidden = outputs["last_hidden_state"]
                .try_extract_array::<f32>()
                .map_err(|e| AppError::Other(format!("embedder output: {e}")))?
                .into_dimensionality::<Ix3>()
                .map_err(|e| AppError::Other(format!("embedder output shape: {e}")))?;
            let dim = hidden.shape()[2];

            // Mean-pooling по attention mask.
            let mut sum = vec![0f32; dim];
            let mut count = 0f32;
            for (t, row) in hidden.index_axis(Axis(0), 0).axis_iter(Axis(0)).enumerate() {
                if mask.get(t).copied() == Some(1) {
                    for (d, v) in row.iter().enumerate() {
                        sum[d] += v;
                    }
                    count += 1.0;
                }
            }
            for v in sum.iter_mut() {
                *v /= count.max(1.0);
            }
            l2_normalize(&mut sum);
            Ok(sum)
        }
    }

    impl TextEmbedder for OnnxTextEmbedder {
        fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, AppError> {
            texts
                .iter()
                .map(|t| self.embed_one(&format!("{PASSAGE_PREFIX}{t}")))
                .collect()
        }

        fn embed_query(&self, question: &str) -> Result<Vec<f32>, AppError> {
            self.embed_one(&format!("{QUERY_PREFIX}{question}"))
        }

        fn dim(&self) -> usize {
            EMBED_DIM
        }
    }

    /// [M15.12] Загрузка из директории с каноническими именами файлов
    /// (`model.onnx` + `tokenizer.json`) — вход eval-harness'а
    /// (env `WOTOLD_EVAL_MODEL_DIR`), минуя каталог.
    #[cfg(test)]
    pub(crate) fn load_from_dir(dir: &Path) -> Result<Arc<dyn TextEmbedder>, AppError> {
        let e = OnnxTextEmbedder::load_from_paths(
            &dir.join("model.onnx"),
            &dir.join("tokenizer.json"),
        )?;
        Ok(Arc::new(e))
    }

    pub(super) async fn try_load(app_data_dir: &Path) -> Option<Arc<dyn TextEmbedder>> {
        for id in [ModelId::E5_SMALL_QINT8, ModelId::E5_TOKENIZER] {
            match check_status_fast(app_data_dir, id.as_str()).await {
                Ok(ModelStatus::Present { .. }) => {}
                Ok(_) => return None, // Absent/Corrupted — тихий BM25-fallback
                Err(e) => {
                    log::warn!("assistant embedder status check: {e}");
                    return None;
                }
            }
        }
        let model = model_path(app_data_dir, ModelId::E5_SMALL_QINT8.as_str());
        let tokenizer = model_path(app_data_dir, ModelId::E5_TOKENIZER.as_str());
        // Session build ~230мс + чтение 118MB — вне async-потока.
        let loaded = tokio::task::spawn_blocking(move || {
            OnnxTextEmbedder::load_from_paths(&model, &tokenizer)
        })
        .await;
        match loaded {
            Ok(Ok(e)) => Some(Arc::new(e)),
            Ok(Err(e)) => {
                log::warn!("assistant embedder load failed: {e}");
                None
            }
            Err(e) => {
                log::warn!("assistant embedder load join: {e}");
                None
            }
        }
    }

    #[cfg(test)]
    mod reference_tests {
        //! Reference-тест на реальной модели (образец B3.7d). Запуск:
        //! `WOTOLD_E5_MODEL_DIR=<dir> cargo test --features assistant-embed \
        //!  -- --ignored embedder`
        //! где <dir> содержит `model.onnx` + `tokenizer.json` (артефакты
        //! intfloat/multilingual-e5-small: onnx/model_qint8_avx512_vnni.onnx
        //! + onnx/tokenizer.json).

        use super::*;

        fn cos(a: &[f32], b: &[f32]) -> f32 {
            a.iter().zip(b).map(|(x, y)| x * y).sum()
        }

        #[test]
        #[ignore = "требует скачанную e5-модель: env WOTOLD_E5_MODEL_DIR"]
        fn reference_embedding_on_real_model() {
            let dir = PathBuf::from(
                std::env::var("WOTOLD_E5_MODEL_DIR").expect("WOTOLD_E5_MODEL_DIR не задан"),
            );
            let emb = OnnxTextEmbedder::load_from_paths(
                &dir.join("model.onnx"),
                &dir.join("tokenizer.json"),
            )
            .expect("load");

            // 1. Размерность + L2-норма.
            let q = emb.embed_query("какие сроки по контракту?").unwrap();
            assert_eq!(q.len(), EMBED_DIM);
            let n: f32 = q.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((n - 1.0).abs() < 1e-4, "norm = {n}");

            // 2. Reference fingerprint (спайк M15.9, intfloat qint8,
            //    Level1/intra=2): первые 8 компонент passage-вектора.
            //    Допуск 5e-3 — устойчивость к минорным версиям ORT.
            let p = &emb
                .embed_passages(&["Договорились хранить записи звонков локально, без облака."])
                .unwrap()[0];
            let reference = [
                0.013125232f32,
                -0.009480266,
                -0.029063327,
                -0.049545348,
                0.05303151,
                -0.037857212,
                -0.029183423,
                0.027741378,
            ];
            for (i, (got, want)) in p.iter().zip(reference.iter()).enumerate() {
                assert!((got - want).abs() < 5e-3, "dim {i}: got {got}, want {want}");
            }

            // 3. Семантика: синонимный пассаж (дедлайн↔сроки) обязан бить
            //    нерелевантный — кейс, который BM25 не ловит.
            let ps = emb
                .embed_passages(&[
                    "Дедлайн подписания договора — 30 мая, Иван пришлёт SOW в пятницу.",
                    "Обсудили дизайн лендинга и цветовую палитру бренда.",
                ])
                .unwrap();
            assert!(
                cos(&q, &ps[0]) > cos(&q, &ps[1]),
                "синонимный пассаж должен ранжироваться выше нерелевантного"
            );

            // 4. Кросс-язычность: en-перевод ближе к ru-оригиналу, чем
            //    ru-нерелевантный текст.
            let pair = emb
                .embed_passages(&[
                    "Договорились хранить записи звонков локально, без облака.",
                    "We agreed to keep call recordings locally, without any cloud.",
                    "Команда переезжает в новый офис в сентябре.",
                ])
                .unwrap();
            assert!(cos(&pair[0], &pair[1]) > cos(&pair[0], &pair[2]));
        }
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

    #[tokio::test]
    async fn ensure_embed_model_clears_vectors_on_model_change() {
        use crate::assistant::types::AssistantPassageKind;
        use crate::db::assistant::{replace_call_passages, PassageInput};
        use crate::db::assistant_embeddings;
        use crate::db::test_support::fresh_db;

        let db = fresh_db().await;
        sqlx::query(
            "INSERT INTO calls (id, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES ('c1', CURRENT_TIMESTAMP, 60, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        replace_call_passages(
            &db.pool,
            "c1",
            &[PassageInput {
                kind: AssistantPassageKind::Transcript,
                speaker: None,
                start_ms: Some(0),
                end_ms: Some(1000),
                text: "текст".into(),
                token_est: 2,
            }],
        )
        .await
        .unwrap();
        let ids = assistant_embeddings::list_call_passage_texts(&db.pool, "c1")
            .await
            .unwrap();
        assistant_embeddings::upsert_embeddings(
            &db.pool,
            1,
            &[(ids[0].0, crate::embeddings::embedding_to_bytes(&[1.0]))],
        )
        .await
        .unwrap();

        // Вектора «чужой» модели: сохранённый id отличается → clear + set.
        crate::db::set_setting(&db.pool, SETTING_EMBED_MODEL_ID, "old-model")
            .await
            .unwrap();
        ensure_embed_model_current(&db.pool).await.unwrap();
        let stamp = assistant_embeddings::embedding_stamp(&db.pool)
            .await
            .unwrap();
        assert_eq!(stamp.embedding_count, 0, "вектора старой модели снесены");
        assert_eq!(
            crate::db::get_setting(&db.pool, SETTING_EMBED_MODEL_ID)
                .await
                .unwrap()
                .as_deref(),
            Some(current_embed_model_id())
        );

        // Повторный вызов с совпадающим id — no-op (вектора не трогаются).
        assistant_embeddings::upsert_embeddings(
            &db.pool,
            1,
            &[(ids[0].0, crate::embeddings::embedding_to_bytes(&[1.0]))],
        )
        .await
        .unwrap();
        ensure_embed_model_current(&db.pool).await.unwrap();
        let stamp = assistant_embeddings::embedding_stamp(&db.pool)
            .await
            .unwrap();
        assert_eq!(stamp.embedding_count, 1, "актуальная модель — вектора целы");
    }
}
