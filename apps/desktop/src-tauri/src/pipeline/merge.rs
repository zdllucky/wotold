use crate::providers::transcription::{DiarizedTranscript, TranscriptSegment};

pub const OWNER_TAG: &str = "owner";

/// M2.4 паспорта: mic-дорожка традиционно привязана к owner. system-дорожка
/// сохраняет лейблы спикеров от STT-провайдера. Оба сливаются в общий
/// таймлайн по started-at таймкодам.
///
/// [Bug-fix] **Mic-diarization aware:** если mic-сегменты уже прошли через
/// `diarize_mic_track` + `relabel_owner_on_mic_full_file` (M13 follow-up),
/// они имеют смешанный набор тэгов — `OWNER_TAG` для owner'а + `speaker:N`
/// для гостевых голосов на mic. В этом случае сохраняем существующие tags,
/// иначе legacy путь — все mic сегменты принудительно в `OWNER_TAG`.
///
/// Detection: если хотя бы один mic-сегмент уже == OWNER_TAG AND хотя бы один
/// имеет `speaker:N` (где N — цифры), значит mic-diarization сработала —
/// сохраняем разнообразие. Иначе force OWNER для всех (cloud-STT path
/// где mic-tags бывают разные но без owner-relabel).
///
/// Позже (M3.5) пользователь подтверждает соответствие `Speaker N` ↔ контакт;
/// сюда это не лезет — мы только готовим базу.
pub fn merge_tracks(
    mic: &DiarizedTranscript,
    system: &DiarizedTranscript,
) -> Vec<TranscriptSegment> {
    let mut combined: Vec<TranscriptSegment> =
        Vec::with_capacity(mic.segments.len() + system.segments.len());

    // [Bug-fix] Mic-diarization detection. Owner-relabel должен был расставить
    // OWNER_TAG на dominant cluster + оставить остальные как `speaker:N`.
    // Если оба условия выполнены — sortformer + relabel реально отработали.
    let mic_has_owner = mic.segments.iter().any(|s| s.speaker_tag == OWNER_TAG);
    let mic_has_distinct_speaker = mic.segments.iter().any(|s| {
        let t = &s.speaker_tag;
        t.starts_with("speaker:")
            && t != "speaker:unknown"
            && t.bytes().skip("speaker:".len()).all(|b| b.is_ascii_digit())
    });
    let preserve_mic_tags = mic_has_owner && mic_has_distinct_speaker;
    if preserve_mic_tags {
        log::info!(
            "merge_tracks: preserving mic-diarization tags ({} segments with mixed owner+speaker:N)",
            mic.segments.len()
        );
    }

    // [B16 audit P2] Filter NaN start times — STT-провайдер может вернуть
    // NaN для broken-segment edge cases (например ZeroDivision на end-of-stream).
    // partial_cmp возвращает None и default Equal → нестабильная сортировка.
    // Лучше дропнуть такие segments чем рисковать undefined order.
    for seg in &mic.segments {
        if seg.start.is_nan() {
            log::warn!("merge_tracks: drop mic segment with NaN start");
            continue;
        }
        let tag = if preserve_mic_tags {
            // Mic-diarization сработала — сохраняем individual tag (owner + speaker:N).
            seg.speaker_tag.clone()
        } else {
            // Legacy fallback — все mic → OWNER_TAG.
            OWNER_TAG.to_string()
        };
        combined.push(TranscriptSegment {
            speaker_tag: tag,
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

    // [Bug-fix] Mic-diarization preserve path — sortformer выделил несколько
    // голосов на mic, owner_identify проставил OWNER на dominant. merge_tracks
    // должен сохранить mixed tags (owner + speaker:N), а не схлопнуть всех
    // в owner.
    #[test]
    fn merge_preserves_mic_tags_when_diarization_active() {
        let mic = diarized(
            "local",
            vec![
                ts(0.0, 2.0, "hello", "owner"),
                ts(2.0, 4.0, "guest line", "speaker:0"),
                ts(4.0, 6.0, "more owner", "owner"),
                ts(6.0, 7.0, "guest again", "speaker:2"),
            ],
        );
        let system = diarized("local", vec![]);
        let merged = merge_tracks(&mic, &system);
        assert_eq!(merged.len(), 4);
        // Detection condition: mic has both OWNER + speaker:N → preserve.
        assert_eq!(merged[0].speaker_tag, "owner");
        assert_eq!(merged[1].speaker_tag, "speaker:0");
        assert_eq!(merged[2].speaker_tag, "owner");
        assert_eq!(merged[3].speaker_tag, "speaker:2");
    }

    // [Bug-fix] Legacy fallback — mic-сегменты пришли от cloud-STT с
    // "Speaker 0" / "Speaker 1" тегами (без owner-relabel). Detection
    // condition (`owner` + `speaker:N`) НЕ выполнено → force OWNER для всех.
    #[test]
    fn merge_forces_owner_on_mic_legacy_path_no_owner_tag() {
        let mic = diarized(
            "soniox",
            vec![
                ts(0.0, 1.0, "a", "Speaker 0"),
                ts(1.0, 2.0, "b", "Speaker 1"),
            ],
        );
        let system = diarized("soniox", vec![]);
        let merged = merge_tracks(&mic, &system);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].speaker_tag, "owner");
        assert_eq!(merged[1].speaker_tag, "owner");
    }

    // [Bug-fix] Если sortformer выдал ТОЛЬКО owner (1-голосовая запись),
    // distinct speaker:N отсутствует → preserve_mic_tags = false → legacy
    // path force OWNER (это правильно: и так был только owner).
    #[test]
    fn merge_forces_owner_when_only_owner_no_distinct_speakers() {
        let mic = diarized(
            "local",
            vec![ts(0.0, 1.0, "a", "owner"), ts(1.0, 2.0, "b", "owner")],
        );
        let system = diarized("local", vec![]);
        let merged = merge_tracks(&mic, &system);
        assert_eq!(merged[0].speaker_tag, "owner");
        assert_eq!(merged[1].speaker_tag, "owner");
    }

    // [Bug-fix] speaker:unknown НЕ считается distinct → legacy path.
    #[test]
    fn merge_treats_speaker_unknown_as_non_distinct() {
        let mic = diarized(
            "local",
            vec![
                ts(0.0, 1.0, "a", "owner"),
                ts(1.0, 2.0, "b", "speaker:unknown"),
            ],
        );
        let system = diarized("local", vec![]);
        let merged = merge_tracks(&mic, &system);
        // Нет real distinct speaker:N → force OWNER на всех (включая unknown).
        assert_eq!(merged[0].speaker_tag, "owner");
        assert_eq!(merged[1].speaker_tag, "owner");
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
