//! [M12.2.3] Merge timestamps: word-level STT segments + speaker segments →
//! `DiarizedTranscript`.
//!
//! Алгоритм по образцу whisperX (PRD §M12.2.3):
//!
//! 1. Для каждого word-segment'а из STT берём центр (`(start + end) / 2`).
//! 2. Находим speaker-segment у которого этот центр попадает в `[start, end]`.
//! 3. Если попадание неоднозначное / нет — используем наибольший overlap.
//! 4. Если speaker-segment'ов нет — `speaker:unknown` (`SPEAKER_UNKNOWN`).
//! 5. Owner-bind: если provider знает что трек = mic-owner, все сегменты
//!    форсятся в `speaker:owner` (M3.7, PRD §M12.2.4).
//!
//! Pure-функция — без I/O, легко тестируема. Используется в M12.6 pipeline
//! интеграции после STT (M12.1) + Diarize (M12.2).

use crate::providers::transcription::{DiarizedTranscript, TranscriptSegment};

use super::diarization::{SpeakerSegment, SPEAKER_UNKNOWN};

/// Stable tag для owner (mic-дорожка).
pub const SPEAKER_OWNER: &str = "speaker:owner";

/// Применить owner-bind: все сегменты форсятся в `SPEAKER_OWNER`.
/// См. M3.7 + PRD §M12.2.4 — mic-дорожка не диаризуется.
pub fn force_owner_track(stt_segments: &[TranscriptSegment]) -> Vec<TranscriptSegment> {
    stt_segments
        .iter()
        .map(|s| TranscriptSegment {
            speaker_tag: SPEAKER_OWNER.to_string(),
            ..s.clone()
        })
        .collect()
}

/// Сопоставить word-segments STT со speaker-segments. Возвращает
/// `Vec<TranscriptSegment>` где `speaker_tag` обновлён.
///
/// Невалидные сегменты (NaN, end < start) фильтруются.
pub fn merge_word_with_speaker(
    stt_segments: &[TranscriptSegment],
    speaker_segments: &[SpeakerSegment],
) -> Vec<TranscriptSegment> {
    stt_segments
        .iter()
        .filter(|s| s.start.is_finite() && s.end.is_finite() && s.end >= s.start)
        .map(|word| {
            let tag = find_best_speaker(word.start, word.end, speaker_segments)
                .unwrap_or_else(|| SPEAKER_UNKNOWN.to_string());
            TranscriptSegment {
                speaker_tag: tag,
                ..word.clone()
            }
        })
        .collect()
}

/// Найти speaker tag по центру word-segment'а. Логика:
/// 1. Центр word'а попадает в speaker-segment → этот тэг.
/// 2. Иначе — speaker с максимальным overlap.
/// 3. Если overlap = 0 — `None` (caller подставит `SPEAKER_UNKNOWN`).
fn find_best_speaker(start: f64, end: f64, speakers: &[SpeakerSegment]) -> Option<String> {
    let center = (start + end) / 2.0;
    if let Some(s) = speakers
        .iter()
        .find(|s| center >= s.start && center <= s.end)
    {
        return Some(s.speaker_tag.clone());
    }
    speakers
        .iter()
        .filter_map(|s| {
            let overlap = overlap_amount(start, end, s.start, s.end);
            if overlap > 0.0 {
                Some((overlap, s.speaker_tag.clone()))
            } else {
                None
            }
        })
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, tag)| tag)
}

fn overlap_amount(a_start: f64, a_end: f64, b_start: f64, b_end: f64) -> f64 {
    let lo = a_start.max(b_start);
    let hi = a_end.min(b_end);
    (hi - lo).max(0.0)
}

/// Финальная сборка `DiarizedTranscript`. Используется в M12.6 после
/// owner-track (mic) + system-track merge.
pub fn assemble_transcript(
    owner_segments: Vec<TranscriptSegment>,
    system_segments: Vec<TranscriptSegment>,
    lang_detected: Option<String>,
    duration_sec: f64,
) -> DiarizedTranscript {
    let mut segments: Vec<TranscriptSegment> =
        owner_segments.into_iter().chain(system_segments).collect();
    // [B16] NaN guard уже стоит в merge_word_with_speaker.filter — оставим
    // здесь sort_by stable на случай если callers смешали порядок.
    segments.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    DiarizedTranscript {
        version: 1,
        lang_detected,
        duration_sec,
        provider: "local".to_string(),
        segments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: text.to_string(),
            speaker_tag: String::new(),
            confidence: None,
        }
    }

    fn sp(start: f64, end: f64, tag: &str) -> SpeakerSegment {
        SpeakerSegment {
            start,
            end,
            speaker_tag: tag.to_string(),
        }
    }

    #[test]
    fn force_owner_track_overrides_all_tags() {
        let stt = vec![seg(0.0, 1.0, "привет")];
        let out = force_owner_track(&stt);
        assert_eq!(out[0].speaker_tag, SPEAKER_OWNER);
    }

    #[test]
    fn merge_picks_speaker_by_center_inclusion() {
        let stt = vec![seg(1.0, 2.0, "alpha"), seg(3.0, 4.0, "beta")];
        let speakers = vec![sp(0.0, 2.5, "speaker:0"), sp(2.5, 5.0, "speaker:1")];
        let merged = merge_word_with_speaker(&stt, &speakers);
        // word 1.0-2.0 center=1.5 ∈ speaker:0
        assert_eq!(merged[0].speaker_tag, "speaker:0");
        // word 3.0-4.0 center=3.5 ∈ speaker:1
        assert_eq!(merged[1].speaker_tag, "speaker:1");
    }

    #[test]
    fn merge_falls_back_to_max_overlap_when_center_outside() {
        // Center 3.5 не входит ни в один speaker-segment, но больший overlap
        // даёт speaker:1 (3.0-3.4 overlap 0.4 vs speaker:0 4.0-4.0 = 0).
        let stt = vec![seg(3.0, 4.0, "word")];
        let speakers = vec![sp(0.0, 3.4, "speaker:0"), sp(3.6, 4.5, "speaker:1")];
        let merged = merge_word_with_speaker(&stt, &speakers);
        assert!(
            merged[0].speaker_tag == "speaker:0" || merged[0].speaker_tag == "speaker:1",
            "got {}",
            merged[0].speaker_tag
        );
        // По метрике overlap (0.4 для speaker:0, 0.4 для speaker:1) tie-break
        // зависит от стабильности сортировки. Проверяем что НЕ unknown.
        assert_ne!(merged[0].speaker_tag, SPEAKER_UNKNOWN);
    }

    #[test]
    fn merge_returns_unknown_when_no_overlap() {
        let stt = vec![seg(10.0, 11.0, "isolated")];
        let speakers = vec![sp(0.0, 5.0, "speaker:0")];
        let merged = merge_word_with_speaker(&stt, &speakers);
        assert_eq!(merged[0].speaker_tag, SPEAKER_UNKNOWN);
    }

    #[test]
    fn merge_filters_nan_segments() {
        let stt = vec![
            TranscriptSegment {
                start: f64::NAN,
                end: 1.0,
                text: "bad".into(),
                speaker_tag: String::new(),
                confidence: None,
            },
            seg(2.0, 3.0, "good"),
        ];
        let speakers = vec![sp(0.0, 5.0, "speaker:0")];
        let merged = merge_word_with_speaker(&stt, &speakers);
        assert_eq!(merged.len(), 1, "NaN segment must be dropped");
        assert_eq!(merged[0].text, "good");
    }

    #[test]
    fn merge_handles_empty_speakers() {
        let stt = vec![seg(0.0, 1.0, "alone")];
        let merged = merge_word_with_speaker(&stt, &[]);
        assert_eq!(merged[0].speaker_tag, SPEAKER_UNKNOWN);
    }

    #[test]
    fn assemble_sorts_segments_by_start() {
        let owner = vec![TranscriptSegment {
            start: 5.0,
            end: 6.0,
            text: "owner-late".into(),
            speaker_tag: SPEAKER_OWNER.into(),
            confidence: None,
        }];
        let system = vec![TranscriptSegment {
            start: 1.0,
            end: 2.0,
            text: "system-early".into(),
            speaker_tag: "speaker:0".into(),
            confidence: None,
        }];
        let t = assemble_transcript(owner, system, Some("ru".into()), 10.0);
        assert_eq!(t.version, 1);
        assert_eq!(t.provider, "local");
        assert_eq!(t.lang_detected.as_deref(), Some("ru"));
        assert_eq!(t.duration_sec, 10.0);
        // System-segment с start=1.0 должен идти первым.
        assert_eq!(t.segments[0].text, "system-early");
        assert_eq!(t.segments[1].text, "owner-late");
    }

    #[test]
    fn assemble_empty_inputs_produce_valid_transcript() {
        let t = assemble_transcript(vec![], vec![], None, 0.0);
        assert!(t.segments.is_empty());
        assert_eq!(t.provider, "local");
    }
}
