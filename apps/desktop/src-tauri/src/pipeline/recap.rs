use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    db::{self, ActionItemInput},
    providers::{
        llm::{AnthropicProvider, LlmProvider, LlmRequest},
        ProviderMode,
    },
    AppError,
};

/// Структурный JSON от LLM-провайдера. См. M4.2 паспорта.
/// Поля `version` и `RecapParticipant::contact_id` не читаются сейчас, но
/// сохраняем их в схеме — пригодятся когда добавим версионирование и
/// post-confirmation owner-маппинг (#26).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RecapJson {
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub key_points: Vec<String>,
    #[serde(default)]
    pub mom: String,
    #[serde(default)]
    pub action_items: Vec<RecapActionItem>,
    #[serde(default)]
    pub participants: Vec<RecapParticipant>,
}

#[derive(Debug, Deserialize)]
pub struct RecapActionItem {
    pub text: String,
    #[serde(default)]
    pub owner_hint: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RecapParticipant {
    pub speaker_tag: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub contact_id: Option<String>,
}

/// Контекст одного рекап-вызова. Собирается на стороне pipeline::run или
/// команды regenerate_recap.
pub struct RecapCtx<'a> {
    pub call_id: &'a str,
    pub call_dir: &'a Path,
    pub transcript_md: &'a str,
    pub lang_detected: Option<&'a str>,
    pub proxy_base_url: &'a str,
    pub device_id: &'a Arc<str>,
    pub provider_path: &'a str,
    pub model_override: Option<&'a str>,
}

/// Генерирует recap.md и action_items по уже сохранённому transcript.md.
/// M4.2-4.4 паспорта. Вызывается:
///   - автоматически после транскрипции (chain in pipeline::run)
///   - из команды regenerate_recap (M4.5, перегенерация без re-STT)
pub async fn run(pool: &SqlitePool, ctx: RecapCtx<'_>) -> Result<(), AppError> {
    let mode = match ctx.provider_path {
        "managed" => {
            if ctx.proxy_base_url.is_empty() {
                return Err(AppError::Other(
                    "Proxy URL не настроен. Settings → Proxy URL (#22 / [B4]).".into(),
                ));
            }
            ProviderMode::Managed {
                proxy_base_url: ctx.proxy_base_url.to_string(),
                device_id: ctx.device_id.to_string(),
            }
        }
        "byo" => {
            return Err(AppError::Other(
                "BYO LLM key ещё не подключён. См. #47 в roadmap.".into(),
            ));
        }
        other => return Err(AppError::Other(format!("unknown provider_path: {other}"))),
    };

    // Собрать known speakers: подтверждённые привязки speaker_tag → contact.
    // Это даст LLM контекст «owner = Damir», «Speaker 0 = Ivan Petrov (Acme)».
    let known_speakers = build_known_speakers_block(pool, ctx.call_id).await?;

    let provider = AnthropicProvider::new(mode);
    let request = LlmRequest {
        model: ctx.model_override.map(str::to_string),
        system: build_system_prompt(ctx.lang_detected, known_speakers.as_deref()),
        input: ctx.transcript_md.to_string(),
        max_tokens: Some(4096),
    };

    let json_value = provider
        .generate(request)
        .await
        .map_err(|e| AppError::Other(format!("llm: {e}")))?;

    let recap: RecapJson = serde_json::from_value(json_value)
        .map_err(|e| AppError::Other(format!("recap JSON shape: {e}")))?;

    // Маппим owner_hint → contact_id через простой case-insensitive substring
    // match по display_name. Не нашли — оставляем текстом, M3 (matching)
    // улучшит точность.
    let contacts = db::list_contacts(pool).await?;
    let action_inputs: Vec<ActionItemInput> = recap
        .action_items
        .iter()
        .map(|ai| {
            let owner_contact_id = ai
                .owner_hint
                .as_deref()
                .and_then(|hint| match_contact_id(&contacts, hint));
            ActionItemInput {
                text: ai.text.clone(),
                owner_contact_id,
                due: ai.due.clone(),
            }
        })
        .collect();

    db::replace_action_items(pool, ctx.call_id, &action_inputs).await?;

    let md = render_recap_md(&recap, &contacts, &action_inputs);
    tokio::fs::write(ctx.call_dir.join("recap.md"), md).await?;

    Ok(())
}

fn match_contact_id(contacts: &[db::Contact], hint: &str) -> Option<String> {
    let hint_lower = hint.trim().to_lowercase();
    if hint_lower.is_empty() {
        return None;
    }
    // Точное совпадение имени важнее частичного.
    if let Some(c) = contacts
        .iter()
        .find(|c| c.display_name.to_lowercase() == hint_lower)
    {
        return Some(c.id.clone());
    }
    contacts
        .iter()
        .find(|c| {
            let dn = c.display_name.to_lowercase();
            dn.contains(&hint_lower) || hint_lower.contains(&dn)
        })
        .map(|c| c.id.clone())
}

fn build_system_prompt(lang_detected: Option<&str>, known_speakers: Option<&str>) -> String {
    let lang_hint = lang_detected
        .map(|l| format!(" Output language: {l}."))
        .unwrap_or_default();
    let known_block = known_speakers
        .map(|s| {
            format!(
                "\n\nKnown participants (use their real names in summary/mom/action_items \
                 instead of raw speaker tags):\n{s}"
            )
        })
        .unwrap_or_default();
    format!(
        "You are a meeting recap assistant. Read the diarized transcript below \
         and return ONE valid JSON object (no markdown fences, no commentary) \
         matching this schema exactly:\n\
         {{\n\
           \"version\": 1,\n\
           \"summary\": string,\n\
           \"key_points\": string[],\n\
           \"mom\": string (Markdown, minutes of meeting),\n\
           \"action_items\": [{{ \"text\": string, \"owner_hint\": string|null, \"due\": string|null }}],\n\
           \"participants\": [{{ \"speaker_tag\": string, \"display_name\": string|null }}]\n\
         }}\n\
         Use 'owner_hint' to refer to action item owner by name as mentioned in the transcript. \
         Use 'speaker_tag' values exactly as they appear in the transcript ('owner', 'Speaker 0' и т.п.).\
         {lang_hint}{known_block}"
    )
}

/// Собирает «Known participants» блок для LLM-контекста: для каждой
/// подтверждённой привязки speaker_tag → contact выводит строку с display_name
/// + опц. org/role. Если привязок нет — None (блок не добавляется).
async fn build_known_speakers_block(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Option<String>, AppError> {
    let speakers = db::list_call_speakers(pool, call_id).await?;
    let confirmed: Vec<_> = speakers
        .iter()
        .filter(|s| s.confirmed && s.contact_id.is_some() && s.contact_display_name.is_some())
        .collect();
    if confirmed.is_empty() {
        return Ok(None);
    }

    // Подтянем дополнительный контекст (org/role) из contacts table.
    let contacts = db::list_contacts(pool).await?;
    let by_id: std::collections::HashMap<&str, &db::Contact> =
        contacts.iter().map(|c| (c.id.as_str(), c)).collect();

    let mut lines = Vec::new();
    for s in confirmed {
        let cid = s.contact_id.as_deref().unwrap_or("");
        let name = s.contact_display_name.as_deref().unwrap_or("");
        let extras = by_id
            .get(cid)
            .map(|c| {
                let mut bits = Vec::new();
                if let Some(role) = c.role.as_deref().filter(|s| !s.is_empty()) {
                    bits.push(role.to_string());
                }
                if let Some(org) = c.org.as_deref().filter(|s| !s.is_empty()) {
                    bits.push(org.to_string());
                }
                if bits.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", bits.join(", "))
                }
            })
            .unwrap_or_default();
        lines.push(format!("- {} = {}{}", s.speaker_tag, name, extras));
    }
    Ok(Some(lines.join("\n")))
}

fn render_recap_md(
    recap: &RecapJson,
    contacts: &[db::Contact],
    action_inputs: &[ActionItemInput],
) -> String {
    let mut out = String::new();
    out.push_str("# Recap\n\n");

    if !recap.summary.is_empty() {
        out.push_str(recap.summary.trim());
        out.push_str("\n\n");
    }

    if !recap.key_points.is_empty() {
        out.push_str("## Key Points\n\n");
        for kp in &recap.key_points {
            out.push_str(&format!("- {}\n", kp.trim()));
        }
        out.push('\n');
    }

    if !recap.mom.is_empty() {
        out.push_str("## MoM\n\n");
        out.push_str(recap.mom.trim());
        out.push_str("\n\n");
    }

    if !action_inputs.is_empty() {
        out.push_str("## Action Items\n\n");
        for (i, ai) in action_inputs.iter().enumerate() {
            let owner_label = ai
                .owner_contact_id
                .as_deref()
                .and_then(|id| contacts.iter().find(|c| c.id == id))
                .map(|c| c.display_name.clone())
                .or_else(|| recap.action_items.get(i).and_then(|r| r.owner_hint.clone()));
            let due_suffix = ai
                .due
                .as_deref()
                .map(|d| format!(" — до {d}"))
                .unwrap_or_default();
            match owner_label {
                Some(label) if !label.trim().is_empty() => {
                    out.push_str(&format!(
                        "- [ ] **{}** — {}{}\n",
                        label.trim(),
                        ai.text.trim(),
                        due_suffix
                    ));
                }
                _ => {
                    out.push_str(&format!("- [ ] {}{}\n", ai.text.trim(), due_suffix));
                }
            }
        }
        out.push('\n');
    }

    if !recap.participants.is_empty() {
        out.push_str("## Participants\n\n");
        for p in &recap.participants {
            let name = p
                .display_name
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(str::trim)
                .unwrap_or(&p.speaker_tag);
            out.push_str(&format!("- {} (`{}`)\n", name, p.speaker_tag));
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn contact(id: &str, name: &str) -> db::Contact {
        db::Contact {
            id: id.to_string(),
            display_name: name.to_string(),
            is_owner: false,
            org: None,
            role: None,
            attributes: Value::Object(serde_json::Map::new()),
            notes: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            identifiers: vec![],
        }
    }

    #[test]
    fn match_contact_id_exact_wins_over_partial() {
        let contacts = vec![contact("a", "Alice"), contact("b", "Alice Smith")];
        assert_eq!(match_contact_id(&contacts, "alice").as_deref(), Some("a"));
    }

    #[test]
    fn match_contact_id_partial_handles_substring() {
        let contacts = vec![contact("b", "Bob Johnson")];
        assert_eq!(match_contact_id(&contacts, "bob").as_deref(), Some("b"));
        assert_eq!(
            match_contact_id(&contacts, "Bob Johnson Jr").as_deref(),
            Some("b")
        );
        assert_eq!(match_contact_id(&contacts, "Carol"), None);
    }

    #[test]
    fn render_recap_md_skips_empty_sections() {
        let recap = RecapJson {
            version: Some(1),
            summary: "Brief".into(),
            key_points: vec![],
            mom: String::new(),
            action_items: vec![],
            participants: vec![],
        };
        let md = render_recap_md(&recap, &[], &[]);
        assert!(md.contains("# Recap"));
        assert!(md.contains("Brief"));
        assert!(!md.contains("## Key Points"));
        assert!(!md.contains("## MoM"));
        assert!(!md.contains("## Action Items"));
    }

    #[test]
    fn render_recap_md_renders_action_items_with_owner_label() {
        let contacts = vec![contact("a", "Alice")];
        let recap = RecapJson {
            version: Some(1),
            summary: "Discussed Q3.".into(),
            key_points: vec!["plan reviewed".into()],
            mom: String::new(),
            action_items: vec![RecapActionItem {
                text: "send draft".into(),
                owner_hint: Some("Alice".into()),
                due: Some("2026-06-01".into()),
            }],
            participants: vec![RecapParticipant {
                speaker_tag: "Speaker 0".into(),
                display_name: Some("Alice".into()),
                contact_id: None,
            }],
        };
        let action_inputs = vec![ActionItemInput {
            text: "send draft".into(),
            owner_contact_id: Some("a".into()),
            due: Some("2026-06-01".into()),
        }];
        let md = render_recap_md(&recap, &contacts, &action_inputs);
        assert!(md.contains("## Action Items"));
        assert!(md.contains("**Alice** — send draft — до 2026-06-01"));
        assert!(md.contains("## Participants"));
        assert!(md.contains("Alice (`Speaker 0`)"));
    }
}
