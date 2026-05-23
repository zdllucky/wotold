//! [M14 T-13] LLM-as-judge G-Eval scoring infrastructure.
//!
//! ## Зачем
//!
//! T-12 golden harness ловит **structural** regressions (parser / promote /
//! strip / dedup logic). Но если cloud prompt начинает выдавать менее
//! coherent summaries (model upgrade, temperature drift), structural diff
//! не сработает — output JSON shape валидный, content деградирует.
//!
//! T-13 (G-Eval, Liu et al. NLP-2024) добавляет **qualitative scoring**
//! через LLM-as-judge: Sonnet/Anthropic оценивает summary по 4 dimensions:
//!
//! 1. **Coherence** — flows logically, no contradictions
//! 2. **Faithfulness** — strictly derived from transcript, no fabrication
//! 3. **Relevance** — captures key decisions / actions / open_questions
//! 4. **Conciseness** — no fluff, no redundancy
//!
//! Output schema: `{ coherence, faithfulness, relevance, conciseness,
//! justification }` где scores 1-5.
//!
//! ## Phase A scope (foundation)
//!
//! - Prompt builder + parser + LLM invocation wrapper
//! - Cloud-only judge (Sonnet через `AnthropicProvider`)
//! - Returns typed `EvalScores`
//! - Tests с MockProvider — no real LLM в CI
//!
//! ## Backlog (M14.5)
//!
//! - DB `summary_eval_scores` table + migration
//! - Tauri command для ad-hoc evaluation
//! - Auto-eval (Labs opt-in) after every generation
//! - Multi-sample averaging для bias mitigation (G-Eval §4)
//! - UI display в CallDetailPage / Settings analytics

#![allow(dead_code)] // [M14 T-13] Foundation. Production usage (Tauri command,
                     // DB persistence) — backlog M14.5.

use serde::Deserialize;

use crate::providers::llm::{LlmProvider, LlmRequest};
use crate::AppError;

const G_EVAL_MAX_TOKENS: u32 = 512;
/// Transcript head для context. Полный transcript обычно превышает context
/// window judge'а — даём начало для grounding (matches T-08 / T-17 helpers).
pub(crate) const TRANSCRIPT_HEAD_CHARS: usize = 12_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvalScores {
    /// Logical flow, internal consistency. 1=poor, 5=excellent.
    pub coherence: u8,
    /// Strict derivability from transcript (no fabricated facts).
    pub faithfulness: u8,
    /// Key decisions / action items / open questions captured.
    pub relevance: u8,
    /// No fluff, no redundancy, information-dense.
    pub conciseness: u8,
    /// 2-4 sentences citing specific examples — provides interpretability.
    pub justification: String,
}

impl EvalScores {
    /// Mean of 4 dimensions. Easy aggregate metric для dashboards / sorting.
    pub fn average(&self) -> f32 {
        (self.coherence + self.faithfulness + self.relevance + self.conciseness) as f32 / 4.0
    }
}

#[derive(Debug, Deserialize)]
struct EvalJson {
    coherence: u8,
    faithfulness: u8,
    relevance: u8,
    conciseness: u8,
    #[serde(default)]
    justification: Option<String>,
}

/// Build judge prompt с 4D rubric. Output language указывается через
/// `lang_detected` (default 'ru'). Scores 1-5 integers; justification
/// 2-4 sentences in `lang_detected`.
pub(crate) fn build_judge_prompt(lang_detected: Option<&str>) -> String {
    let lang = lang_detected.unwrap_or("ru");
    format!(
        "You are an impartial summary quality judge. Rate the SUMMARY below against the SOURCE TRANSCRIPT on 4 dimensions (1=poor, 5=excellent). Output language for justification: {lang}.\n\
\n\
## DIMENSIONS\n\
\n\
1. **Coherence** — Does the summary flow logically? No contradictions, clear narrative?\n\
2. **Faithfulness** — Are ALL claims strictly derivable from transcript? No fabricated facts / dates / commitments?\n\
3. **Relevance** — Does it capture the KEY decisions, action items, open questions from transcript?\n\
4. **Conciseness** — No fluff, no redundancy? Direct and information-dense?\n\
\n\
## RULES\n\
\n\
1. Score each dimension 1-5 (integers only). Be strict — 5 means near-perfect.\n\
2. Justification: 2-4 sentences citing specific examples from transcript/summary.\n\
3. Output ONLY ONE JSON object matching schema below. No prose, no markdown fences.\n\
\n\
## SCHEMA\n\
\n\
{{\n\
  \"coherence\": 1..5,\n\
  \"faithfulness\": 1..5,\n\
  \"relevance\": 1..5,\n\
  \"conciseness\": 1..5,\n\
  \"justification\": string\n\
}}\n\
\n\
Output ONLY the JSON object."
    )
}

/// UTF-8 safe transcript head truncation. Matches existing pattern from
/// `classifier::extract_classifier_head` / `title_regen::extract_transcript_head`.
pub(crate) fn extract_transcript_head(transcript_md: &str, max_chars: usize) -> &str {
    if transcript_md.chars().count() <= max_chars {
        return transcript_md;
    }
    let cutoff = transcript_md
        .char_indices()
        .nth(max_chars)
        .map(|(b, _)| b)
        .unwrap_or(transcript_md.len());
    &transcript_md[..cutoff]
}

/// Parse judge JSON response → typed EvalScores. Clamps out-of-range scores
/// (0 → 1, 6+ → 5) — defensive против garbage LLM output. Missing
/// justification → empty string.
pub(crate) fn parse_eval_response(json_value: serde_json::Value) -> Result<EvalScores, AppError> {
    let parsed: EvalJson = serde_json::from_value(json_value)
        .map_err(|e| AppError::Other(format!("eval JSON shape: {e}")))?;
    let clamp = |n: u8| n.clamp(1, 5);
    Ok(EvalScores {
        coherence: clamp(parsed.coherence),
        faithfulness: clamp(parsed.faithfulness),
        relevance: clamp(parsed.relevance),
        conciseness: clamp(parsed.conciseness),
        justification: parsed.justification.unwrap_or_default(),
    })
}

/// Dispatch LLM judge call + parse. Cloud usage — `AnthropicProvider::Managed`.
/// На LLM error → AppError; caller handles (показывает toast / logs).
pub(crate) async fn evaluate_summary(
    provider: &dyn LlmProvider,
    transcript_md: &str,
    summary_json: &serde_json::Value,
    lang_detected: Option<&str>,
) -> Result<EvalScores, AppError> {
    let transcript_head = extract_transcript_head(transcript_md, TRANSCRIPT_HEAD_CHARS);
    let summary_str = serde_json::to_string_pretty(summary_json)
        .map_err(|e| AppError::Other(format!("g-eval summary serialize: {e}")))?;
    let input = format!(
        "## SOURCE TRANSCRIPT\n\n{transcript_head}\n\n## SUMMARY (under evaluation)\n\n{summary_str}\n\nRate per RULES and OUTPUT only the JSON object."
    );
    let request = LlmRequest {
        model: None,
        system: build_judge_prompt(lang_detected),
        input,
        max_tokens: Some(G_EVAL_MAX_TOKENS),
        grammar: None,
    };
    let json_value = provider
        .generate(request)
        .await
        .map_err(|e| AppError::Other(format!("g-eval llm: {e}")))?;
    parse_eval_response(json_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    use crate::providers::llm::LlmError;

    struct MockProvider {
        responses: Mutex<Vec<Result<serde_json::Value, LlmError>>>,
        captured: Mutex<Vec<LlmRequest>>,
    }
    impl MockProvider {
        fn new(responses: Vec<Result<serde_json::Value, LlmError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                captured: Mutex::new(Vec::new()),
            }
        }
        fn captured(&self) -> Vec<LlmRequest> {
            self.captured.lock().unwrap().clone()
        }
    }
    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn generate(&self, request: LlmRequest) -> Result<serde_json::Value, LlmError> {
            self.captured.lock().unwrap().push(request);
            let mut guard = self.responses.lock().unwrap();
            if guard.is_empty() {
                return Err(LlmError::Provider("no scripted response".into()));
            }
            guard.remove(0)
        }
    }

    #[test]
    fn build_judge_prompt_includes_4_dimensions() {
        let p = build_judge_prompt(Some("en"));
        assert!(p.contains("Coherence"));
        assert!(p.contains("Faithfulness"));
        assert!(p.contains("Relevance"));
        assert!(p.contains("Conciseness"));
        assert!(p.contains("Output language for justification: en"));
        assert!(p.contains("1..5"));
        assert!(p.contains("justification"));

        let p_default = build_judge_prompt(None);
        assert!(p_default.contains("justification: ru"));
    }

    #[test]
    fn extract_transcript_head_respects_max_chars() {
        assert_eq!(extract_transcript_head("abcdefgh", 4), "abcd");
        assert_eq!(extract_transcript_head("short", 100), "short");
        // Кириллица: 5 chars cutoff.
        assert_eq!(extract_transcript_head("абвгдежзик", 5), "абвгд");
    }

    #[test]
    fn parse_eval_response_valid_returns_scores() {
        let v = serde_json::json!({
            "coherence": 4,
            "faithfulness": 5,
            "relevance": 3,
            "conciseness": 4,
            "justification": "Solid recap; one missing action item."
        });
        let r = parse_eval_response(v).unwrap();
        assert_eq!(r.coherence, 4);
        assert_eq!(r.faithfulness, 5);
        assert_eq!(r.relevance, 3);
        assert_eq!(r.conciseness, 4);
        assert!(r.justification.contains("Solid"));
    }

    #[test]
    fn parse_eval_response_clamps_out_of_range_scores() {
        let v = serde_json::json!({
            "coherence": 0,
            "faithfulness": 5,
            "relevance": 6,
            "conciseness": 1,
            "justification": "test clamp"
        });
        let r = parse_eval_response(v).unwrap();
        assert_eq!(r.coherence, 1, "0 should clamp to 1");
        assert_eq!(r.relevance, 5, "6 should clamp to 5");
    }

    #[test]
    fn parse_eval_response_missing_justification_defaults_empty() {
        let v = serde_json::json!({
            "coherence": 3,
            "faithfulness": 3,
            "relevance": 3,
            "conciseness": 3
        });
        let r = parse_eval_response(v).unwrap();
        assert_eq!(r.justification, "");
    }

    #[test]
    fn parse_eval_response_garbage_json_returns_error() {
        let v = serde_json::json!({ "wrong_keys": true });
        let err = parse_eval_response(v).unwrap_err();
        assert!(err.to_string().contains("eval JSON shape"));
    }

    #[test]
    fn eval_scores_average_returns_mean() {
        let s = EvalScores {
            coherence: 4,
            faithfulness: 5,
            relevance: 3,
            conciseness: 4,
            justification: String::new(),
        };
        assert!((s.average() - 4.0).abs() < 1e-6);

        let s_high = EvalScores {
            coherence: 5,
            faithfulness: 5,
            relevance: 5,
            conciseness: 5,
            justification: String::new(),
        };
        assert!((s_high.average() - 5.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn evaluate_summary_llm_success_returns_scores() {
        let response = serde_json::json!({
            "coherence": 5,
            "faithfulness": 4,
            "relevance": 5,
            "conciseness": 4,
            "justification": "Captures all key decisions; one redundant phrase."
        });
        let mock = MockProvider::new(vec![Ok(response)]);
        let summary = serde_json::json!({
            "title": "Test",
            "summary": "Brief sync",
            "action_items": []
        });
        let scores = evaluate_summary(&mock, "transcript text", &summary, Some("en"))
            .await
            .unwrap();
        assert_eq!(scores.coherence, 5);
        assert_eq!(scores.faithfulness, 4);
        assert!((scores.average() - 4.5).abs() < 1e-6);

        // Verify request shape — system contains rubric, input contains both
        // transcript и summary.
        let captured = mock.captured();
        assert_eq!(captured.len(), 1);
        assert!(captured[0].system.contains("Coherence"));
        assert!(captured[0].input.contains("SOURCE TRANSCRIPT"));
        assert!(captured[0].input.contains("SUMMARY"));
        assert!(captured[0].input.contains("transcript text"));
        assert!(captured[0].input.contains("\"title\""));
    }

    #[tokio::test]
    async fn evaluate_summary_llm_failure_returns_error() {
        let mock = MockProvider::new(vec![Err(LlmError::Provider("simulated crash".into()))]);
        let summary = serde_json::json!({});
        let err = evaluate_summary(&mock, "transcript", &summary, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("g-eval llm"));
    }
}
