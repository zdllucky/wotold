//! LLM hint для speaker identification (#25 / M3.3).
//!
//! По диаризованному транскрипту и списку известных контактов модель предлагает
//! привязку спикеров через обращения по имени, контекст разговора, должность.
//! Голосовая биометрия (embedding) — параллельный сигнал; merge сливает оба
//! в один ranked suggestion per speaker (M3.4 → `crate::merge`).
//!
//! Результат — `HashMap<speaker_tag, MatchCandidate>` для совместимости с
//! embedding-результатом.

use std::collections::HashMap;

use serde::Deserialize;

use crate::{
    matching::MatchCandidate,
    providers::llm::{LlmProvider, LlmRequest},
    AppError,
};

/// Контакт-кандидат, который мы шлём в LLM. Owner отсекается вызывающим (mic-track).
#[derive(Debug, Clone)]
pub struct LlmHintContact {
    pub id: String,
    pub display_name: String,
    pub role: Option<String>,
    pub org: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HintResponse {
    suggestions: Vec<HintSuggestion>,
}

#[derive(Debug, Deserialize)]
struct HintSuggestion {
    speaker_tag: String,
    contact_id: String,
    score: f32,
}

/// Запрос к LLM. Возвращает HashMap по speaker_tag. Score ограничен [0,1].
/// При ошибке провайдера или невалидном JSON возвращает пустую map — pipeline
/// продолжит matching по embedding-only (M3.4 graceful degradation).
pub async fn request_speaker_hints(
    provider: &dyn LlmProvider,
    transcript_md: &str,
    contacts: &[LlmHintContact],
    model: &str,
) -> Result<HashMap<String, MatchCandidate>, AppError> {
    if contacts.is_empty() {
        return Ok(HashMap::new());
    }
    let prompt = build_prompt(transcript_md, contacts);

    let req = LlmRequest {
        model: Some(model.to_string()),
        system: SYSTEM_PROMPT.to_string(),
        input: prompt,
        max_tokens: Some(800),
    };

    // AnthropicProvider возвращает уже распарсенный JSON (Value).
    let raw = match provider.generate(req).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("llm hint provider failed (continuing without): {e}");
            return Ok(HashMap::new());
        }
    };

    parse_value(raw, contacts)
}

const SYSTEM_PROMPT: &str = "Ты ассистент идентификации спикеров. По диаризованному транскрипту и списку известных контактов предположи, кто какой спикер из списка. Используй обращения по имени, контекст должности/организации, манеру речи. Если уверенности < 0.6 — не возвращай этого speaker'а. Верни СТРОГО JSON:\n{\"suggestions\":[{\"speaker_tag\":\"Speaker 0\",\"contact_id\":\"<uuid>\",\"score\":0.85}]}\nscore в [0,1]. Никакого Markdown, никаких комментариев.";

fn build_prompt(transcript_md: &str, contacts: &[LlmHintContact]) -> String {
    let contacts_list = contacts
        .iter()
        .map(|c| {
            let role_org: Vec<&str> = [c.role.as_deref(), c.org.as_deref()]
                .into_iter()
                .flatten()
                .collect();
            let suffix = if role_org.is_empty() {
                String::new()
            } else {
                format!(" — {}", role_org.join(", "))
            };
            format!("- id={} name=\"{}\"{}", c.id, c.display_name, suffix)
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Известные контакты:\n{contacts_list}\n\nТранскрипт:\n```\n{transcript_md}\n```\n\nВерни JSON с suggestions[]."
    )
}

fn parse_value(
    value: serde_json::Value,
    contacts: &[LlmHintContact],
) -> Result<HashMap<String, MatchCandidate>, AppError> {
    let parsed: HintResponse = serde_json::from_value(value)
        .map_err(|e| AppError::Other(format!("llm hint json parse: {e}")))?;

    let mut out = HashMap::new();
    let by_id: HashMap<&str, &LlmHintContact> =
        contacts.iter().map(|c| (c.id.as_str(), c)).collect();

    for s in parsed.suggestions {
        let score = s.score.clamp(0.0, 1.0);
        let Some(contact) = by_id.get(s.contact_id.as_str()) else {
            // Модель выдумала несуществующий contact_id — игнор.
            log::warn!("llm hint: unknown contact_id {}", s.contact_id);
            continue;
        };
        out.insert(
            s.speaker_tag,
            MatchCandidate {
                contact_id: contact.id.clone(),
                display_name: contact.display_name.clone(),
                score,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
fn parse_response(
    text: &str,
    contacts: &[LlmHintContact],
) -> Result<HashMap<String, MatchCandidate>, AppError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| AppError::Other(format!("llm hint json parse: {e}")))?;
    parse_value(value, contacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contacts() -> Vec<LlmHintContact> {
        vec![
            LlmHintContact {
                id: "c1".into(),
                display_name: "Alice".into(),
                role: Some("CEO".into()),
                org: None,
            },
            LlmHintContact {
                id: "c2".into(),
                display_name: "Bob".into(),
                role: None,
                org: Some("Acme".into()),
            },
        ]
    }

    #[test]
    fn build_prompt_includes_contacts_and_transcript() {
        let p = build_prompt("Speaker 0: Hi", &sample_contacts());
        assert!(p.contains("c1"));
        assert!(p.contains("Alice"));
        assert!(p.contains("CEO"));
        assert!(p.contains("Acme"));
        assert!(p.contains("Speaker 0: Hi"));
    }

    #[test]
    fn parse_response_maps_speakers() {
        let json = r#"{"suggestions":[
            {"speaker_tag":"Speaker 0","contact_id":"c1","score":0.85},
            {"speaker_tag":"Speaker 1","contact_id":"c2","score":0.7}
        ]}"#;
        let map = parse_response(json, &sample_contacts()).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["Speaker 0"].contact_id, "c1");
        assert_eq!(map["Speaker 0"].display_name, "Alice");
        assert!((map["Speaker 0"].score - 0.85).abs() < 1e-6);
    }

    #[test]
    fn parse_response_drops_unknown_contact_id() {
        let json = r#"{"suggestions":[
            {"speaker_tag":"Speaker 0","contact_id":"ghost","score":0.9}
        ]}"#;
        let map = parse_response(json, &sample_contacts()).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn parse_response_clamps_score() {
        let json = r#"{"suggestions":[
            {"speaker_tag":"Speaker 0","contact_id":"c1","score":1.5},
            {"speaker_tag":"Speaker 1","contact_id":"c2","score":-0.2}
        ]}"#;
        let map = parse_response(json, &sample_contacts()).unwrap();
        assert!((map["Speaker 0"].score - 1.0).abs() < 1e-6);
        assert!((map["Speaker 1"].score - 0.0).abs() < 1e-6);
    }

    #[test]
    fn parse_response_returns_error_on_invalid_json() {
        let err = parse_response("not json", &sample_contacts()).unwrap_err();
        assert!(matches!(err, AppError::Other(_)));
    }

    #[test]
    fn parse_response_handles_empty_suggestions() {
        let map = parse_response(r#"{"suggestions":[]}"#, &sample_contacts()).unwrap();
        assert!(map.is_empty());
    }
}
