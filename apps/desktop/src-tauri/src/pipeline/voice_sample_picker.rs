//! [P4] Best-segment selection для voice sample slice metadata.
//!
//! После speaker confirm в `voice_backfill::maybe_backfill_voice_sample`
//! сохраняем embedding cluster centroid + (новое в migration 0017) ссылку
//! на короткий аудио-фрагмент: start_sec, end_sec, track_kind. UI потом
//! играет эту slice вместо full source_call mic.wav (см. P3 silence bug).
//!
//! Источник данных — merged `raw_stt.json` artifact на диске (содержит
//! `merged: [{speakerTag, start, end, text}, ...]`). На non-chunked path
//! это файл из stage_merge_artifacts; на chunked — assembled через
//! chunk_assembly.
//!
//! # Heuristic для track_kind
//!
//! Merged transcript НЕ содержит per-segment track origin — segments
//! из mic_segments и sys_segments объединены через `merge_word_with_speaker`.
//! Применяем upstream-invariant правило:
//!
//! - `speakerTag == OWNER_TAG` (`"owner"`) → `mic` (M3.7: owner всегда mic).
//! - Прочее (`speaker:N`, `speaker:unknown`) → `system` (большинство случаев).
//!
//! **Limitation:** для anonymous mic speakers появившихся после P1.2
//! (mic-диаризация выделяет `speaker:N` на mic-дорожке без identify_owner
//! relabel) heuristic shows их как system → playback из system.wav может
//! быть silent. Refinement: добавить track_kind в `call_speakers` schema
//! at cluster-compute time. Backlog.
//!
//! # Selection criteria
//!
//! - Filter `merged[]` по speakerTag.
//! - Pick longest segment (max `end - start`).
//! - Floor: ≥ 1.5 sec (короче — bad audio preview UX). Если все короче →
//!   `None` (caller сохраняет sample без slice metadata).
//! - Cap: ≤ 10 sec (longer не нужен для voice preview).

use serde::Deserialize;

/// Минимальная длина sample slice. Короче — UX bad (юзер не услышит
/// различимый голос), не возвращаем.
pub const MIN_SAMPLE_SEC: f64 = 1.5;

/// Максимальная длина sample slice. Длиннее обрезаем — full call playback
/// был root P3 bug.
pub const MAX_SAMPLE_SEC: f64 = 10.0;

/// Track origin для voice sample. Сохраняется как `'mic'` либо `'system'`
/// в DB.track_kind. См. модульный комментарий про OWNER heuristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Mic,
    System,
}

impl TrackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TrackKind::Mic => "mic",
            TrackKind::System => "system",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawSegment {
    #[serde(rename = "speakerTag")]
    speaker_tag: String,
    start: f64,
    end: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawStt {
    #[serde(default)]
    merged: Vec<RawSegment>,
}

/// Выбрать best slice metadata для voice sample.
///
/// `raw_stt_json` — содержимое `raw_stt.json` artifact (либо `None`
/// если файл missing → возвращаем None, graceful).
/// `speaker_tag` — confirmed speaker tag (`"owner"` либо `"speaker:N"`).
/// `owner_tag` — value of OWNER_TAG constant (passed explicitly чтобы
/// модуль не depended на pipeline::merge).
///
/// Возвращает `Some((start_sec, end_sec, track))` если найден segment
/// ≥ MIN_SAMPLE_SEC, иначе `None` (legacy / no audio).
pub fn best_sample_segment(
    raw_stt_json: &str,
    speaker_tag: &str,
    owner_tag: &str,
) -> Option<(f64, f64, TrackKind)> {
    let parsed: RawStt = serde_json::from_str(raw_stt_json).ok()?;
    let candidates: Vec<&RawSegment> = parsed
        .merged
        .iter()
        .filter(|s| s.speaker_tag == speaker_tag)
        .filter(|s| s.end > s.start && (s.end - s.start) >= MIN_SAMPLE_SEC)
        .collect();
    // partial_cmp может вернуть None только для NaN (defensive: filter уже
    // отбросил degenerate end<=start). Fall back на Ordering::Equal — порядок
    // двух NaN-like rows не важен, главное не panic.
    let best = candidates.iter().max_by(|a, b| {
        (a.end - a.start)
            .partial_cmp(&(b.end - b.start))
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let start = best.start;
    let raw_end = best.end;
    let capped_end = (start + MAX_SAMPLE_SEC).min(raw_end);
    let track = if speaker_tag == owner_tag {
        TrackKind::Mic
    } else {
        TrackKind::System
    };
    Some((start, capped_end, track))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "owner";

    fn raw(segments: &[(&str, f64, f64)]) -> String {
        let merged: Vec<_> = segments
            .iter()
            .map(|(tag, start, end)| {
                serde_json::json!({
                    "speakerTag": tag,
                    "start": start,
                    "end": end,
                    "text": "...",
                })
            })
            .collect();
        serde_json::json!({ "merged": merged }).to_string()
    }

    #[test]
    fn returns_none_for_empty_merged() {
        let json = raw(&[]);
        assert!(best_sample_segment(&json, OWNER, OWNER).is_none());
    }

    #[test]
    fn returns_none_when_speaker_absent() {
        let json = raw(&[(OWNER, 0.0, 5.0)]);
        assert!(best_sample_segment(&json, "speaker:0", OWNER).is_none());
    }

    #[test]
    fn returns_none_when_all_segments_too_short() {
        // Все segments < MIN_SAMPLE_SEC (1.5) — graceful skip.
        let json = raw(&[(OWNER, 0.0, 1.0), (OWNER, 2.0, 3.0)]);
        assert!(best_sample_segment(&json, OWNER, OWNER).is_none());
    }

    #[test]
    fn picks_longest_segment_for_owner() {
        // 3 segments owner — pick longest (3.0-6.0 = 3s).
        let json = raw(&[
            (OWNER, 0.0, 2.0),         // 2.0s
            (OWNER, 3.0, 6.0),         // 3.0s ← longest
            (OWNER, 7.0, 9.0),         // 2.0s
            ("speaker:0", 10.0, 15.0), // wrong tag — skipped
        ]);
        let (start, end, track) = best_sample_segment(&json, OWNER, OWNER).unwrap();
        assert!((start - 3.0).abs() < 1e-9);
        assert!((end - 6.0).abs() < 1e-9);
        assert_eq!(track, TrackKind::Mic);
    }

    #[test]
    fn caps_at_max_sample_sec() {
        // Single very long segment — clamp to MAX_SAMPLE_SEC.
        let json = raw(&[(OWNER, 5.0, 30.0)]); // 25s segment
        let (start, end, _track) = best_sample_segment(&json, OWNER, OWNER).unwrap();
        assert!((start - 5.0).abs() < 1e-9);
        assert!((end - 15.0).abs() < 1e-9); // start + 10s cap
    }

    #[test]
    fn non_owner_tag_resolved_as_system() {
        let json = raw(&[("speaker:0", 0.0, 3.0)]);
        let (_start, _end, track) = best_sample_segment(&json, "speaker:0", OWNER).unwrap();
        assert_eq!(track, TrackKind::System);
    }

    #[test]
    fn handles_malformed_json_gracefully() {
        assert!(best_sample_segment("not json", OWNER, OWNER).is_none());
        assert!(best_sample_segment("{}", OWNER, OWNER).is_none());
    }

    #[test]
    fn ignores_zero_or_negative_duration_segments() {
        let json = raw(&[
            (OWNER, 5.0, 5.0),  // zero-length
            (OWNER, 10.0, 8.0), // negative (end<start)
            (OWNER, 0.0, 2.0),  // valid 2s
        ]);
        let (start, end, _track) = best_sample_segment(&json, OWNER, OWNER).unwrap();
        assert!((start - 0.0).abs() < 1e-9);
        assert!((end - 2.0).abs() < 1e-9);
    }
}
