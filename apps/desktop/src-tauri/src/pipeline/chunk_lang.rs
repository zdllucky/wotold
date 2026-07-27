//! [P-fix] Язык чанка и хвост-промпт: две мелочи, от которых зависит, каким
//! языком уедет весь звонок и насколько точна первая фраза следующего чанка.
//!
//! [TD-41] Выделено из `chunk_runner.rs` (852 строки при лимите 800,
//! правило 8) вместе с тестами. Логика не менялась.

use crate::local_engine::hallucination::is_hallucination;
use crate::providers::transcription::DiarizedTranscript;

/// Извлечь последние `max_words` слов из transcript'а. Для prompt priming
/// whisper-cli — точность первой фразы chunk N+1 вырастает с 80% до 95%.
pub(crate) fn extract_tail_words(transcript: &DiarizedTranscript, max_words: usize) -> String {
    // Concat сегменты в одну строку слов, отбрасываем speaker tags. Просто
    // последние N "пробело-разделённых" токенов — для whisper.cpp prompt
    // нужен plain text без diarization markup.
    let all_text: String = transcript
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let words: Vec<&str> = all_text.split_whitespace().collect();
    if words.len() <= max_words {
        all_text.trim().to_string()
    } else {
        words[words.len() - max_words..].join(" ")
    }
}

/// [P-fix] Минимум «реальных» слов в chunk'е, чтобы доверять его language
/// detection для per-call pinning. Короткий/тихий chunk не должен пинить язык.
pub(crate) const MIN_WORDS_FOR_LANG_PIN: usize = 8;

/// [P-fix] Число слов в треке, исключая сегменты-галлюцинации ([FOREIGN] и пр.)
/// — это «реальный» объём речи, по которому судим об уверенности lang-detect.
pub(crate) fn real_word_count(t: &DiarizedTranscript) -> usize {
    t.segments
        .iter()
        .filter(|s| !is_hallucination(&s.text, s.confidence))
        .map(|s| s.text.split_whitespace().count())
        .sum()
}

/// [P-fix] Выбрать язык для per-call pinning из трека с бóльшим объёмом речи.
/// Возвращает `None` если самый «речевой» трек ниже порога уверенности —
/// тогда пин откладывается до более содержательного chunk'а (а не отравляется
/// тихим mic-треком, который whisper часто детектит как «en»).
pub(crate) fn pick_pinned_lang(
    mic: &DiarizedTranscript,
    sys: Option<&DiarizedTranscript>,
) -> Option<String> {
    let mic_words = real_word_count(mic);
    let sys_words = sys.map(real_word_count).unwrap_or(0);
    let (lang, words) = if sys_words > mic_words {
        (sys.and_then(|t| t.lang_detected.clone()), sys_words)
    } else {
        (mic.lang_detected.clone(), mic_words)
    };
    let lang = lang.filter(|s| !s.is_empty())?;
    (words >= MIN_WORDS_FOR_LANG_PIN).then_some(lang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::transcription::TranscriptSegment;

    fn track(lang: &str, segs: Vec<(&str, &str)>) -> DiarizedTranscript {
        DiarizedTranscript {
            version: 1,
            lang_detected: Some(lang.into()),
            duration_sec: 10.0,
            provider: "local".into(),
            segments: segs
                .into_iter()
                .enumerate()
                .map(|(i, (text, tag))| TranscriptSegment {
                    start: i as f64,
                    end: i as f64 + 1.0,
                    text: text.into(),
                    speaker_tag: tag.into(),
                    confidence: None,
                })
                .collect(),
        }
    }

    #[test]
    fn pick_pinned_lang_prefers_busier_track() {
        // [P-fix] mic тихий/«en» (только [FOREIGN]) + sys русский с 10 словами
        // → pin берётся из sys = «ru», не из mic.
        let mic = track("en", vec![("[FOREIGN]", "owner")]);
        let sys = track(
            "ru",
            vec![(
                "раз два три четыре пять шесть семь восемь девять десять",
                "speaker:0",
            )],
        );
        assert_eq!(pick_pinned_lang(&mic, Some(&sys)).as_deref(), Some("ru"));
    }

    #[test]
    fn pick_pinned_lang_none_below_threshold() {
        // mic = 0 реальных слов ([FOREIGN] исключён), sys = 1 слово < порога → None.
        let mic = track("en", vec![("[FOREIGN]", "owner")]);
        let sys = track("ru", vec![("да", "speaker:0")]);
        assert_eq!(pick_pinned_lang(&mic, Some(&sys)), None);
    }

    #[test]
    fn pick_pinned_lang_uses_mic_when_busier() {
        let mic = track(
            "ru",
            vec![(
                "это длинная реплика владельца на десять слов ровно вот",
                "owner",
            )],
        );
        let sys = track("en", vec![("ok", "speaker:0")]);
        assert_eq!(pick_pinned_lang(&mic, Some(&sys)).as_deref(), Some("ru"));
    }
}
