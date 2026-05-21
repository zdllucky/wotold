//! [B3.7] Real ONNX Embedder для WeSpeaker / ECAPA-TDNN моделей.
//!
//! Скоуп этого модуля (B3.7a — scaffold):
//!   1. `OnnxEmbedder` struct + load() из path → возвращает Result
//!   2. `extract()` — preprocessing (fbank) + inference + L2 normalize
//!   3. `Embedder` trait impl
//!
//! Подключается через `embeddings::try_load_onnx_embedder` который под
//! `#[cfg(feature = "voice-onnx")]` импортирует этот модуль и пробует
//! load(`$APP_DATA/models/embedder.onnx`).
//!
//! # Текущее состояние (B3.7a)
//!
//! Scaffold: модуль компилируется под фичей, `OnnxEmbedder::load()` грузит
//! ONNX session, но `extract()` возвращает Err("fbank preprocessing not
//! implemented") — preprocessing будет реализован в B3.7b с unit-тестами
//! против reference Python output. Pipeline'у безопасно: `try_load_onnx_embedder`
//! при Err'е fallback'ит на StubEmbedder (см. embeddings.rs:: dispatcher).
//!
//! # B3.7b — Kaldi mel-fbank
//!
//! WeSpeaker и ECAPA-TDNN ожидают 80-dim mel-fbank features:
//!   - 25ms window, 10ms hop (16kHz sample rate)
//!   - 80 mel bins
//!   - Cepstral mean normalization (CMN) per-utterance
//!   - log10
//!
//! Без exact replication → embeddings garbage. Unit-тесты должны сравнить
//! с reference от torchaudio.compliance.kaldi.fbank() для зашитого WAV.
//!
//! # B3.7c — Bundling decision
//!
//! Model bundled в DMG vs runtime download — отдельный design call.
//! Bundled: +26MB к .dmg, но offline-first per паспорт R8.
//! Runtime: малый bundle, нужен fetch на первом запуске + checksum.
//! Скорее всего runtime download — bundled .dmg > 100MB требует Apple
//! notarization tier (R6, не сделано в MVP).

use std::path::Path;

use ndarray::Array;
use ort::session::Session;

use crate::embeddings::{Embedder, EMBEDDING_DIM};
use crate::AppError;

/// Production ONNX embedder. Wraps ort::Session + хранит config константы
/// модели (input/output names, sample rate, fbank params).
///
/// Создаётся через `OnnxEmbedder::load(model_path)`. Thread-safe (`Send + Sync`)
/// потому что ort::Session — `Sync`.
///
/// `#[allow(dead_code)]` на полях — B3.7a scaffold не использует их в
/// extract() (preprocessing TODO). Снимется в B3.7b когда fbank + inference
/// landed.
#[allow(dead_code)]
pub struct OnnxEmbedder {
    session: Session,
    /// Имя input tensor'а в модели — WeSpeaker обычно "feats" или "input".
    /// Захардкоженно под наш bundled checkpoint; если model file other —
    /// load() прочитает из session.inputs() и сравнит.
    input_name: String,
    /// Имя output tensor'а — обычно "embed" / "embedding" / "output".
    output_name: String,
}

impl OnnxEmbedder {
    /// Загрузить модель с диска. Возвращает Err если:
    ///   - файл отсутствует / нечитаемый
    ///   - не валидный ONNX format
    ///   - модель не имеет ровно 1 input + 1 output (sanity check для
    ///     WeSpeaker / ECAPA — обе семейства single-input single-output)
    pub fn load(model_path: &Path) -> Result<Self, AppError> {
        if !model_path.exists() {
            return Err(AppError::Other(format!(
                "ONNX model not found: {}",
                model_path.display()
            )));
        }

        // ort 2.x: SessionBuilder::new() → commit_from_file().
        // По умолчанию single-threaded CPU EP — это и нужно для speaker
        // embedding inference (модель ~26MB, single inference 50-100ms).
        let session = Session::builder()
            .map_err(|e| AppError::Other(format!("ort session builder: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| {
                AppError::Other(format!("load ONNX model {}: {e}", model_path.display()))
            })?;

        // Sanity check: WeSpeaker/ECAPA — 1 input + 1 output.
        if session.inputs.len() != 1 {
            return Err(AppError::Other(format!(
                "expected 1 input tensor, model has {}",
                session.inputs.len()
            )));
        }
        if session.outputs.len() != 1 {
            return Err(AppError::Other(format!(
                "expected 1 output tensor, model has {}",
                session.outputs.len()
            )));
        }

        let input_name = session.inputs[0].name.clone();
        let output_name = session.outputs[0].name.clone();
        log::info!(
            "OnnxEmbedder loaded: {} → input={input_name}, output={output_name}",
            model_path.display()
        );

        Ok(Self {
            session,
            input_name,
            output_name,
        })
    }
}

impl Embedder for OnnxEmbedder {
    fn extract(&self, _samples: &[f32], _sample_rate: u32) -> Result<Vec<f32>, AppError> {
        // [B3.7b TODO] Kaldi-style mel-fbank preprocessing + ONNX inference.
        //
        // Flow:
        //   1. Resample to 16kHz if needed (sample_rate != 16000) — sidecar
        //      уже пишет 16kHz, sanity-check + skip обычно.
        //   2. Compute 80-dim log-mel fbank features:
        //      - frame_length=25ms (400 samples @ 16kHz)
        //      - hop=10ms (160 samples)
        //      - n_mels=80
        //      - power-spectrum → mel filterbank → log
        //   3. CMN: subtract per-utterance mean from each feature.
        //   4. Transpose to (T, 80) ndarray + add batch dim → (1, T, 80).
        //   5. Call session.run() с input_name → Vec<f32> output (256-dim).
        //   6. L2 normalize → return.
        //
        // Без reference test'а против torchaudio.compliance.kaldi.fbank()
        // — embeddings garbage. B3.7b landed когда reference baked в test
        // fixtures.
        //
        // [Note] Сейчас не блокирует pipeline: try_load_onnx_embedder()
        // вернёт Some(this), pipeline вызовет extract() → Err → cluster
        // skip'ается, fallback на пустые clusters (как со StubEmbedder).
        let _ = self.input_name.as_str();
        let _ = self.output_name.as_str();
        let _: Result<Array<f32, _>, _> = Array::from_shape_vec((1, 1, 80), vec![0.0]);
        Err(AppError::Other(
            "OnnxEmbedder.extract: fbank preprocessing not implemented yet (B3.7b)".to_string(),
        ))
    }
}

/// Compile-time check: EMBEDDING_DIM из embeddings.rs (256) совпадает с
/// тем, что выдаёт WeSpeaker ResNet34 / ECAPA-TDNN. Если меняешь модель
/// на другую dim — обнови `EMBEDDING_DIM` и миграцию `0005_voice_samples_embedding_dim.sql`.
#[allow(dead_code)]
const _ASSERT_DIM: () = {
    assert!(EMBEDDING_DIM == 256, "WeSpeaker/ECAPA both produce 256-dim");
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_err_when_model_missing() {
        let path = std::path::PathBuf::from("/nonexistent/model.onnx");
        // OnnxEmbedder не impl Debug (ort::Session не Debug), unwrap_err()
        // не работает — используем match напрямую.
        match OnnxEmbedder::load(&path) {
            Ok(_) => panic!("expected Err on missing model"),
            Err(AppError::Other(msg)) => assert!(msg.contains("not found")),
            Err(e) => panic!("expected AppError::Other, got {e:?}"),
        }
    }

    // B3.7b: integration test против reference Python torchaudio.kaldi.fbank()
    // — добавится когда preprocessing landed.
}
