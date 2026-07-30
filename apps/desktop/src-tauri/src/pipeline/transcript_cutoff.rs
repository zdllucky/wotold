//! [T6/R14] Отсечка транскрипта по точке реза тихого хвоста.
//!
//! # Зачем отдельным модулем
//!
//! Подрезать один только корневой WAV бессмысленно: в chunked-режиме (он
//! включён по умолчанию) тихие чанки уже прошли whisper **во время** записи, и
//! их галлюцинации собрались бы в транскрипт и уехали в рекап. Поэтому у реза
//! две половины — файловая (`audio::wav_trim`) и транскриптная (эта).
//!
//! Логика чистая и живёт отдельно от сборки чанков: `chunk_assembly` и без
//! того упирается в гейт 800 строк (правило 8), а здесь — три решения, каждое
//! на одну строку, но каждое ошибочное молча портит результат звонка.
//!
//! Времена везде абсолютные — от начала записи, после применения
//! `chunk.start_ms`. Смешение шкал в этом файле уже стоило одного бага
//! (см. `process_final_chunk`: pause-inclusive против pause-subtracted).

use crate::providers::transcription::TranscriptSegment;

/// Попадает ли чанк в запись целиком за точкой реза. `cutoff_ms = None` —
/// ручной стоп, режем ничего.
///
/// `start_ms` берётся из БД как `i64` и может быть отрицательным у битой
/// строки — трактуем такую как начинающуюся в нуле, то есть оставляем:
/// выбросить реальный кусок разговора хуже, чем оставить лишний.
pub fn chunk_is_before_cutoff(start_ms: i64, cutoff_ms: Option<u64>) -> bool {
    match cutoff_ms {
        Some(cut) => (start_ms.max(0) as u64) < cut,
        None => true,
    }
}

/// Сегмент относительно точки реза: целиком за ней — выбросить, пересекающий
/// её — подрезать по концу.
pub fn clip_segment(
    mut seg: TranscriptSegment,
    cutoff_sec: Option<f64>,
) -> Option<TranscriptSegment> {
    let Some(cut) = cutoff_sec else {
        return Some(seg);
    };
    if seg.start >= cut {
        return None;
    }
    if seg.end > cut {
        seg.end = cut;
    }
    Some(seg)
}

/// Длительность не должна обгонять подрезанное аудио: `end_ms` пограничного
/// чанка считался до реза, и плеер с транскриптом разошлись бы.
pub fn clamp_duration_sec(duration_sec: f64, cutoff_sec: Option<f64>) -> f64 {
    match cutoff_sec {
        Some(cut) => duration_sec.min(cut),
        None => duration_sec,
    }
}

/// Точка реза в секундах — в транскрипте времена в секундах, в БД в мс.
pub fn cutoff_sec(cutoff_ms: Option<u64>) -> Option<f64> {
    cutoff_ms.map(|ms| ms as f64 / 1_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: "t".into(),
            speaker_tag: "owner".into(),
            confidence: None,
        }
    }

    #[test]
    fn chunk_before_cutoff_survives() {
        assert!(chunk_is_before_cutoff(0, Some(600_000)));
        assert!(chunk_is_before_cutoff(599_999, Some(600_000)));
    }

    #[test]
    fn chunk_starting_at_or_past_cutoff_is_dropped() {
        // Ровно на резе — уже за ним: у такого чанка не может быть звука до
        // точки реза, только тишина после.
        assert!(!chunk_is_before_cutoff(600_000, Some(600_000)));
        assert!(!chunk_is_before_cutoff(1_200_000, Some(600_000)));
    }

    #[test]
    fn without_cutoff_every_chunk_survives() {
        assert!(chunk_is_before_cutoff(0, None));
        assert!(chunk_is_before_cutoff(99_999_999, None));
    }

    #[test]
    fn corrupt_negative_start_is_kept_not_dropped() {
        // Выбросить реальный кусок разговора хуже, чем оставить лишний.
        assert!(chunk_is_before_cutoff(-1, Some(600_000)));
    }

    #[test]
    fn segment_fully_before_cutoff_is_untouched() {
        let out = clip_segment(seg(1.0, 2.0), Some(5.0)).expect("оставить");
        assert_eq!((out.start, out.end), (1.0, 2.0));
    }

    #[test]
    fn segment_fully_past_cutoff_is_dropped() {
        assert!(clip_segment(seg(5.0, 6.0), Some(5.0)).is_none());
        assert!(clip_segment(seg(9.0, 10.0), Some(5.0)).is_none());
    }

    #[test]
    fn segment_crossing_cutoff_is_clipped_at_the_cut() {
        let out = clip_segment(seg(4.0, 9.0), Some(5.0)).expect("оставить");
        assert_eq!((out.start, out.end), (4.0, 5.0));
    }

    #[test]
    fn without_cutoff_segments_pass_through_unchanged() {
        let out = clip_segment(seg(4.0, 9.0), None).expect("оставить");
        assert_eq!((out.start, out.end), (4.0, 9.0));
    }

    #[test]
    fn duration_is_clamped_to_cut_but_never_stretched() {
        assert_eq!(clamp_duration_sec(600.0, Some(120.0)), 120.0);
        assert_eq!(clamp_duration_sec(60.0, Some(120.0)), 60.0);
        assert_eq!(clamp_duration_sec(600.0, None), 600.0);
    }

    #[test]
    fn cutoff_converts_ms_to_sec() {
        assert_eq!(cutoff_sec(Some(1_505_000)), Some(1_505.0));
        assert_eq!(cutoff_sec(None), None);
    }
}
