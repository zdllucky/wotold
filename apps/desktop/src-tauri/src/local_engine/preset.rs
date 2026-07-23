//! [M12.4.3] Preset API — раскладка моделей под целевое железо.
//!
//! См. PRD §2.5:
//!
//! | Preset    | Whisper           | LLM           |
//! |-----------|-------------------|---------------|
//! | Light     | whisper-small     | gemma3-2b     |
//! | Balanced  | whisper-medium    | qwen25-3b     |
//! | Quality   | whisper-large-v3  | qwen25-7b     |
//!
//! Хранение: SQLite settings key `local_engine.active_preset` (см.
//! [`crate::db::settings`]). `None` до первого выбора в Settings UI (M12.5).

use serde::{Deserialize, Serialize};

use super::models::ModelId;

/// Целевая раскладка локального движка. Совместим с contract'ом
/// `packages/contracts/src/local-engine.ts::LocalEnginePreset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocalEnginePreset {
    Light,
    Balanced,
    Quality,
}

impl LocalEnginePreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            LocalEnginePreset::Light => "light",
            LocalEnginePreset::Balanced => "balanced",
            LocalEnginePreset::Quality => "quality",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "light" => Some(LocalEnginePreset::Light),
            "balanced" => Some(LocalEnginePreset::Balanced),
            "quality" => Some(LocalEnginePreset::Quality),
            _ => None,
        }
    }

    /// Какие модели нужны для этого preset'а. Используется UI чтобы
    /// показать progress на превращении Light → Balanced (M12.4.3).
    pub fn whisper_model_id(&self) -> ModelId {
        match self {
            LocalEnginePreset::Light => ModelId::WHISPER_SMALL,
            LocalEnginePreset::Balanced => ModelId::WHISPER_MEDIUM,
            LocalEnginePreset::Quality => ModelId::WHISPER_LARGE_V3,
        }
    }

    pub fn llm_model_id(&self) -> ModelId {
        match self {
            // PRD §11 O1 deviation: Gemma → Qwen 1.5B (Gemma gated by Google TOS).
            LocalEnginePreset::Light => ModelId::QWEN25_1_5B,
            LocalEnginePreset::Balanced => ModelId::QWEN25_3B,
            LocalEnginePreset::Quality => ModelId::QWEN25_7B,
        }
    }

    /// Оба model id'а для preset'а. Удобно для cleanup и status-aggregation
    /// в UI («установить Balanced — скачать обе модели»). Wire-up в M12.5
    /// Settings UI (Design Gate'нутая секция picker'а preset'ов).
    ///
    /// Не включает pyannote-segmentation — она shared across presets и
    /// optional (degraded mode без неё). См. `shared_model_ids`.
    #[allow(dead_code)]
    pub fn required_model_ids(&self) -> [ModelId; 2] {
        [self.whisper_model_id(), self.llm_model_id()]
    }

    /// [M12-D5] Модели общие для всех presets — pyannote segmentation для
    /// multi-speaker диаризации + [M15.9] пара файлов текст-эмбеддера
    /// ассистента. Все optional: без pyannote system track single-bucket,
    /// без эмбеддера retrieval ассистента деградирует до чистого BM25.
    #[allow(dead_code)]
    pub fn shared_model_ids() -> [ModelId; 3] {
        [
            ModelId::PYANNOTE_SEGMENTATION,
            ModelId::E5_SMALL_QINT8,
            ModelId::E5_TOKENIZER,
        ]
    }

    /// [P5.1] Human-friendly engine label для `calls.summary_engine` field.
    /// Используется как в success path (`persist_recap_from_json`), так и в
    /// failure path (`set_recap_failure`) — гарантирует consistent badge ↔
    /// reason matching в UI.
    pub fn engine_label(&self) -> &'static str {
        match self {
            LocalEnginePreset::Light => "local-qwen-1.5b",
            LocalEnginePreset::Balanced => "local-qwen-3b",
            LocalEnginePreset::Quality => "local-qwen-7b",
        }
    }
}

/// Сериализованная raskладка для отдачи на frontend. См. contract
/// `packages/contracts/src/local-engine.ts::PresetSpec`.
#[derive(Debug, Clone, Serialize)]
pub struct PresetSpec {
    pub preset: LocalEnginePreset,
    pub whisper_model_id: &'static str,
    pub llm_model_id: &'static str,
}

impl From<LocalEnginePreset> for PresetSpec {
    fn from(preset: LocalEnginePreset) -> Self {
        Self {
            preset,
            whisper_model_id: preset.whisper_model_id().as_str(),
            llm_model_id: preset.llm_model_id().as_str(),
        }
    }
}

/// Settings KV-ключ. См. PRD §M12.4.3 («атомарный swap setting `local_engine.active_preset`»).
pub const SETTING_ACTIVE_PRESET: &str = "local_engine.active_preset";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_round_trips() {
        for p in [
            LocalEnginePreset::Light,
            LocalEnginePreset::Balanced,
            LocalEnginePreset::Quality,
        ] {
            assert_eq!(LocalEnginePreset::from_str(p.as_str()), Some(p));
        }
        assert_eq!(LocalEnginePreset::from_str("xtreme"), None);
    }

    #[test]
    fn required_model_ids_are_distinct_per_preset() {
        let light = LocalEnginePreset::Light.required_model_ids();
        let balanced = LocalEnginePreset::Balanced.required_model_ids();
        let quality = LocalEnginePreset::Quality.required_model_ids();
        // Whisper модели разные между preset'ами.
        assert_ne!(light[0], balanced[0]);
        assert_ne!(balanced[0], quality[0]);
        // LLM модели разные.
        assert_ne!(light[1], balanced[1]);
        assert_ne!(balanced[1], quality[1]);
        // Каждый preset — ровно одна STT + одна LLM.
        assert_eq!(light[0], ModelId::WHISPER_SMALL);
        assert_eq!(light[1], ModelId::QWEN25_1_5B);
        assert_eq!(balanced[1], ModelId::QWEN25_3B);
        assert_eq!(quality[1], ModelId::QWEN25_7B);
    }

    #[test]
    fn preset_spec_serializes_with_string_ids() {
        let spec = PresetSpec::from(LocalEnginePreset::Balanced);
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["preset"], "balanced");
        assert_eq!(json["whisper_model_id"], "whisper-medium");
        assert_eq!(json["llm_model_id"], "qwen25-3b");
    }

    #[test]
    fn light_preset_uses_qwen_not_gemma() {
        // PRD §11 O1 deviation: Gemma 3 2B заменён на Qwen 2.5 1.5B
        // (Google TOS gating). Регрессия: не вернуться обратно случайно.
        assert_eq!(
            LocalEnginePreset::Light.llm_model_id(),
            ModelId::QWEN25_1_5B
        );
    }
}
