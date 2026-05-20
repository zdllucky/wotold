//! Слияние сигналов voice embedding + LLM hint (#25 / M3.4).
//!
//! Для каждого `speaker_tag` берём top embedding-кандидата и LLM-кандидата.
//! Если оба указывают на одного контакта → `source = both`, score усреднён.
//! Если только один источник → `source = embedding` или `llm`.
//! Если оба разошлись → выбираем тот, у кого выше score (с лёгким bias на
//! embedding — биометрия более robust, чем lexical hint).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::matching::MatchCandidate;

/// Источник сигнала для UI «откуда подсказка».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionSource {
    Embedding,
    Llm,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergedSuggestion {
    pub speaker_tag: String,
    pub contact_id: String,
    pub display_name: String,
    pub score: f32,
    pub source: SuggestionSource,
}

/// Bias embedding score на сколько перевешивает LLM при равенстве. Биометрия
/// устойчивее к подвохам типа «модель угадала имя из контекста чужого разговора».
const EMBEDDING_BIAS: f32 = 0.05;

pub fn merge(
    embedding: HashMap<String, Vec<MatchCandidate>>,
    llm: HashMap<String, MatchCandidate>,
) -> Vec<MergedSuggestion> {
    let speakers: HashSet<String> = embedding.keys().chain(llm.keys()).cloned().collect();
    let mut out: Vec<MergedSuggestion> = speakers
        .into_iter()
        .filter_map(|tag| merge_one(&tag, embedding.get(&tag), llm.get(&tag)))
        .collect();
    out.sort_by(|a, b| a.speaker_tag.cmp(&b.speaker_tag));
    out
}

fn merge_one(
    tag: &str,
    embedding: Option<&Vec<MatchCandidate>>,
    llm: Option<&MatchCandidate>,
) -> Option<MergedSuggestion> {
    let emb_top = embedding.and_then(|v| v.first());

    match (emb_top, llm) {
        (None, None) => None,
        (Some(e), None) => Some(MergedSuggestion {
            speaker_tag: tag.to_string(),
            contact_id: e.contact_id.clone(),
            display_name: e.display_name.clone(),
            score: e.score,
            source: SuggestionSource::Embedding,
        }),
        (None, Some(l)) => Some(MergedSuggestion {
            speaker_tag: tag.to_string(),
            contact_id: l.contact_id.clone(),
            display_name: l.display_name.clone(),
            score: l.score,
            source: SuggestionSource::Llm,
        }),
        (Some(e), Some(l)) if e.contact_id == l.contact_id => Some(MergedSuggestion {
            speaker_tag: tag.to_string(),
            contact_id: e.contact_id.clone(),
            display_name: e.display_name.clone(),
            score: ((e.score + l.score) / 2.0).clamp(0.0, 1.0),
            source: SuggestionSource::Both,
        }),
        (Some(e), Some(l)) => {
            // Разошлись — берём с более высоким score, embedding с bias.
            if e.score + EMBEDDING_BIAS >= l.score {
                Some(MergedSuggestion {
                    speaker_tag: tag.to_string(),
                    contact_id: e.contact_id.clone(),
                    display_name: e.display_name.clone(),
                    score: e.score,
                    source: SuggestionSource::Embedding,
                })
            } else {
                Some(MergedSuggestion {
                    speaker_tag: tag.to_string(),
                    contact_id: l.contact_id.clone(),
                    display_name: l.display_name.clone(),
                    score: l.score,
                    source: SuggestionSource::Llm,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, name: &str, score: f32) -> MatchCandidate {
        MatchCandidate {
            contact_id: id.into(),
            display_name: name.into(),
            score,
        }
    }

    #[test]
    fn merge_returns_empty_for_no_signals() {
        let out = merge(HashMap::new(), HashMap::new());
        assert!(out.is_empty());
    }

    #[test]
    fn embedding_only_passes_through() {
        let mut emb = HashMap::new();
        emb.insert("Speaker 0".into(), vec![cand("c1", "Alice", 0.9)]);
        let out = merge(emb, HashMap::new());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, SuggestionSource::Embedding);
        assert_eq!(out[0].contact_id, "c1");
    }

    #[test]
    fn llm_only_passes_through() {
        let mut llm = HashMap::new();
        llm.insert("Speaker 0".into(), cand("c1", "Alice", 0.8));
        let out = merge(HashMap::new(), llm);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, SuggestionSource::Llm);
    }

    #[test]
    fn agreement_averages_scores_and_marks_both() {
        let mut emb = HashMap::new();
        emb.insert("Speaker 0".into(), vec![cand("c1", "Alice", 0.9)]);
        let mut llm = HashMap::new();
        llm.insert("Speaker 0".into(), cand("c1", "Alice", 0.7));
        let out = merge(emb, llm);
        assert_eq!(out[0].source, SuggestionSource::Both);
        assert!((out[0].score - 0.8).abs() < 1e-6);
    }

    #[test]
    fn disagreement_picks_higher_with_embedding_bias() {
        // Embedding 0.6 + bias 0.05 = 0.65 > LLM 0.6 → embedding wins.
        let mut emb = HashMap::new();
        emb.insert("Speaker 0".into(), vec![cand("c1", "Alice", 0.6)]);
        let mut llm = HashMap::new();
        llm.insert("Speaker 0".into(), cand("c2", "Bob", 0.6));
        let out = merge(emb, llm);
        assert_eq!(out[0].contact_id, "c1");
        assert_eq!(out[0].source, SuggestionSource::Embedding);
    }

    #[test]
    fn disagreement_picks_llm_when_clearly_higher() {
        let mut emb = HashMap::new();
        emb.insert("Speaker 0".into(), vec![cand("c1", "Alice", 0.5)]);
        let mut llm = HashMap::new();
        llm.insert("Speaker 0".into(), cand("c2", "Bob", 0.9));
        let out = merge(emb, llm);
        assert_eq!(out[0].contact_id, "c2");
        assert_eq!(out[0].source, SuggestionSource::Llm);
    }

    #[test]
    fn merges_multiple_speakers_sorted_by_tag() {
        let mut emb = HashMap::new();
        emb.insert("Speaker 1".into(), vec![cand("c2", "Bob", 0.8)]);
        let mut llm = HashMap::new();
        llm.insert("Speaker 0".into(), cand("c1", "Alice", 0.7));
        let out = merge(emb, llm);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].speaker_tag, "Speaker 0");
        assert_eq!(out[1].speaker_tag, "Speaker 1");
    }
}
