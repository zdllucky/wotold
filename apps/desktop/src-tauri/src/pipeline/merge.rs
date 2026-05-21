use crate::providers::transcription::{DiarizedTranscript, TranscriptSegment};

pub const OWNER_TAG: &str = "owner";

/// M2.4 паспорта: mic-дорожка всегда привязана к owner без диаризации,
/// system-дорожка сохраняет лейблы спикеров от STT-провайдера, оба сливаются
/// в общий таймлайн по started-at таймкодам.
///
/// Позже (M3.5) пользователь подтверждает соответствие `Speaker N` ↔ контакт;
/// сюда это не лезет — мы только готовим базу.
pub fn merge_tracks(
    mic: &DiarizedTranscript,
    system: &DiarizedTranscript,
) -> Vec<TranscriptSegment> {
    let mut combined: Vec<TranscriptSegment> =
        Vec::with_capacity(mic.segments.len() + system.segments.len());

    // [B16 audit P2] Filter NaN start times — STT-провайдер может вернуть
    // NaN для broken-segment edge cases (например ZeroDivision на end-of-stream).
    // partial_cmp возвращает None и default Equal → нестабильная сортировка.
    // Лучше дропнуть такие segments чем рисковать undefined order.
    for seg in &mic.segments {
        if seg.start.is_nan() {
            log::warn!("merge_tracks: drop mic segment with NaN start");
            continue;
        }
        combined.push(TranscriptSegment {
            speaker_tag: OWNER_TAG.to_string(),
            ..seg.clone()
        });
    }
    for seg in &system.segments {
        if seg.start.is_nan() {
            log::warn!("merge_tracks: drop system segment with NaN start");
            continue;
        }
        combined.push(seg.clone());
    }

    combined.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    combined
}

/// Рендерит merged-таймлайн в Markdown с заголовками спикеров и таймкодами начала
/// каждой смены спикера. Подходит для `transcript.md` (M4.4 паспорта).
pub fn render_transcript_md(segments: &[TranscriptSegment]) -> String {
    let mut out = String::new();
    out.push_str("# Transcript\n\n");

    let mut last_speaker = String::new();
    for seg in segments {
        if seg.speaker_tag != last_speaker {
            if !last_speaker.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!(
                "**{}** [{}]:\n",
                seg.speaker_tag,
                format_time(seg.start)
            ));
            last_speaker.clone_from(&seg.speaker_tag);
        }
        let text = seg.text.trim();
        if !text.is_empty() {
            out.push_str(text);
            out.push(' ');
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let m = total / 60;
    let s = total % 60;
    format!("{m}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(start: f64, end: f64, text: &str, speaker: &str) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: text.to_string(),
            speaker_tag: speaker.to_string(),
            confidence: None,
        }
    }

    fn diarized(provider: &str, segs: Vec<TranscriptSegment>) -> DiarizedTranscript {
        DiarizedTranscript {
            version: 1,
            provider: provider.to_string(),
            lang_detected: None,
            duration_sec: segs.last().map(|s| s.end).unwrap_or(0.0),
            segments: segs,
        }
    }

    #[test]
    fn merge_interleaves_mic_owner_and_system_speakers_by_start() {
        let mic = diarized(
            "soniox",
            vec![
                ts(0.0, 1.5, "hi there", "Speaker 0"),
                ts(3.0, 4.0, "okay", "Speaker 0"),
            ],
        );
        let system = diarized(
            "soniox",
            vec![
                ts(1.5, 2.5, "hello back", "Speaker 0"),
                ts(4.5, 6.0, "great", "Speaker 1"),
            ],
        );

        let merged = merge_tracks(&mic, &system);

        // mic helpers → owner, system сохраняет лейбл
        assert_eq!(merged[0].text, "hi there");
        assert_eq!(merged[0].speaker_tag, "owner");
        assert_eq!(merged[1].speaker_tag, "Speaker 0"); // system
        assert_eq!(merged[2].speaker_tag, "owner"); // mic
        assert_eq!(merged[3].speaker_tag, "Speaker 1"); // system

        let starts: Vec<f64> = merged.iter().map(|s| s.start).collect();
        assert_eq!(starts, vec![0.0, 1.5, 3.0, 4.5]);
    }

    #[test]
    fn merge_drops_segments_with_nan_start_time() {
        // [Phase 1 / B16 audit P2] STT-провайдер может вернуть NaN для
        // broken-segment edge cases. partial_cmp возвращает None →
        // sort_by default Equal → нестабильная сортировка. Дропаем.
        let mic = diarized(
            "soniox",
            vec![
                ts(0.0, 1.0, "valid mic", "Speaker 0"),
                ts(f64::NAN, 2.0, "broken mic", "Speaker 0"),
            ],
        );
        let system = diarized(
            "soniox",
            vec![
                ts(f64::NAN, 3.0, "broken sys", "Speaker 0"),
                ts(1.5, 2.5, "valid sys", "Speaker 1"),
            ],
        );

        let merged = merge_tracks(&mic, &system);

        assert_eq!(
            merged.len(),
            2,
            "оба NaN-сегмента должны быть отфильтрованы"
        );
        // Порядок: только valid сегменты остались, сортировка по start.
        assert_eq!(merged[0].text, "valid mic");
        assert_eq!(merged[0].speaker_tag, "owner");
        assert_eq!(merged[1].text, "valid sys");
        // Все starts финитны.
        for seg in &merged {
            assert!(seg.start.is_finite(), "non-finite start: {}", seg.start);
        }
    }

    #[test]
    fn merge_handles_all_nan_input_returns_empty() {
        // Crash-bait: оба track'а целиком из NaN. Должен вернуть пустой Vec,
        // не паниковать на sort.
        let mic = diarized("soniox", vec![ts(f64::NAN, f64::NAN, "x", "Speaker 0")]);
        let system = diarized("soniox", vec![ts(f64::NAN, f64::NAN, "y", "Speaker 0")]);
        let merged = merge_tracks(&mic, &system);
        assert!(merged.is_empty());
    }

    #[test]
    fn render_md_groups_consecutive_same_speaker_under_one_header() {
        let segs = vec![
            ts(0.0, 1.0, "hi", "owner"),
            ts(1.5, 2.0, "there", "owner"),
            ts(2.5, 4.0, "hello back", "Speaker 0"),
            ts(65.0, 66.0, "later", "owner"),
        ];
        let md = render_transcript_md(&segs);
        assert!(md.contains("**owner** [0:00]:\nhi there"));
        assert!(md.contains("**Speaker 0** [0:02]:\nhello back"));
        assert!(md.contains("**owner** [1:05]:\nlater"));
    }
}
