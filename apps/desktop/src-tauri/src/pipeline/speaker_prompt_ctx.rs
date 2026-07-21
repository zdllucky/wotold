//! [F2] Унификация спикеров одного контакта на этапе сборки LLM-промпта.
//!
//! Несколько diarization-тегов (`speaker:0`, `speaker:3`) могут быть
//! подтверждены на один contact. Транскрипт при этом хранит сырые теги —
//! LLM видит «двух разных людей». Этот модуль переписывает заголовки
//! `**{tag}** [MM:SS]:` на display_name подтверждённого контакта **только
//! в тексте промпта** (transcript.md на диске не трогается) и строит
//! person-level «Known participants» блок без дублей по контакту.
//!
//! ВАЖНО: перезаписанный транскрипт обязан попадать и в evidence-валидатор
//! (`persist_recap_from_json(transcript_md=…)`) — quotes могут пересекать
//! заголовок, валидировать надо против того текста, что видела модель.

use sqlx::SqlitePool;

use crate::{db, AppError};

/// Одна подтверждённая привязка speaker_tag → contact.
#[derive(Debug, Clone)]
pub(crate) struct ConfirmedSpeaker {
    pub speaker_tag: String,
    pub contact_id: String,
    pub display_name: String,
    /// " (PM, Acme)" | "" — готовый суффикс для Known-блока.
    pub role_org_suffix: String,
}

/// Контекст спикеров для сборки промпта.
pub(crate) struct SpeakerPromptCtx {
    pub confirmed: Vec<ConfirmedSpeaker>,
    /// Есть ли в звонке теги без подтверждённой привязки (кроме owner).
    pub has_unconfirmed: bool,
}

/// Переписывает заголовки `**{tag}** [MM:SS]:` на `**{display_name}** …` для
/// подтверждённых тегов. Матчится только полная строка-заголовок (mirror
/// `chunker::is_speaker_header_line`): начинается с `**`, тег закрыт `**`,
/// дальше ` [` и `]:`. Тело сегментов и неподтверждённые теги не трогаются.
/// `speaker:1` не матчит `speaker:10` — сравнение по полному тегу до `**`.
pub(crate) fn rewrite_speaker_headers(
    transcript_md: &str,
    confirmed: &[ConfirmedSpeaker],
) -> String {
    if confirmed.is_empty() {
        return transcript_md.to_string();
    }
    let mut out = String::with_capacity(transcript_md.len());
    for (i, line) in transcript_md.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&rewrite_header_line(line, confirmed));
    }
    if transcript_md.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn rewrite_header_line(line: &str, confirmed: &[ConfirmedSpeaker]) -> String {
    let Some(rest) = line.strip_prefix("**") else {
        return line.to_string();
    };
    let Some(end) = rest.find("**") else {
        return line.to_string();
    };
    let tag = &rest[..end];
    let tail = &rest[end + 2..];
    // Заголовок = `**tag** [MM:SS]:` — хвост начинается с " [" и содержит "]:".
    if !tail.starts_with(" [") || !tail.contains("]:") {
        return line.to_string();
    }
    match confirmed.iter().find(|c| c.speaker_tag == tag) {
        Some(c) => format!("**{}**{}", c.display_name, tail),
        None => line.to_string(),
    }
}

/// Person-level «Known participants» блок: одна строка на контакт, даже если
/// на него подтверждено несколько тегов. None — если подтверждённых нет.
pub(crate) fn build_known_participants_block(ctx: &SpeakerPromptCtx) -> Option<String> {
    if ctx.confirmed.is_empty() {
        return None;
    }
    let mut seen: Vec<&str> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for c in &ctx.confirmed {
        if seen.contains(&c.contact_id.as_str()) {
            continue;
        }
        seen.push(&c.contact_id);
        lines.push(format!("- {}{}", c.display_name, c.role_org_suffix));
    }
    if ctx.has_unconfirmed {
        lines.push("Other speakers appear as raw speaker tags (not yet identified).".to_string());
    }
    Some(lines.join("\n"))
}

/// Загружает подтверждённые привязки спикеров звонка + org/role контактов.
pub(crate) async fn load_speaker_prompt_ctx(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<SpeakerPromptCtx, AppError> {
    let speakers = db::list_call_speakers(pool, call_id).await?;
    let has_unconfirmed = speakers
        .iter()
        .any(|s| !s.confirmed || s.contact_id.is_none());

    let confirmed_rows: Vec<_> = speakers
        .into_iter()
        .filter(|s| s.confirmed && s.contact_id.is_some() && s.contact_display_name.is_some())
        .collect();
    if confirmed_rows.is_empty() {
        return Ok(SpeakerPromptCtx {
            confirmed: Vec::new(),
            has_unconfirmed,
        });
    }

    let contacts = db::list_contacts(pool).await?;
    let suffix_by_id: std::collections::HashMap<&str, String> = contacts
        .iter()
        .map(|c| {
            let mut bits = Vec::new();
            if let Some(role) = c.role.as_deref().filter(|s| !s.is_empty()) {
                bits.push(role.to_string());
            }
            if let Some(org) = c.org.as_deref().filter(|s| !s.is_empty()) {
                bits.push(org.to_string());
            }
            let suffix = if bits.is_empty() {
                String::new()
            } else {
                format!(" ({})", bits.join(", "))
            };
            (c.id.as_str(), suffix)
        })
        .collect();

    let confirmed = confirmed_rows
        .into_iter()
        .map(|s| {
            let contact_id = s.contact_id.unwrap_or_default();
            let display_name = s.contact_display_name.unwrap_or_default();
            let role_org_suffix = suffix_by_id
                .get(contact_id.as_str())
                .cloned()
                .unwrap_or_default();
            ConfirmedSpeaker {
                speaker_tag: s.speaker_tag,
                contact_id,
                display_name,
                role_org_suffix,
            }
        })
        .collect();

    Ok(SpeakerPromptCtx {
        confirmed,
        has_unconfirmed,
    })
}

/// Convenience: (rewritten_transcript_md, known_participants_block).
/// При отсутствии подтверждённых привязок транскрипт возвращается как есть.
pub(crate) async fn build_prompt_transcript(
    pool: &SqlitePool,
    call_id: &str,
    transcript_md: &str,
) -> Result<(String, Option<String>), AppError> {
    let ctx = load_speaker_prompt_ctx(pool, call_id).await?;
    let rewritten = rewrite_speaker_headers(transcript_md, &ctx.confirmed);
    let block = build_known_participants_block(&ctx);
    Ok((rewritten, block))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::insert_recording;
    use crate::db::test_support::fresh_db;

    fn cs(tag: &str, contact_id: &str, name: &str, suffix: &str) -> ConfirmedSpeaker {
        ConfirmedSpeaker {
            speaker_tag: tag.to_string(),
            contact_id: contact_id.to_string(),
            display_name: name.to_string(),
            role_org_suffix: suffix.to_string(),
        }
    }

    #[test]
    fn rewrites_two_tags_of_one_contact_to_same_name() {
        let md =
            "# Transcript\n\n**speaker:0** [0:00]:\nhi\n\n**speaker:3** [1:05]:\nyes as I said\n";
        let confirmed = vec![
            cs("speaker:0", "c1", "Alice", ""),
            cs("speaker:3", "c1", "Alice", ""),
        ];
        let out = rewrite_speaker_headers(md, &confirmed);
        assert!(out.contains("**Alice** [0:00]:\nhi"));
        assert!(out.contains("**Alice** [1:05]:\nyes as I said"));
        assert!(!out.contains("speaker:0"));
        assert!(!out.contains("speaker:3"));
    }

    #[test]
    fn does_not_rewrite_speaker_10_when_speaker_1_confirmed() {
        let md = "**speaker:1** [0:00]:\na\n\n**speaker:10** [0:10]:\nb\n";
        let confirmed = vec![cs("speaker:1", "c1", "Alice", "")];
        let out = rewrite_speaker_headers(md, &confirmed);
        assert!(out.contains("**Alice** [0:00]:"));
        assert!(
            out.contains("**speaker:10** [0:10]:"),
            "speaker:10 must stay raw"
        );
    }

    #[test]
    fn unconfirmed_tags_stay_raw() {
        let md = "**Speaker 0** [0:00]:\na\n\n**Speaker 1** [0:10]:\nb\n";
        let confirmed = vec![cs("Speaker 0", "c1", "Alice", "")];
        let out = rewrite_speaker_headers(md, &confirmed);
        assert!(out.contains("**Alice** [0:00]:"));
        assert!(out.contains("**Speaker 1** [0:10]:"));
    }

    #[test]
    fn body_text_mentioning_tag_is_untouched() {
        let md = "**speaker:1** [0:00]:\nplease ping speaker:1 about **speaker:1** later\n";
        let confirmed = vec![cs("speaker:1", "c1", "Alice", "")];
        let out = rewrite_speaker_headers(md, &confirmed);
        assert!(out.starts_with("**Alice** [0:00]:"));
        // Тело не тронуто — включая псевдо-bold без хвоста заголовка.
        assert!(out.contains("please ping speaker:1 about **speaker:1** later"));
    }

    #[test]
    fn owner_tag_rewritten_to_owner_name() {
        let md = "**owner** [0:00]:\nhello\n";
        let confirmed = vec![cs("owner", "c-me", "Damir", " (CEO)")];
        let out = rewrite_speaker_headers(md, &confirmed);
        assert!(out.contains("**Damir** [0:00]:"));
    }

    #[test]
    fn empty_confirmed_returns_transcript_unchanged() {
        let md = "**speaker:0** [0:00]:\nhi\n";
        assert_eq!(rewrite_speaker_headers(md, &[]), md);
        let ctx = SpeakerPromptCtx {
            confirmed: vec![],
            has_unconfirmed: true,
        };
        assert!(build_known_participants_block(&ctx).is_none());
    }

    #[test]
    fn known_block_dedups_by_contact_and_notes_unconfirmed() {
        let ctx = SpeakerPromptCtx {
            confirmed: vec![
                cs("speaker:0", "c1", "Alice", " (PM, Acme)"),
                cs("speaker:3", "c1", "Alice", " (PM, Acme)"),
                cs("owner", "c2", "Damir", ""),
            ],
            has_unconfirmed: true,
        };
        let block = build_known_participants_block(&ctx).unwrap();
        let alice_lines = block.matches("- Alice (PM, Acme)").count();
        assert_eq!(alice_lines, 1, "один контакт = одна строка: {block}");
        assert!(block.contains("- Damir"));
        assert!(block.contains("Other speakers appear as raw speaker tags"));
    }

    #[test]
    fn known_block_without_unconfirmed_has_no_trailing_note() {
        let ctx = SpeakerPromptCtx {
            confirmed: vec![cs("owner", "c2", "Damir", "")],
            has_unconfirmed: false,
        };
        let block = build_known_participants_block(&ctx).unwrap();
        assert!(!block.contains("Other speakers"));
    }

    #[tokio::test]
    async fn load_and_build_prompt_transcript_end_to_end() {
        let db = fresh_db().await;
        let call = insert_recording(&db.pool, "managed").await.unwrap();

        // Контакт Alice (role+org) + два подтверждённых тега на неё.
        let alice = {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO contacts (id, display_name, is_owner, role, org, attributes, created_at, updated_at)
                 VALUES (?1, 'Alice', 0, 'PM', 'Acme', '{}', ?2, ?2)",
            )
            .bind(&id)
            .bind(&now)
            .execute(&db.pool)
            .await
            .unwrap();
            id
        };
        for tag in ["speaker:0", "speaker:3"] {
            let sid = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO call_speakers (id, call_id, speaker_tag, contact_id, confirmed)
                 VALUES (?1, ?2, ?3, ?4, 1)",
            )
            .bind(&sid)
            .bind(&call.id)
            .bind(tag)
            .bind(&alice)
            .execute(&db.pool)
            .await
            .unwrap();
        }
        // Неподтверждённый третий спикер.
        sqlx::query(
            "INSERT INTO call_speakers (id, call_id, speaker_tag, confirmed)
             VALUES (?1, ?2, 'speaker:5', 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&call.id)
        .execute(&db.pool)
        .await
        .unwrap();

        let md = "**speaker:0** [0:00]:\nhi\n\n**speaker:3** [0:20]:\nagain me\n\n**speaker:5** [0:40]:\nguest\n";
        let (rewritten, block) = build_prompt_transcript(&db.pool, &call.id, md)
            .await
            .unwrap();
        assert!(rewritten.contains("**Alice** [0:00]:"));
        assert!(rewritten.contains("**Alice** [0:20]:"));
        assert!(
            rewritten.contains("**speaker:5** [0:40]:"),
            "unconfirmed stays"
        );
        let block = block.unwrap();
        assert_eq!(block.matches("- Alice (PM, Acme)").count(), 1);
        assert!(block.contains("Other speakers appear as raw speaker tags"));
    }
}
