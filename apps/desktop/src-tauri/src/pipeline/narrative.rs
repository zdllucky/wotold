//! [recap-rich] Нарратив-минутки — связный markdown-протокол встречи.
//!
//! Отдельный focused-вызов ПОСЛЕ structured reduce (паттерн «extract → write»):
//! слабая local-модель устойчивее пишет прозу из уже готовой структуры, чем
//! пытается выдать структуру + нарратив в одном JSON. Best-effort: пустая
//! строка при любой ошибке → секция просто опускается в рендере.
//!
//! Markdown прячем внутрь JSON-поля `{ "narrative": "..." }` — так вызов идёт
//! тем же schema-constrained путём (`gbnf::generate_with_schema`), что и
//! остальные (грамматика форсит валидный объект, парсер извлекает строку).

use crate::pipeline::gbnf;
use crate::pipeline::llm_schemas;
use crate::providers::llm::{LlmProvider, LlmRequest};

const NARRATIVE_MAX_TOKENS: u32 = 1024;
/// Голова транскрипта для контекста нарратива (символов). Структура уже несёт
/// суть; транскрипт — для тона/деталей.
const TRANSCRIPT_HEAD_CHARS: usize = 8_000;

pub(crate) fn build_narrative_prompt(lang: &str) -> String {
    format!(
        "OUTPUT LANGUAGE = {lang}. Пиши связный протокол встречи (минутки) в markdown НА ЯЗЫКЕ {lang}.\n\
\n\
Тебе дан structured JSON рекапа (summary, key_points, decisions, action_items, open_questions, topics) и голова транскрипта. Напиши 2-4 абзаца связного текста: о чём была встреча, как развивался разговор, к чему пришли и что дальше. Плавно, по-человечески — как заметки хорошего секретаря, а не список.\n\
\n\
ПРАВИЛА:\n\
1. Только факты из данных/транскрипта — ничего не выдумывай.\n\
2. Имена людей вместо 'Speaker 0' / тегов.\n\
3. Выделяй **жирным** (markdown) имена участников, даты и ключевые цифры — не более 1-2 выделений на абзац.\n\
4. Без заголовков-секций и без буллетов — только связные абзацы.\n\
5. Верни РОВНО ОДИН JSON-объект: {{ \"narrative\": \"...markdown-абзацы...\" }}. Никакого текста вне JSON.\n\
6. Язык всего текста — {lang}."
    )
}

/// Сгенерировать нарратив. Best-effort — пустая строка при любой ошибке
/// (нет провайдера, LLM error, нет поля). Caller опускает секцию если пусто.
pub(crate) async fn generate_narrative(
    provider: &dyn LlmProvider,
    summary_json: &serde_json::Value,
    transcript_md: &str,
    lang_detected: Option<&str>,
) -> String {
    let lang = lang_detected.unwrap_or("ru");
    let head: String = transcript_md.chars().take(TRANSCRIPT_HEAD_CHARS).collect();
    let input = serde_json::json!({ "summary": summary_json, "transcript_head": head });
    let input_str = match serde_json::to_string(&input) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("narrative: serialize input failed (skip): {e}");
            return String::new();
        }
    };
    let request = LlmRequest {
        model: None,
        system: build_narrative_prompt(lang),
        input: input_str,
        max_tokens: Some(NARRATIVE_MAX_TOKENS),
        grammar: None,
        json_schema: None,
    };
    match gbnf::generate_with_schema(provider, request, llm_schemas::NARRATIVE_JSON_SCHEMA).await {
        Ok(v) => v
            .get("narrative")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        Err(e) => {
            log::warn!("narrative: LLM failed (skip): {e}");
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    use crate::providers::llm::LlmError;

    struct MockProvider {
        response: Mutex<Option<Result<serde_json::Value, LlmError>>>,
    }
    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn generate(&self, _request: LlmRequest) -> Result<serde_json::Value, LlmError> {
            self.response
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Err(LlmError::Provider("no response".into())))
        }
    }

    #[test]
    fn narrative_prompt_has_lang_and_json_rule() {
        let p = build_narrative_prompt("ru");
        assert!(p.contains("OUTPUT LANGUAGE = ru"));
        assert!(p.contains("\"narrative\""));
        assert!(p.contains("markdown"));
    }

    #[tokio::test]
    async fn generate_narrative_extracts_field() {
        let mock = MockProvider {
            response: Mutex::new(Some(Ok(
                serde_json::json!({ "narrative": "  Встреча прошла хорошо.  " }),
            ))),
        };
        let out = generate_narrative(&mock, &serde_json::json!({}), "stub", Some("ru")).await;
        assert_eq!(out, "Встреча прошла хорошо.");
    }

    #[tokio::test]
    async fn generate_narrative_empty_on_error() {
        let mock = MockProvider {
            response: Mutex::new(Some(Err(LlmError::Provider("boom".into())))),
        };
        let out = generate_narrative(&mock, &serde_json::json!({}), "stub", Some("ru")).await;
        assert_eq!(out, "");
    }
}
