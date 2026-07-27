//! [M12.4] Каталог локальных моделей: id, URL, SHA256, размеры.
//!
//! [TD-41] Выделено из `local_engine/models.rs` (943 строки при лимите 800,
//! правило 8) вместе с тестами каталога. Здесь только данные и их разбор —
//! скачивание, проверка на диске и учёт использования остались в `models.rs`.
//!
//! # Контракт безопасности (W5, PRD M12.4.6)
//!
//! SHA256 в этих записях — единственная защита от подмены release-файла на
//! HuggingFace. Правится каталог только через
//! [`scripts/refresh-model-catalog.sh`](../../../../../scripts/refresh-model-catalog.sh);
//! руками хэши не подставлять.

use serde::Serialize;

/// Тип модели в каталоге — STT (Whisper) / LLM (GGUF) / Diarization
/// (pyannote segmentation .onnx для sherpa-onnx OfflineSpeakerDiarization) /
/// Embedding ([M15.9] текст-эмбеддер RAG-ассистента, e5-small ONNX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Stt,
    Llm,
    Diarization,
    Embedding,
}

/// Стабильный id записи в каталоге. Newtype-обёртка чтобы не путать со
/// строковыми ключами settings. См. PRD §M12.4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ModelId(pub &'static str);

impl ModelId {
    pub const WHISPER_SMALL: ModelId = ModelId("whisper-small");
    pub const WHISPER_MEDIUM: ModelId = ModelId("whisper-medium");
    pub const WHISPER_LARGE_V3: ModelId = ModelId("whisper-large-v3");
    /// [M14 T-16 P2] Draft модель для speculative decoding (Quality preset).
    /// При SUMMARY_SPECULATIVE_DECODING=1 и preset=Quality, llama-cli
    /// получает `--model-draft <path>` указывающий на этот файл.
    pub const QWEN25_0_5B: ModelId = ModelId("qwen25-0_5b");
    pub const QWEN25_1_5B: ModelId = ModelId("qwen25-1_5b");
    pub const QWEN25_3B: ModelId = ModelId("qwen25-3b");
    pub const QWEN25_7B: ModelId = ModelId("qwen25-7b");
    /// [M12-D5] Pyannote segmentation 3.0 для sherpa-onnx
    /// OfflineSpeakerDiarization. Shared across all 3 presets (~6 MB).
    pub const PYANNOTE_SEGMENTATION: ModelId = ModelId("pyannote-segmentation");
    /// [P15.2] Silero VAD v5.1.2 для whisper-cli `--vad` silence-trim.
    /// Shared across all 3 presets (~1.6 MB). Дропает silence regions ДО
    /// encoder pass → 30-50% wall-clock reduction на pause-heavy calls.
    pub const SILERO_VAD: ModelId = ModelId("silero-vad-v5");
    /// [M15.9] Текст-эмбеддер RAG-ассистента (retrieval Ph2, гибрид RRF).
    /// Официальный intfloat ONNX-экспорт multilingual-e5-small, dynamic
    /// qint8 (имя файла упоминает avx512_vnni, но квантованные ops исполнимы
    /// и на arm64 — спайк M15.9: ~5мс/пассаж на M1 Pro). Shared across
    /// presets, optional: без него retrieval деградирует до чистого BM25.
    pub const E5_SMALL_QINT8: ModelId = ModelId("e5-small-qint8");
    /// [M15.9] tokenizer.json (XLM-R fast tokenizer) того же HF-репо — второй
    /// обязательный файл эмбеддера.
    pub const E5_TOKENIZER: ModelId = ModelId("e5-small-tokenizer");

    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Запись каталога. Захардкожено в `MODEL_CATALOG` (PRD §M12.4.1, DEFERRED
/// alternative — `models-manifest.json` endpoint).
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: ModelId,
    pub kind: ModelKind,
    pub display_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub license_url: &'static str,
}

/// Каталог — 4 STT + 4 LLM + 1 diarization + 2 embedding файла. SHA256 +
/// size_bytes получены через `scripts/refresh-model-catalog.sh` (PRD §14
/// pre-flight) на 2026-05-22 (e5-записи — спайк M15.9, 2026-07-22).
/// При замене файла на HF — bump version в скрипте + регенерировать.
pub const MODEL_CATALOG: [ModelEntry; 11] = [
    ModelEntry {
        id: ModelId::WHISPER_SMALL,
        kind: ModelKind::Stt,
        display_name: "Whisper Small (RU+EN)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        size_bytes: 190_085_487,
        license_url: "https://huggingface.co/ggerganov/whisper.cpp",
    },
    ModelEntry {
        id: ModelId::WHISPER_MEDIUM,
        kind: ModelKind::Stt,
        display_name: "Whisper Medium (RU+EN)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin",
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        size_bytes: 539_212_467,
        license_url: "https://huggingface.co/ggerganov/whisper.cpp",
    },
    ModelEntry {
        id: ModelId::WHISPER_LARGE_V3,
        kind: ModelKind::Stt,
        display_name: "Whisper Large v3",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
        sha256: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        size_bytes: 1_081_140_203,
        license_url: "https://huggingface.co/ggerganov/whisper.cpp",
    },
    ModelEntry {
        // [M14 T-16 P2] Speculative-decoding draft модель. Standalone не
        // используется (slug в preset.rs не указывает на 0.5B). При активации
        // SUMMARY_SPECULATIVE_DECODING flag + preset=Quality, llama-cli
        // получает `--model-draft <path>` указывающий на этот файл.
        // [TD-10] SHA256/size сняты с HF (x-linked-etag + content-length),
        // раньше были placeholder — модель была недокачиваема, а size — оценка.
        id: ModelId::QWEN25_0_5B,
        kind: ModelKind::Llm,
        display_name: "Qwen 2.5 (0.5B) — draft model",
        url: "https://huggingface.co/bartowski/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf",
        sha256: "6eb923e7d26e9cea28811e1a8e852009b21242fb157b26149d3b188f3a8c8653",
        size_bytes: 397_808_192,
        license_url: "https://huggingface.co/bartowski/Qwen2.5-0.5B-Instruct-GGUF",
    },
    ModelEntry {
        id: ModelId::QWEN25_1_5B,
        kind: ModelKind::Llm,
        display_name: "Qwen 2.5 (1.5B)",
        url: "https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf",
        sha256: "1adf0b11065d8ad2e8123ea110d1ec956dab4ab038eab665614adba04b6c3370",
        size_bytes: 986_048_768,
        license_url: "https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF",
    },
    ModelEntry {
        id: ModelId::QWEN25_3B,
        kind: ModelKind::Llm,
        display_name: "Qwen 2.5 (3B)",
        url: "https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf",
        sha256: "9c9f56a391a3abbd5b89d0245bf6106081bcc3173119d4229235dd9d23253f94",
        size_bytes: 1_929_903_264,
        license_url: "https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF",
    },
    ModelEntry {
        id: ModelId::QWEN25_7B,
        kind: ModelKind::Llm,
        display_name: "Qwen 2.5 (7B)",
        url: "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf",
        sha256: "65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423",
        size_bytes: 4_683_074_240,
        license_url: "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF",
    },
    ModelEntry {
        id: ModelId::PYANNOTE_SEGMENTATION,
        kind: ModelKind::Diarization,
        display_name: "Pyannote Segmentation 3.0",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.onnx",
        sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
        size_bytes: 5_992_913,
        license_url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0",
    },
    // [P15.2] Silero VAD model для whisper-cli `--vad`. ggml-org официально
    // публикует ggml-silero-v5.1.2.bin. [TD-10] SHA256/size сняты с HF —
    // раньше были placeholder, а заявленный размер (~1.6 MB) был вдвое больше
    // реального (885 KB), из-за чего check_status_fast пометил бы корректно
    // скачанный файл как Corrupted. Если SHA mismatch — download отказывает,
    // pipeline gracefully fallback'ится без `--vad`.
    ModelEntry {
        id: ModelId::SILERO_VAD,
        kind: ModelKind::Stt,
        display_name: "Silero VAD v5",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
        sha256: "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
        size_bytes: 885_098,
        license_url: "https://huggingface.co/ggml-org/whisper-vad",
    },
    // [M15.9] Эмбеддер ассистента. SHA256/size сняты в спайке 2026-07-22 с
    // официального intfloat-репо (MIT). PRD §6.3 оценивал «~30MB» — фактически
    // qint8 = 118MB: у XLM-R словарь 250k доминирует в весах.
    ModelEntry {
        id: ModelId::E5_SMALL_QINT8,
        kind: ModelKind::Embedding,
        display_name: "Multilingual E5 Small (поиск)",
        url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main/onnx/model_qint8_avx512_vnni.onnx",
        sha256: "dd476dd0c2514e9b9be83aeb3853fac0763e0bdf4a71645407587d77c48a2d88",
        size_bytes: 118_346_824,
        license_url: "https://huggingface.co/intfloat/multilingual-e5-small",
    },
    ModelEntry {
        id: ModelId::E5_TOKENIZER,
        kind: ModelKind::Embedding,
        display_name: "Multilingual E5 Tokenizer",
        url: "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main/onnx/tokenizer.json",
        sha256: "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
        size_bytes: 17_082_730,
        license_url: "https://huggingface.co/intfloat/multilingual-e5-small",
    },
];

/// Найти запись каталога по id. `None` если id неизвестен — caller обязан
/// сообщить ошибку, никогда не fallback'иться на «дефолт».
pub fn lookup(id: &str) -> Option<&'static ModelEntry> {
    MODEL_CATALOG.iter().find(|m| m.id.as_str() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_expected_kind_counts() {
        let count = |kind: ModelKind| MODEL_CATALOG.iter().filter(|m| m.kind == kind).count();
        // [P15.2] +silero-vad-v5 (Stt-kind, helper для whisper-cli `--vad`).
        assert_eq!(
            count(ModelKind::Stt),
            4,
            "expected 4 STT models (whisper small/medium/large-v3 + silero-vad-v5)"
        );
        // [M14 T-16 P2] +qwen25-0_5b (draft model для speculative decoding).
        assert_eq!(
            count(ModelKind::Llm),
            4,
            "expected 4 LLM models (qwen25-0_5b draft + 1_5b/3b/7b targets)"
        );
        assert_eq!(
            count(ModelKind::Diarization),
            1,
            "expected 1 diarization model (pyannote)"
        );
        // [M15.9] Текст-эмбеддер ассистента: onnx-модель + tokenizer.json.
        assert_eq!(
            count(ModelKind::Embedding),
            2,
            "expected 2 embedding files (e5-small qint8 onnx + tokenizer.json)"
        );
    }

    #[test]
    fn catalog_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for entry in MODEL_CATALOG.iter() {
            assert!(
                seen.insert(entry.id.as_str()),
                "duplicate model id: {}",
                entry.id.as_str()
            );
        }
    }

    #[test]
    fn catalog_urls_use_https() {
        for entry in MODEL_CATALOG.iter() {
            assert!(
                entry.url.starts_with("https://"),
                "model {} URL must be HTTPS: {}",
                entry.id.as_str(),
                entry.url
            );
        }
    }

    #[test]
    fn lookup_finds_each_canonical_id() {
        for entry in MODEL_CATALOG.iter() {
            assert_eq!(
                lookup(entry.id.as_str()).map(|e| e.id.as_str()),
                Some(entry.id.as_str())
            );
        }
        assert!(lookup("not-a-model").is_none());
    }
}
