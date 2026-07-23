//! [M15.1] Типы ассистента — зеркало `packages/contracts/src/assistant.ts` (S2).
//!
//! Wire-формат camelCase (`rename_all`), persist ответа — JSON в
//! `assistant_messages.answer_json`. Семантика — docs/M15_ASSISTANT_PRD.md §7.

// [M15.1] Backbone — production callers (indexer/retrieval/answer/commands)
// подключаются в M15.3..M15.8.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Вид ответа: refusal = генеративный запрос (без retrieval), empty = не найдено.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssistantAnswerKind {
    Answer,
    Refusal,
    Empty,
}

/// Тип пассажа индекса (`assistant_passages.kind`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssistantPassageKind {
    Transcript,
    Recap,
    Decision,
    ActionItem,
    OpenQuestion,
    /// [M16.6] Синтетическая «карточка звонка»: титул + дата + участники —
    /// якорь для вопросов «в каком звонке / кто был / о чём» (глобально).
    CallMeta,
    /// [B26.5] Карточка контакта (инжект-канал; в assistant_passages НЕ
    /// хранится — только на wire в answer_json, sentinel call_id contact:*).
    Contact,
}

impl AssistantPassageKind {
    /// Stable string id для DB-колонки `assistant_passages.kind`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AssistantPassageKind::Transcript => "transcript",
            AssistantPassageKind::Recap => "recap",
            AssistantPassageKind::Decision => "decision",
            AssistantPassageKind::ActionItem => "action_item",
            AssistantPassageKind::OpenQuestion => "open_question",
            AssistantPassageKind::CallMeta => "call_meta",
            AssistantPassageKind::Contact => "contact",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "transcript" => AssistantPassageKind::Transcript,
            "recap" => AssistantPassageKind::Recap,
            "decision" => AssistantPassageKind::Decision,
            "action_item" => AssistantPassageKind::ActionItem,
            "open_question" => AssistantPassageKind::OpenQuestion,
            "call_meta" => AssistantPassageKind::CallMeta,
            "contact" => AssistantPassageKind::Contact,
            _ => return None,
        })
    }
}

/// Источник ответа: звонок + опциональный таймкод (None — recap-абзац и т.п.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSource {
    pub call_id: String,
    /// Денормализован на момент ответа; фронт резолвит заново при рендере истории.
    pub call_title: String,
    pub start_ms: Option<i64>,
}

/// Фрагмент, реально попавший в контекст LLM (блок «Контекст поиска»).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistantFragment {
    pub call_id: String,
    pub call_title: String,
    pub kind: AssistantPassageKind,
    pub speaker: Option<String>,
    pub start_ms: Option<i64>,
    pub text: String,
    /// [B26.4] Текст усечён при отдаче на фронт (полный — в answer_json,
    /// ленивая подгрузка командой `assistant_get_fragment_text`).
    /// `default` — старые answer_json без поля читаются как false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub text_truncated: bool,
}

/// Полный ответ (persist в `assistant_messages.answer_json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistantAnswer {
    pub kind: AssistantAnswerKind,
    pub text: String,
    /// Пусто для refusal/empty.
    pub sources: Vec<AssistantSource>,
    /// Пусто для refusal (retrieval не выполнялся).
    pub fragments: Vec<AssistantFragment>,
    /// Оценка токенов фрагментов (mono-строка «фрагментов: N · ≈X.XK»).
    pub fragment_tokens: u32,
    /// Фикс окна локальной модели (8192).
    pub window_tokens: u32,
    /// Только empty в call-scope: чип «Искать во всех звонках».
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalate: Option<bool>,
}

/// Метаданные чата. `call_id == None` — глобальный чат раздела.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistantChatMeta {
    pub id: String,
    pub call_id: Option<String>,
    /// Первый вопрос, усечённый до ~42 симв.
    pub title: String,
    pub created_at: String,
}

/// Роль сообщения.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssistantRole {
    User,
    Assistant,
}

impl AssistantRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AssistantRole::User => "user",
            AssistantRole::Assistant => "assistant",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "user" => AssistantRole::User,
            "assistant" => AssistantRole::Assistant,
            _ => return None,
        })
    }
}

/// Сообщение чата; для role=assistant заполнен `answer`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub id: String,
    pub role: AssistantRole,
    pub text: String,
    pub answer: Option<AssistantAnswer>,
    pub created_at: String,
}

/// Статистика индекса: чип «в поиске X из Y звонков · ЧЧ ч ММ мин».
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistantIndexStats {
    pub indexed_calls: u32,
    pub total_calls: u32,
    pub total_duration_sec: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_answer() -> AssistantAnswer {
        AssistantAnswer {
            kind: AssistantAnswerKind::Answer,
            text: "Два решения: локальный режим на демо; отчёт к пятнице.".into(),
            sources: vec![AssistantSource {
                call_id: "c1".into(),
                call_title: "Синхрон по пилоту".into(),
                start_ms: Some(62_000),
            }],
            fragments: vec![AssistantFragment {
                call_id: "c1".into(),
                call_title: "Синхрон по пилоту".into(),
                kind: AssistantPassageKind::Transcript,
                speaker: Some("Арман Сулейменов".into()),
                start_ms: Some(62_000),
                text: "И давайте зафиксируем: на демо показываем локальный режим.".into(),
                text_truncated: false,
            }],
            fragment_tokens: 1_400,
            window_tokens: 8_192,
            escalate: None,
        }
    }

    #[test]
    fn answer_roundtrip_camel_case() {
        let ans = sample_answer();
        let json = serde_json::to_string(&ans).expect("serialize");
        // Wire-формат — camelCase, kind — snake_case литералы.
        assert!(json.contains("\"callId\":\"c1\""));
        assert!(json.contains("\"callTitle\""));
        assert!(json.contains("\"startMs\":62000"));
        assert!(json.contains("\"fragmentTokens\":1400"));
        assert!(json.contains("\"windowTokens\":8192"));
        assert!(json.contains("\"kind\":\"answer\""));
        assert!(json.contains("\"kind\":\"transcript\""));
        // escalate: None не сериализуется.
        assert!(!json.contains("escalate"));
        let back: AssistantAnswer = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ans);
    }

    #[test]
    fn message_roundtrip_with_null_answer() {
        let msg = AssistantMessage {
            id: "m1".into(),
            role: AssistantRole::User,
            text: "Какие задачи у Дмитрия?".into(),
            answer: None,
            created_at: "2026-07-22T10:00:00Z".into(),
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"answer\":null"));
        assert!(json.contains("\"createdAt\""));
        let back: AssistantMessage = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, msg);
    }

    #[test]
    fn passage_kind_db_roundtrip() {
        for kind in [
            AssistantPassageKind::Transcript,
            AssistantPassageKind::Recap,
            AssistantPassageKind::Decision,
            AssistantPassageKind::ActionItem,
            AssistantPassageKind::OpenQuestion,
        ] {
            assert_eq!(AssistantPassageKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(AssistantPassageKind::parse("bogus"), None);
    }

    #[test]
    fn empty_answer_with_escalate() {
        let ans = AssistantAnswer {
            kind: AssistantAnswerKind::Empty,
            text: "В этом звонке этого не нашлось.".into(),
            sources: vec![],
            fragments: vec![],
            fragment_tokens: 0,
            window_tokens: 8_192,
            escalate: Some(true),
        };
        let json = serde_json::to_string(&ans).expect("serialize");
        assert!(json.contains("\"kind\":\"empty\""));
        assert!(json.contains("\"escalate\":true"));
        let back: AssistantAnswer = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ans);
    }
}
