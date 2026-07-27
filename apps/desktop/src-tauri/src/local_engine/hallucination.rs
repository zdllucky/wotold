//! Фильтр whisper-галлюцинаций: строковые правила поверх сегмента.
//!
//! Выделен из `stt.rs` при TD-20 — тот перевалил за лимит когезии 800 строк,
//! и править фильтр на месте стало нельзя (инженерное правило 8). Граница
//! естественная: здесь чистые функции над текстом сегмента, ничего не знающие
//! про сайдкар и JSON-схему whisper.cpp.
//!
//! Потребители — не только `stt`: `pipeline::merge::sanitize_merged` и
//! `pipeline::chunk_runner::real_word_count` зовут тот же фильтр, поэтому
//! модуль лежит рядом со `stt`, а не внутри него.

/// [P12.1] Whisper hallucination exact-match patterns (lowercase).
/// Расширенный список — English boundary fillers + blank/music tags +
/// `[FOREIGN]` language-confusion tag + multilingual silence markers.
const HALLUCINATIONS_EXACT: &[&str] = &[
    // Existing English/blank/music (M13 baseline).
    "you",
    "thank you",
    "thanks",
    "bye",
    "goodbye",
    "thanks for watching",
    "[blank_audio]",
    "(silence)",
    "[music]",
    "(music)",
    "[applause]",
    // [P12.1] Language-confusion tag — whisper выдаёт когда detect failed
    // на сегменте. Чаще всего после тишины либо короткого шума.
    "[foreign]",
    // Multilingual silence/audio markers — YouTube training contamination.
    "[音楽]",
    "[bgm]",
    "[♪音楽♪]",
    // [P-fix2] Cyrillic noise tags — whisper выдаёт на музыке/тишине/аплодисментах
    // (русская озвучка YouTube training). Latin-аналоги уже выше.
    "[музыка]",
    "[аплодисменты]",
    "[смех]",
    "[шум]",
    "[тишина]",
];

/// [P12.1] Substring patterns — lowercase substring match (case-insensitive
/// через `.to_lowercase()` на input). Покрывает Russian subtitle-credit
/// hallucinations из YouTube training data Whisper'а.
///
/// Пример из реальных данных user'а: «[Редактор субтитров Н.Александрова]
/// [Апалькова]» × N раз. Это classic YouTube-subtitler attribution.
const HALLUCINATION_SUBSTRINGS: &[&str] = &[
    // Russian YouTube subtitle credits.
    "редактор субтитров",
    "корректор субтитров",
    "субтитры подготовил",
    "субтитры:",
    "[апалькова",
    "[александрова",
    "н.александрова",
    "а.семкин",
    // Generic YouTube attribution patterns (multilingual).
    "subtitles by",
    "transcribed by",
    "продолжение следует",
];

/// [P12.1] Проверить, является ли сегмент hallucination'ом по совокупности
/// признаков. Используется в `build_transcript` filter.
///
/// Order:
/// 1. Exact-match (lowercase) против HALLUCINATIONS_EXACT.
/// 2. Substring (lowercase) против HALLUCINATION_SUBSTRINGS.
/// 3. Bracket-only shape: текст полностью в `[...]` длиной >20 — почти
///    наверняка YouTube subtitle credit или similar artifact. Безопасный
///    floor: legit `[music]` (5 chars) и `[applause]` (10) уже в exact list.
pub(crate) fn is_hallucination(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    // [P-fix] Strip ведущие dialogue-маркеры ("- ", "– ", "— ", "•", "*")
    // которые whisper иногда добавляет к сегменту — без этого exact-match по
    // "[foreign]" промахивается на "- [FOREIGN]".
    let stripped = trimmed.trim_start_matches(['-', '–', '—', '•', '*']).trim();
    if stripped.is_empty() {
        return true;
    }
    let lower = stripped.to_lowercase();
    if HALLUCINATIONS_EXACT.contains(&lower.as_str()) {
        return true;
    }
    if HALLUCINATION_SUBSTRINGS.iter().any(|p| lower.contains(p)) {
        return true;
    }
    // [P-fix] Сегмент, чей alnum-контент == "foreign" И содержит скобку
    // (например "[FOREIGN]", "- [FOREIGN]", "[Foreign].") — language-confusion
    // tag. Скобка обязательна чтобы не дропнуть легитимное слово "foreign".
    let alnum: String = lower.chars().filter(|c| c.is_alphanumeric()).collect();
    if alnum == "foreign" && stripped.contains('[') {
        return true;
    }
    // Bracket-only shape: текст начинается с '[' и заканчивается ']',
    // длина >20 chars (catch'ит длинные attribution патены). Короткие
    // bracket tags типа `[music]` пропускаются (уже в exact list если
    // hallucination).
    if stripped.starts_with('[') && stripped.ends_with(']') && stripped.chars().count() > 20 {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // [P12.1] Расширенный hallucination filter — Russian subtitle credits,
    // [FOREIGN] tag, generic bracket-only shape.
    #[test]
    fn is_hallucination_filters_foreign_tag() {
        assert!(is_hallucination("[FOREIGN]"));
        assert!(is_hallucination("[foreign]"));
        assert!(is_hallucination("  [FOREIGN]  "));
    }

    #[test]
    fn is_hallucination_filters_foreign_tag_with_dash_or_punct() {
        // [P-fix] whisper иногда префиксует dialogue-dash или trailing-точку.
        assert!(is_hallucination("- [FOREIGN]"));
        assert!(is_hallucination("— [FOREIGN]"));
        assert!(is_hallucination("[FOREIGN]."));
        assert!(is_hallucination("- [foreign] "));
        // Legit слово "foreign" без скобок — НЕ дропаем.
        assert!(!is_hallucination("foreign policy was discussed"));
        // Dialogue-dash перед реальной речью — НЕ дропаем.
        assert!(!is_hallucination("- Добрый день, ещё раз."));
    }

    #[test]
    fn is_hallucination_filters_russian_subtitle_credits() {
        assert!(is_hallucination("[Редактор субтитров Н.Александрова]"));
        assert!(is_hallucination("[Апалькова]"));
        assert!(is_hallucination("[Александрова]"));
        assert!(is_hallucination(
            "[Редактор субтитров Н.Александрова] [Апалькова]"
        ));
        assert!(is_hallucination("[Субтитры подготовил пользователь]"));
        assert!(is_hallucination("Subtitles by Anonymous"));
        assert!(is_hallucination("Transcribed by AI"));
    }

    #[test]
    fn is_hallucination_filters_existing_exact_matches() {
        // Backward-compat — existing list still active.
        assert!(is_hallucination("you"));
        assert!(is_hallucination("Thank you"));
        assert!(is_hallucination("[blank_audio]"));
        assert!(is_hallucination("(silence)"));
        assert!(is_hallucination("[music]"));
        // [P-fix2] Cyrillic noise tags.
        assert!(is_hallucination("[Музыка]"));
        assert!(is_hallucination("[музыка]"));
        assert!(is_hallucination("[Аплодисменты]"));
        assert!(is_hallucination("[Смех]"));
    }

    #[test]
    fn is_hallucination_filters_long_bracket_only_shape() {
        // Generic: bracket-only длина >20 chars → hallucination shape.
        assert!(is_hallucination("[Some Long Mystery Attribution Label]"));
        // Короткий bracket tag — не дропаем (если не в exact list).
        // Эти НЕ должны фильтроваться:
        assert!(!is_hallucination("[unknown]")); // 9 chars
        assert!(!is_hallucination("[?]"));
    }

    #[test]
    fn is_hallucination_keeps_legit_russian_speech() {
        assert!(!is_hallucination("Привет, как дела?"));
        assert!(!is_hallucination(
            "Мы обсуждали проект, нужно подготовить документы."
        ));
        assert!(!is_hallucination("Да, согласен."));
        // Edge: реальное слово которое случайно matches никаким substring.
        assert!(!is_hallucination("Александр сказал что согласен"));
    }
}
