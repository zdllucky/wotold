//! [M14 foundation] Type-driven evidence-grounded summary schema v2.
//!
//! Backbone для будущих фаз M14 (T-02..T-12). Содержит:
//! - 8 call types + `Other` fallback ([`CallType`]).
//! - Action item categories: commitment / proposal / idea ([`ActionItemCategory`]).
//! - Substring-anchored evidence quotes ([`EvidenceAnchor`]).
//! - Полная structured summary ([`CallSummaryV2`]) — это то, что в будущих
//!   фазах будет выдавать local Qwen pipeline + cloud Groq prompt.
//!
//! **Schema versioning:** все existing rows получают `summary_schema_version=1`
//! через migration 0015 default; новые v2 будут писать 2. Pipeline сам решит
//! какую версию рендерить ([T-11] UI legacy adapter).
//!
//! **Serde aliases:** некоторые cloud провайдеры возвращают camelCase
//! (`ownerHint`, `startMs`) — поддерживаем оба варианта через
//! `#[serde(alias = ...)]`. Output идёт в snake_case (`rename_all`).

// [M14 foundation] Backbone — production callers (cloud prompt + local
// pipeline) подключатся в T-02..T-10. Тесты verify roundtrip + aliases.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// 8 типов корпоративных звонков + `Other` fallback. Источник истины — PRD §5.1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CallType {
    SalesDiscovery,
    SalesDemo,
    ProductSync,
    Standup,
    CustomerInterview,
    OneOnOne,
    StrategyBrainstorm,
    StatusUpdate,
    Other,
}

impl CallType {
    /// Stable string id для DB column `calls.call_type` (snake_case).
    pub fn as_str(&self) -> &'static str {
        match self {
            CallType::SalesDiscovery => "sales_discovery",
            CallType::SalesDemo => "sales_demo",
            CallType::ProductSync => "product_sync",
            CallType::Standup => "standup",
            CallType::CustomerInterview => "customer_interview",
            CallType::OneOnOne => "one_on_one",
            CallType::StrategyBrainstorm => "strategy_brainstorm",
            CallType::StatusUpdate => "status_update",
            CallType::Other => "other",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "sales_discovery" => CallType::SalesDiscovery,
            "sales_demo" => CallType::SalesDemo,
            "product_sync" => CallType::ProductSync,
            "standup" => CallType::Standup,
            "customer_interview" => CallType::CustomerInterview,
            "one_on_one" => CallType::OneOnOne,
            "strategy_brainstorm" => CallType::StrategyBrainstorm,
            "status_update" => CallType::StatusUpdate,
            "other" => CallType::Other,
            _ => return None,
        })
    }
}

/// Категория action item — кто и зачем сказал:
/// - `Commitment` — explicit accept («I'll do X»).
/// - `Proposal` — suggested но не accepted.
/// - `Idea` — raised, no clear action assignment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ActionItemCategory {
    Commitment,
    Proposal,
    Idea,
}

impl ActionItemCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionItemCategory::Commitment => "commitment",
            ActionItemCategory::Proposal => "proposal",
            ActionItemCategory::Idea => "idea",
        }
    }
}

fn default_commitment() -> ActionItemCategory {
    ActionItemCategory::Commitment
}

/// Default `id` для items, когда модель его не эмитит (JSON-schema не требует
/// id — мелкие local-модели часто опускают). Без serde-default serde падал на
/// `missing field id` → весь CallSummaryV2 parse откатывался на v1 legacy.
fn gen_item_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Substring-anchored evidence quote. `quote` обязательно verbatim substring
/// transcript'а (≥ 90% fuzzy match per [`crate::pipeline::summary_validator`]).
/// Если evidence не найдётся — caller drop'ит item (degraded ok).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct EvidenceAnchor {
    pub quote: String,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default, alias = "startMs")]
    pub start_ms: Option<i64>,
    #[serde(default, alias = "endMs")]
    pub end_ms: Option<i64>,
}

/// V2 action item с confidence + category + evidence. UI рендерит confidence
/// badges per PRD §7.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItemV2 {
    #[serde(default = "gen_item_id")]
    pub id: String,
    pub text: String,
    #[serde(default, alias = "ownerHint")]
    pub owner_hint: Option<String>,
    /// 0..1. ≥ 0.8 only при explicit accept (см. PRD §5.7 personal deixis warning).
    #[serde(default, alias = "ownerConfidence")]
    pub owner_confidence: Option<f32>,
    /// ISO date OR human ("end of Q2"). Free-form.
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default, alias = "dueConfidence")]
    pub due_confidence: Option<f32>,
    #[serde(default = "default_commitment")]
    pub category: ActionItemCategory,
    #[serde(default)]
    pub evidence: Option<EvidenceAnchor>,
}

/// Decision (явный выбор сделанный в звонке).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    #[serde(default = "gen_item_id")]
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub evidence: Option<EvidenceAnchor>,
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// Open question (поднятый, не разрешённый вопрос).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenQuestion {
    #[serde(default = "gen_item_id")]
    pub id: String,
    pub text: String,
    #[serde(default, alias = "raisedBy")]
    pub raised_by: Option<String>,
    #[serde(default)]
    pub evidence: Option<EvidenceAnchor>,
}

/// Participant с (опц.) ролью.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantV2 {
    #[serde(alias = "speakerTag")]
    pub speaker_tag: String,
    #[serde(default, alias = "displayName")]
    pub display_name: Option<String>,
    #[serde(default, alias = "roleHint")]
    pub role_hint: Option<String>,
}

/// [recap-rich] Обсуждённая тема с под-пунктами — секция «Темы» в рекапе.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicV2 {
    pub title: String,
    #[serde(default)]
    pub points: Vec<String>,
}

/// Полная V2 summary — то, что выдаёт pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSummaryV2 {
    /// Версия схемы — `2` для всех new rows. UI legacy adapter switch'ит на
    /// V1 рендер если schema_version == 1.
    pub schema_version: u8,
    pub title: String,
    pub summary: String,
    pub key_points: Vec<String>,
    /// [MoM cleanup] Deprecated — больше не запрашивается в промпте и не
    /// рендерится в recap.md (модель эхо-копировала схему сюда → мусор).
    /// serde-default чтобы старые/новые ответы без `mom` парсились в v2.
    #[serde(default)]
    pub mom: String,
    /// "ru" | "en" | "kk" | "mixed". Дублируется в `calls.lang_detected`.
    pub language: String,
    pub call_type: CallType,
    pub call_type_confidence: f32,
    pub participants: Vec<ParticipantV2>,
    pub action_items: Vec<ActionItemV2>,
    pub decisions: Vec<Decision>,
    pub open_questions: Vec<OpenQuestion>,
    /// [recap-rich] Обсуждённые темы с под-пунктами. `skip_serializing_if` —
    /// пустой массив не попадает в JSON (совместимость с golden-фикстурами +
    /// не шумим). `default` — старые ответы без `topics` парсятся.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<TopicV2>,
    /// [recap-rich] Связный markdown-протокол встречи (нарратив-минутки),
    /// генерится отдельным narrative-проходом после reduce (backend, не LLM v2
    /// JSON). Пустой не сериализуется.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub narrative: String,
    /// JSON object с per-type structured data (pain_points для sales_discovery,
    /// per_person для standup и т.д.). `None` если call_type=Other.
    #[serde(default)]
    pub type_specific_block: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn call_type_roundtrip_via_as_str_and_from_str() {
        for ct in [
            CallType::SalesDiscovery,
            CallType::SalesDemo,
            CallType::ProductSync,
            CallType::Standup,
            CallType::CustomerInterview,
            CallType::OneOnOne,
            CallType::StrategyBrainstorm,
            CallType::StatusUpdate,
            CallType::Other,
        ] {
            let s = ct.as_str();
            assert_eq!(CallType::from_str(s), Some(ct));
        }
        assert_eq!(CallType::from_str("nonsense"), None);
    }

    #[test]
    fn call_type_serde_snake_case() {
        let ct = CallType::SalesDiscovery;
        let s = serde_json::to_string(&ct).unwrap();
        assert_eq!(s, r#""sales_discovery""#);
        let back: CallType = serde_json::from_str(&s).unwrap();
        assert_eq!(back, ct);
    }

    #[test]
    fn action_item_category_defaults_to_commitment() {
        // Если cloud LLM не вернул `category` — default commitment per PRD §5.7.
        let json_no_cat = r#"{"id":"a1","text":"do stuff"}"#;
        let ai: ActionItemV2 = serde_json::from_str(json_no_cat).unwrap();
        assert_eq!(ai.category, ActionItemCategory::Commitment);
    }

    #[test]
    fn items_without_id_get_autogen_default() {
        // [Fix B2] Слабая local-модель часто опускает `id` (schema его не
        // требует). Раньше serde падал на missing field → весь CallSummaryV2
        // откатывался на v1 legacy. Теперь id авто-генерится, parse проходит.
        let ai: ActionItemV2 = serde_json::from_str(r#"{"text":"no id here"}"#).unwrap();
        assert!(!ai.id.is_empty(), "id должен авто-сгенериться");
        let d: Decision = serde_json::from_str(r#"{"text":"decided"}"#).unwrap();
        assert!(!d.id.is_empty());
        let q: OpenQuestion = serde_json::from_str(r#"{"text":"open?"}"#).unwrap();
        assert!(!q.id.is_empty());
        // Два разных item'а получают разные id.
        let a2: ActionItemV2 = serde_json::from_str(r#"{"text":"x"}"#).unwrap();
        assert_ne!(ai.id, a2.id);
    }

    #[test]
    fn evidence_anchor_accepts_camelcase_aliases() {
        // Cloud провайдеры (например xAI Grok) часто возвращают camelCase.
        let camel_json = r#"{"quote":"hello","startMs":1500,"endMs":3000}"#;
        let ev: EvidenceAnchor = serde_json::from_str(camel_json).unwrap();
        assert_eq!(ev.start_ms, Some(1500));
        assert_eq!(ev.end_ms, Some(3000));
    }

    #[test]
    fn action_item_accepts_camelcase_aliases() {
        let camel_json = r#"{
            "id": "a1", "text": "follow up",
            "ownerHint": "Alice",
            "ownerConfidence": 0.95,
            "dueConfidence": 0.6,
            "category": "commitment"
        }"#;
        let ai: ActionItemV2 = serde_json::from_str(camel_json).unwrap();
        assert_eq!(ai.owner_hint.as_deref(), Some("Alice"));
        assert!((ai.owner_confidence.unwrap() - 0.95).abs() < 1e-6);
        assert!((ai.due_confidence.unwrap() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn full_summary_serde_roundtrip() {
        let original = CallSummaryV2 {
            schema_version: 2,
            title: "Q3 pricing decision".into(),
            summary: "Team agreed on enterprise tier at $499.".into(),
            key_points: vec![
                "Pricing locked".into(),
                "Launch next week".into(),
                "GTM alignment".into(),
            ],
            mom: "## Decisions\n- Enterprise tier $499".into(),
            language: "en".into(),
            call_type: CallType::SalesDiscovery,
            call_type_confidence: 0.92,
            participants: vec![ParticipantV2 {
                speaker_tag: "owner".into(),
                display_name: Some("Alice".into()),
                role_hint: Some("CEO".into()),
            }],
            action_items: vec![ActionItemV2 {
                id: "ai-1".into(),
                text: "Send proposal to Bob by Tuesday".into(),
                owner_hint: Some("Alice".into()),
                owner_confidence: Some(0.95),
                due: Some("Tuesday".into()),
                due_confidence: Some(0.8),
                category: ActionItemCategory::Commitment,
                evidence: Some(EvidenceAnchor {
                    quote: "I'll send the proposal to Bob by Tuesday".into(),
                    speaker: Some("owner".into()),
                    start_ms: Some(12_000),
                    end_ms: Some(15_500),
                }),
            }],
            decisions: vec![Decision {
                id: "d-1".into(),
                text: "Enterprise tier locked at $499".into(),
                evidence: None,
                confidence: Some(0.85),
            }],
            open_questions: vec![OpenQuestion {
                id: "q-1".into(),
                text: "Should we offer a trial?".into(),
                raised_by: Some("speaker:0".into()),
                evidence: None,
            }],
            topics: vec![TopicV2 {
                title: "Pricing".into(),
                points: vec!["Enterprise tier at $499".into()],
            }],
            narrative: "На звонке обсудили тариф и следующие шаги.".into(),
            type_specific_block: Some(json!({
                "pain_points": ["slow onboarding", "manual exports"],
                "budget_signal": "approved $50K",
            })),
        };
        let s = serde_json::to_string(&original).unwrap();
        let back: CallSummaryV2 = serde_json::from_str(&s).unwrap();
        assert_eq!(back.schema_version, 2);
        assert_eq!(back.call_type, CallType::SalesDiscovery);
        assert_eq!(back.action_items.len(), 1);
        assert_eq!(
            back.action_items[0].category,
            ActionItemCategory::Commitment
        );
        assert_eq!(back.decisions[0].confidence, Some(0.85));
        assert!(back.type_specific_block.is_some());
    }

    #[test]
    fn missing_optional_fields_use_defaults() {
        // Minimal summary с только required полями — должен парситься.
        let minimal = json!({
            "schema_version": 2,
            "title": "Standup",
            "summary": "Daily standup.",
            "key_points": ["A", "B", "C"],
            "mom": "## Yesterday\n- A",
            "language": "en",
            "call_type": "standup",
            "call_type_confidence": 0.99,
            "participants": [],
            "action_items": [],
            "decisions": [],
            "open_questions": []
        });
        let s: CallSummaryV2 = serde_json::from_value(minimal).unwrap();
        assert!(s.type_specific_block.is_none());
    }
}
