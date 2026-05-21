//! [B3.7] Real ONNX Embedder через официальный `sherpa-onnx` Rust crate.
//!
//! Wrapping `SpeakerEmbeddingExtractor` от k2-fsa — полный pipeline:
//!   1. Kaldi-style mel-fbank preprocessing (внутри C++ lib sherpa-onnx)
//!   2. ONNX inference через bundled ONNX Runtime
//!   3. L2 normalization (мы делаем defensive normalize поверх, на случай
//!      если конкретная модель не нормализует output сама)
//!
//! Альтернативы (отклонены в research):
//! - `ort` + `ndarray` + manual Kaldi fbank — больше control но риск
//!   implementation drift от reference → garbage embeddings.
//! - `sherpa-rs` — deprecated, upstream рекомендует прямой sherpa-onnx crate.
//! - Поиск ECAPA с fbank-in-graph — таких production моделей нет
//!   (WeSpeaker/3D-Speaker/NeMo все ожидают pre-computed features).
//!
//! # Подключение
//!
//! Под `#[cfg(feature = "voice-onnx")]`. Default build не тянет
//! sherpa-onnx + ONNX Runtime native libs. Production-сборка включает.
//!
//! # Совместимые модели
//!
//! Sherpa-onnx releases (<https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models>):
//!   - `wespeaker_en_voxceleb_resnet34_LM.onnx` — 25MB, English, 256-dim
//!   - `wespeaker_zh_cnceleb_resnet34_LM.onnx` — 25MB, Chinese, 256-dim
//!   - `3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx` — 25MB
//!   - `nemo_en_titanet_small.onnx` — 40MB
//!
//! Все ожидают 16kHz mono PCM (что Swift sidecar и пишет).
//!
//! # Runtime download (B3.7c)
//!
//! Модель НЕ bundled — runtime download через https + SHA256 check на первой
//! записи. Кэш в `$APP_DATA/models/embedder.onnx`. UI: «Скачиваем модель
//! распознавания голоса (25MB)...» splash.

use std::path::Path;

use sherpa_onnx::{SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig};

use crate::embeddings::{Embedder, EMBEDDING_DIM};
use crate::AppError;

/// Production ONNX embedder. Wraps `SpeakerEmbeddingExtractor` от sherpa-onnx.
/// Thread-safe (`Send + Sync`) потому что underlying C++ extractor — `Sync`.
pub struct OnnxEmbedder {
    extractor: SpeakerEmbeddingExtractor,
}

impl OnnxEmbedder {
    /// Загрузить модель с диска. Возвращает Err если:
    ///   - файл отсутствует
    ///   - не валидный ONNX format / не совместим с SpeakerEmbeddingExtractor
    ///   - native lib init failure
    pub fn load(model_path: &Path) -> Result<Self, AppError> {
        if !model_path.exists() {
            return Err(AppError::Other(format!(
                "ONNX model not found: {}",
                model_path.display()
            )));
        }

        let config = SpeakerEmbeddingExtractorConfig {
            model: Some(model_path.to_string_lossy().into_owned()),
            // CPU EP только. GPU EP (cuda/metal) — overkill для одной
            // inference per call (~50-100ms) + добавляет binary weight.
            num_threads: 1,
            debug: false,
            provider: Some("cpu".into()),
        };
        // sherpa-onnx API возвращает Option<_> при create/stream/compute —
        // None означает failure (model load fail, OOM, и т.д.). Конкретного
        // error message нет в Rust API (логи sherpa-onnx идут в stderr).
        let extractor = SpeakerEmbeddingExtractor::create(&config).ok_or_else(|| {
            AppError::Other(format!(
                "SpeakerEmbeddingExtractor::create failed for {} — \
                 проверь совместимость модели с sherpa-onnx + stderr",
                model_path.display()
            ))
        })?;

        // Sanity check: модель выдаёт ожидаемое 256-dim embedding.
        let dim = extractor.dim();
        if dim as usize != EMBEDDING_DIM {
            return Err(AppError::Other(format!(
                "model dim {dim} != EMBEDDING_DIM {EMBEDDING_DIM} — несовместимая модель"
            )));
        }
        log::info!("OnnxEmbedder loaded: {} (dim={dim})", model_path.display());

        Ok(Self { extractor })
    }
}

impl Embedder for OnnxEmbedder {
    fn extract(&self, samples: &[f32], sample_rate: u32) -> Result<Vec<f32>, AppError> {
        if samples.is_empty() {
            return Err(AppError::Other("empty samples".into()));
        }
        // Sidecar пишет 16kHz mono — sherpa-onnx сам resample'нет если нужно.
        let stream = self.extractor.create_stream().ok_or_else(|| {
            AppError::Other("SpeakerEmbeddingExtractor::create_stream failed".into())
        })?;
        stream.accept_waveform(sample_rate as i32, samples);
        stream.input_finished();

        if !self.extractor.is_ready(&stream) {
            // Слишком короткий segment (< чем минимум фреймов для статистики
            // pooling). cluster pipeline filter'ит < 0.5s — этого должно
            // хватать, но если модель требует > 0.5s — Err.
            return Err(AppError::Other(
                "audio segment too short for embedding extraction".into(),
            ));
        }

        let mut embedding = self
            .extractor
            .compute(&stream)
            .ok_or_else(|| AppError::Other("SpeakerEmbeddingExtractor::compute failed".into()))?;

        // L2 normalize defensively. WeSpeaker LM-models обычно нормализуют,
        // но cluster pipeline всё равно делает L2 после mean-pool — здесь
        // дополнительная страховка для consistency cosine matching.
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > f32::EPSILON {
            for x in embedding.iter_mut() {
                *x /= norm;
            }
        } else {
            return Err(AppError::Other(
                "zero-norm embedding — model output bug".into(),
            ));
        }

        Ok(embedding)
    }
}

/// Compile-time check: EMBEDDING_DIM (256) совпадает с WeSpeaker/3D-Speaker.
/// Если меняешь на NeMo TitaNet (192-dim) или иное — обнови EMBEDDING_DIM
/// в embeddings.rs + миграцию `0005_voice_samples_embedding_dim.sql`.
#[allow(dead_code)]
const _ASSERT_DIM: () = {
    assert!(EMBEDDING_DIM == 256, "WeSpeaker/3D-Speaker → 256-dim");
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_err_when_model_missing() {
        let path = std::path::PathBuf::from("/nonexistent/model.onnx");
        // SpeakerEmbeddingExtractor не impl Debug, unwrap_err() не работает.
        match OnnxEmbedder::load(&path) {
            Ok(_) => panic!("expected Err on missing model"),
            Err(AppError::Other(msg)) => assert!(msg.contains("not found")),
            Err(e) => panic!("expected AppError::Other, got {e:?}"),
        }
    }

    // B3.7c: integration test против reference embedding для известного WAV
    // (fangjun-sr-1.wav из sherpa-onnx test fixtures). Requires model file —
    // запускается только под `--features voice-onnx` + WOTOLD_TEST_MODEL_PATH env.
}
