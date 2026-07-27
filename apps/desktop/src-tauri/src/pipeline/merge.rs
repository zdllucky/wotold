use std::collections::{HashMap, HashSet};

use crate::local_engine::hallucination::is_hallucination;
use crate::providers::transcription::{DiarizedTranscript, TranscriptSegment};

pub const OWNER_TAG: &str = "owner";

/// [P-fix] Минимум слов в сегменте, прежде чем пытаться схлопывать
/// intra-segment repeat. Защищает легитимные короткие эмфазы
/// («очень очень хорошо») от ложного схлопа.
const MIN_WORDS_FOR_REPEAT_COLLAPSE: usize = 6;

/// [P-fix2] Ключи «шумовых» bracket-тегов, которые whisper лепит на музыке/
/// тишине/аплодисментах (`[Музыка] …`, `[Applause] …`). Whitelist — НЕ трогаем
/// произвольные `[...]` (могут нести смысл). Сравнение по lowercase-substring.
const NOISE_TAG_KEYWORDS: &[&str] = &[
    "музык",
    "music",
    "musique",
    "musik",
    "muzyk",
    "applause",
    "аплодис",
    "смех",
    "laughter",
    "шум",
    "noise",
    "silence",
    "тишина",
];

/// [P-fix5] Global loop-hallucination: длинная фраза (≥ LOOP_MIN_WORDS слов),
/// повторяющаяся в ≥ LOOP_MIN_OCCURRENCES сегментах по ВСЕМУ треку — whisper
/// loop (prompt-echo / зацикливание на тишине). Дропаем все вхождения. Порог по
/// длине защищает легитимные короткие повторы («да», «угу», «понятно»).
const LOOP_MIN_OCCURRENCES: usize = 4;
const LOOP_MIN_WORDS: usize = 5;

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

/// [P-fix] Единый chokepoint очистки merged-таймлайна от whisper-артефактов.
/// Применяется в `stage_merge_artifacts` ПОСЛЕ `merge_tracks`, до записи
/// `raw_stt.json` (поле `merged`, которое читает UI) + `transcript.md` + recap.
/// Покрывает ВСЕ пути (chunked / full-file / cloud) одним местом — в т.ч.
/// чинит переассемблируемые старые chunks без повторного STT.
///
/// Шаги:
/// 0. global loop-pass — фраза, повторённая по всему треку (≥4×, ≥5 слов) →
///    whisper-петля/prompt-echo, дропаем все вхождения.
/// 1. intra-segment collapse — `phrase phrase phrase…` (≥3×) → одна фраза.
/// 2. drop галлюцинаций (`is_hallucination`): `[FOREIGN]`, субтитр-credits.
/// 3. collapse подряд идущих сегментов ОДНОГО спикера с идентичным
///    нормализованным текстом (whisper repetition loops) — растягивая `end`.
pub fn sanitize_merged(segments: Vec<TranscriptSegment>) -> Vec<TranscriptSegment> {
    // 0. Pre-pass: найти global loop-фразы (повтор длинной фразы по всему треку).
    let mut freq: HashMap<String, usize> = HashMap::new();
    for seg in &segments {
        let norm = normalize_text(&seg.text);
        if norm.split_whitespace().count() >= LOOP_MIN_WORDS {
            *freq.entry(norm).or_insert(0) += 1;
        }
    }
    let loop_texts: HashSet<String> = freq
        .into_iter()
        .filter(|(_, c)| *c >= LOOP_MIN_OCCURRENCES)
        .map(|(t, _)| t)
        .collect();

    let mut out: Vec<TranscriptSegment> = Vec::with_capacity(segments.len());
    let mut dropped = 0usize;
    let mut collapsed = 0usize;
    for mut seg in segments {
        // 1. strip ведущие noise-теги («[Музыка] текст» → «текст») + intra-segment
        //    loop collapse. После может остаться чистый текст, мусор или "".
        let cleaned = collapse_intra_repeats(&strip_leading_noise_tags(&seg.text));
        if cleaned != seg.text.trim() {
            collapsed += 1;
        }
        seg.text = cleaned;

        // 2. drop галлюцинаций (включая опустевшие после strip) + global loop.
        if is_hallucination(&seg.text)
            || (!loop_texts.is_empty() && loop_texts.contains(&normalize_text(&seg.text)))
        {
            dropped += 1;
            continue;
        }

        // 3. consecutive-duplicate collapse (тот же спикер, тот же текст).
        if let Some(prev) = out.last_mut() {
            if prev.speaker_tag == seg.speaker_tag
                && normalize_text(&prev.text) == normalize_text(&seg.text)
            {
                if seg.end > prev.end {
                    prev.end = seg.end;
                }
                collapsed += 1;
                continue;
            }
        }
        out.push(seg);
    }
    if dropped > 0 || collapsed > 0 {
        log::info!("sanitize_merged: dropped {dropped} hallucinations, collapsed {collapsed} repeats → {} segments", out.len());
    }
    out
}

/// [P-fix2] Срезать ведущие «шумовые» bracket-теги (`[Музыка] …`, `[Applause] …`),
/// оставляя реальный текст. Стрипает только теги из NOISE_TAG_KEYWORDS whitelist
/// и не длиннее ~40 символов внутри скобок — произвольные `[...]` не трогаем.
/// Работает в цикле: «[Музыка] [Applause] текст» → «текст». Standalone
/// «[Музыка]» → "" (далее дропнется как пустой в is_hallucination).
fn strip_leading_noise_tags(text: &str) -> String {
    let mut s = text.trim_start();
    while let Some(rest) = s.strip_prefix('[') {
        let Some(close_rel) = rest.find(']') else {
            break;
        };
        let inner = &rest[..close_rel];
        if inner.chars().count() > 40 {
            break;
        }
        let inner_low = inner.to_lowercase();
        if NOISE_TAG_KEYWORDS.iter().any(|k| inner_low.contains(k)) {
            s = rest[close_rel + 1..].trim_start();
        } else {
            break;
        }
    }
    s.to_string()
}

/// Нормализация для сравнения текста: схлоп пробелов + lowercase.
fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// [P-fix] Свернуть текст-петлю «phrase phrase phrase …» в одну фразу.
/// Консервативно: только если ВЕСЬ текст (по словам) — одна фраза, повторённая
/// ≥3 раза подряд. Ниже `MIN_WORDS_FOR_REPEAT_COLLAPSE` слов не трогаем
/// (защита легитимной короткой эмфазы). Возвращает trimmed-текст без изменений
/// если петля не найдена.
fn collapse_intra_repeats(text: &str) -> String {
    let trimmed = text.trim();
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() < MIN_WORDS_FOR_REPEAT_COLLAPSE {
        return trimmed.to_string();
    }
    for plen in 1..=(words.len() / 3) {
        if words.len() % plen != 0 {
            continue;
        }
        let reps = words.len() / plen;
        if reps < 3 {
            continue;
        }
        let phrase = &words[0..plen];
        if (0..reps).all(|i| &words[i * plen..(i + 1) * plen] == phrase) {
            return phrase.join(" ");
        }
    }
    trimmed.to_string()
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

    // ── sanitize_merged ─────────────────────────────────────────────────

    #[test]
    fn sanitize_drops_foreign_and_subtitle_credits() {
        let segs = vec![
            ts(0.0, 13.0, "Добрый день, ещё раз.", "speaker:1"),
            ts(116.0, 126.0, "[FOREIGN]", "owner"),
            ts(146.0, 156.0, "- [FOREIGN]", "owner"),
            ts(
                162.0,
                170.0,
                "[Редактор субтитров Н.Александрова] [Апалькова]",
                "speaker:unknown",
            ),
            ts(200.0, 205.0, "Да, согласен.", "speaker:1"),
        ];
        let out = sanitize_merged(segs);
        assert_eq!(out.len(), 2, "должны остаться только 2 легит-сегмента");
        assert_eq!(out[0].text, "Добрый день, ещё раз.");
        assert_eq!(out[1].text, "Да, согласен.");
    }

    #[test]
    fn sanitize_collapses_consecutive_duplicates_same_speaker() {
        let segs = vec![
            ts(0.0, 2.0, "Угу.", "speaker:0"),
            ts(2.0, 4.0, "угу.", "speaker:0"), // дубль (case/space-insensitive)
            ts(4.0, 6.0, "  Угу.  ", "speaker:0"), // дубль
            ts(6.0, 8.0, "Понятно.", "speaker:0"),
        ];
        let out = sanitize_merged(segs);
        assert_eq!(out.len(), 2, "три подряд 'угу' → один");
        assert_eq!(out[0].text, "Угу.");
        // end растянулся до последнего дубля.
        assert!((out[0].end - 6.0).abs() < 1e-9);
        assert_eq!(out[1].text, "Понятно.");
    }

    #[test]
    fn sanitize_keeps_same_text_from_different_speakers() {
        // Один и тот же ответ от РАЗНЫХ спикеров — не дубль, оставляем оба.
        let segs = vec![
            ts(0.0, 1.0, "Да.", "speaker:0"),
            ts(1.0, 2.0, "Да.", "speaker:1"),
        ];
        let out = sanitize_merged(segs);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn sanitize_collapses_intra_segment_loop() {
        // Реальная фраза, повторённая ≥3× внутри одного сегмента → одна.
        let segs = vec![ts(
            0.0,
            10.0,
            "так и есть так и есть так и есть",
            "speaker:1",
        )];
        let out = sanitize_merged(segs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "так и есть");
    }

    #[test]
    fn sanitize_preserves_legit_short_emphasis() {
        // Короткая легит-эмфаза (< MIN_WORDS_FOR_REPEAT_COLLAPSE) не трогается.
        let segs = vec![ts(0.0, 2.0, "очень очень хорошо", "speaker:1")];
        let out = sanitize_merged(segs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "очень очень хорошо");
    }

    #[test]
    fn sanitize_strips_leading_music_tag_keeps_text() {
        let segs = vec![
            // standalone music-тег → drop.
            ts(0.0, 2.0, "[Музыка]", "speaker:0"),
            // inline ведущий music-тег → срезается, текст остаётся.
            ts(
                2.0,
                12.0,
                "[Музыка] Может есть какие-то дополнительные точки.",
                "speaker:1",
            ),
            // польский «inaudible music» → drop (substring muzyk).
            ts(12.0, 14.0, "[niedosłyszalna muzyka]", "speaker:2"),
        ];
        let out = sanitize_merged(segs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "Может есть какие-то дополнительные точки.");
    }

    #[test]
    fn sanitize_drops_global_loop_phrase() {
        // [P-fix5] prompt-echo: длинная фраза повторяется по всему треку
        // (разные спикеры, не подряд) → дропается целиком.
        let echo = "Что вы думаете о том что вы говорите на русском языке";
        let mut segs = vec![ts(0.0, 6.0, echo, "owner")];
        for i in 1..6 {
            let sp = if i % 2 == 0 {
                "owner"
            } else {
                "speaker:unknown"
            };
            segs.push(ts(i as f64 * 6.0, i as f64 * 6.0 + 6.0, echo, sp));
        }
        // вкрапим реальную речь между эхо.
        segs.insert(
            2,
            ts(50.0, 55.0, "Это реальная реплика собеседника", "speaker:1"),
        );
        let out = sanitize_merged(segs);
        assert!(
            out.iter().all(|s| !s.text.contains("на русском языке")),
            "все эхо-вхождения должны быть удалены"
        );
        assert!(out
            .iter()
            .any(|s| s.text == "Это реальная реплика собеседника"));
    }

    #[test]
    fn sanitize_keeps_short_filler_repeated_many_times() {
        // Короткий филлер «да» ×10 → НЕ дропается (ниже LOOP_MIN_WORDS).
        let segs: Vec<_> = (0..10)
            .map(|i| ts(i as f64, i as f64 + 0.5, "Да.", "speaker:1"))
            .collect();
        let out = sanitize_merged(segs);
        // consecutive-dedup схлопнет подряд-дубли в один, но не дропнет как loop.
        assert!(
            !out.is_empty(),
            "короткий филлер не должен дропаться целиком"
        );
        assert!(out.iter().all(|s| s.text == "Да."));
    }

    #[test]
    fn strip_leading_noise_tags_leaves_non_noise_brackets() {
        // Не-шумовые скобки и обычный текст не трогаются.
        assert_eq!(strip_leading_noise_tags("[1] пункт"), "[1] пункт");
        assert_eq!(strip_leading_noise_tags("обычный текст"), "обычный текст");
        // Несколько ведущих шумовых тегов подряд → все срезаются.
        assert_eq!(
            strip_leading_noise_tags("[Музыка] [Applause] реальный текст"),
            "реальный текст"
        );
    }
}
