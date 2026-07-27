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

/// [TD-20] Спорные формулы: whisper выдаёт их на тишине, но люди их же и
/// говорят — особенно в конце звонка. Дропаются **только** при низкой
/// уверенности модели, см. [`is_hallucination`].
///
/// Список может только сокращаться. Добавить сюда слово — значит разрешить
/// удалять его из расшифровки, а это делается не «на всякий случай», а по
/// замерам.
///
/// «thanks for watching» тут нет намеренно: это артефакт YouTube-обучения,
/// в телефонном разговоре не встречается, и остаётся в безусловном списке.
const AMBIGUOUS_FILLERS: &[&str] = &["you", "thank you", "thanks", "bye", "goodbye"];

/// [TD-20] Порог средней вероятности токенов, ниже которого спорная формула
/// считается галлюцинацией.
///
/// Замеры на реальном сайдкаре (whisper-small и whisper-medium, аргументы
/// прода):
/// - тишина 3/8/12 с → сегмент `" you"`, средняя вероятность **0.135**
///   (medium — 0.191);
/// - произнесённые вслух короткие реплики → **0.513…0.999**, минимум даёт
///   как раз односложное «You?».
///
/// Геометрическая середина между худшей галлюцинацией (0.191) и слабейшей
/// реальной речью (0.513) — 0.313. Берём 0.30: чуть в сторону сохранения
/// речи, потому что ошибки несимметричны — потерянная реплика необратима и
/// невидима, лишнее «you» в расшифровке заметно и безобидно.
pub(crate) const MIN_FILLER_CONFIDENCE: f64 = 0.30;

/// [P12.1] Whisper hallucination exact-match patterns (lowercase).
/// Расширенный список — English boundary fillers + blank/music tags +
/// `[FOREIGN]` language-confusion tag + multilingual silence markers.
///
/// [TD-20] Спорные формулы отсюда переехали в [`AMBIGUOUS_FILLERS`]; здесь
/// остались только строки, которых человек не произносит.
const HALLUCINATIONS_EXACT: &[&str] = &[
    // Existing English/blank/music (M13 baseline).
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

/// [TD-20] Галлюцинация ли сегмент. `confidence` — средняя вероятность
/// содержательных токенов whisper'а (`whisper_json::mean_token_probability`),
/// `None` если сигнал недоступен.
///
/// Два класса, и это главное разделение:
/// - **артефакты** ([`is_artifact`]) — строки, которых человек не произносит:
///   `[blank_audio]`, `[FOREIGN]`, субтитр-credits, длинные bracket-формы.
///   Дропаются безусловно, ровно как раньше.
/// - **спорные формулы** ([`AMBIGUOUS_FILLERS`]) — «thanks», «bye» и подобные.
///   Дропаются только при `confidence < MIN_FILLER_CONFIDENCE`.
///
/// При `confidence == None` спорная формула **сохраняется**. Ошибки
/// несимметричны: удалённая реплика необратима и незаметна, лишнее «you»
/// заметно и безобидно. Единственный такой путь на практике — чанки,
/// записанные до этого изменения (в их JSON нет токенов).
pub(crate) fn is_hallucination(text: &str, confidence: Option<f64>) -> bool {
    if is_artifact(text) {
        return true;
    }
    if !is_ambiguous_filler(text) {
        return false;
    }
    match confidence {
        Some(p) => p < MIN_FILLER_CONFIDENCE,
        None => false,
    }
}

/// [TD-20] Спорная ли формула — «thanks», «bye», «you» и т.п.
///
/// Пунктуация игнорируется. Это осознанное расширение: раньше exact-match шёл
/// по голой строке, поэтому `"Thanks."` не совпадал с `thanks` и выживал
/// случайно — whisper почти всегда ставит точку реальной речи. Теперь форма
/// с точкой тоже попадает под правило, но дропнуть её может только низкая
/// вероятность, так что реальная речь по-прежнему цела, а галлюцинация с
/// точкой перестала проходить мимо фильтра.
pub(crate) fn is_ambiguous_filler(text: &str) -> bool {
    let Some(stripped) = strip_markers(text) else {
        return false;
    };
    let normalized = stripped
        .trim_matches(|c: char| c.is_ascii_punctuation() || c == '…')
        .trim()
        .to_lowercase();
    AMBIGUOUS_FILLERS.contains(&normalized.as_str())
}

/// Обрезать пробелы и ведущие dialogue-маркеры. `None` — пусто после обрезки.
fn strip_markers(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    // [P-fix] Ведущие "- ", "– ", "— ", "•", "*" whisper иногда добавляет к
    // сегменту — без обрезки exact-match по "[foreign]" промахивается на
    // "- [FOREIGN]".
    let stripped = trimmed.trim_start_matches(['-', '–', '—', '•', '*']).trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}

/// [P12.1] Безусловный артефакт: строка, которую человек не произносит.
/// Контекст не нужен и не может изменить вердикт.
///
/// Order:
/// 1. Exact-match (lowercase) против HALLUCINATIONS_EXACT.
/// 2. Substring (lowercase) против HALLUCINATION_SUBSTRINGS.
/// 3. Bracket-only shape: текст полностью в `[...]` длиной >20 — почти
///    наверняка YouTube subtitle credit или similar artifact. Безопасный
///    floor: legit `[music]` (5 chars) и `[applause]` (10) уже в exact list.
pub(crate) fn is_artifact(text: &str) -> bool {
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
    fn is_artifact_filters_foreign_tag() {
        assert!(is_artifact("[FOREIGN]"));
        assert!(is_artifact("[foreign]"));
        assert!(is_artifact("  [FOREIGN]  "));
    }

    #[test]
    fn is_artifact_filters_foreign_tag_with_dash_or_punct() {
        // [P-fix] whisper иногда префиксует dialogue-dash или trailing-точку.
        assert!(is_artifact("- [FOREIGN]"));
        assert!(is_artifact("— [FOREIGN]"));
        assert!(is_artifact("[FOREIGN]."));
        assert!(is_artifact("- [foreign] "));
        // Legit слово "foreign" без скобок — НЕ дропаем.
        assert!(!is_artifact("foreign policy was discussed"));
        // Dialogue-dash перед реальной речью — НЕ дропаем.
        assert!(!is_artifact("- Добрый день, ещё раз."));
    }

    #[test]
    fn is_artifact_filters_russian_subtitle_credits() {
        assert!(is_artifact("[Редактор субтитров Н.Александрова]"));
        assert!(is_artifact("[Апалькова]"));
        assert!(is_artifact("[Александрова]"));
        assert!(is_artifact(
            "[Редактор субтитров Н.Александрова] [Апалькова]"
        ));
        assert!(is_artifact("[Субтитры подготовил пользователь]"));
        assert!(is_artifact("Subtitles by Anonymous"));
        assert!(is_artifact("Transcribed by AI"));
    }

    #[test]
    fn is_artifact_filters_existing_exact_matches() {
        // [TD-20] «you» и «Thank you» отсюда УБРАНЫ намеренно: раньше тест
        // утверждал `is_hallucination("you") == true` безусловно, то есть
        // фиксировал сам баг. Теперь это спорные формулы, вердикт по ним —
        // в `ambiguous_filler_table`.
        assert!(!is_artifact("you"), "спорная формула, не артефакт");
        assert!(!is_artifact("Thank you"), "спорная формула, не артефакт");
        // «thanks for watching» остаётся безусловным: в телефонном
        // разговоре не встречается, это след YouTube-обучения.
        assert!(is_artifact("Thanks for watching"));
        assert!(is_artifact("[blank_audio]"));
        assert!(is_artifact("(silence)"));
        assert!(is_artifact("[music]"));
        // [P-fix2] Cyrillic noise tags.
        assert!(is_artifact("[Музыка]"));
        assert!(is_artifact("[музыка]"));
        assert!(is_artifact("[Аплодисменты]"));
        assert!(is_artifact("[Смех]"));
    }

    #[test]
    fn is_artifact_filters_long_bracket_only_shape() {
        // Generic: bracket-only длина >20 chars → hallucination shape.
        assert!(is_artifact("[Some Long Mystery Attribution Label]"));
        // Короткий bracket tag — не дропаем (если не в exact list).
        // Эти НЕ должны фильтроваться:
        assert!(!is_artifact("[unknown]")); // 9 chars
        assert!(!is_artifact("[?]"));
    }

    #[test]
    fn is_artifact_keeps_legit_russian_speech() {
        assert!(!is_artifact("Привет, как дела?"));
        assert!(!is_artifact(
            "Мы обсуждали проект, нужно подготовить документы."
        ));
        assert!(!is_artifact("Да, согласен."));
        // Edge: реальное слово которое случайно matches никаким substring.
        assert!(!is_artifact("Александр сказал что согласен"));
    }

    // ── [TD-20] спорные формулы: вердикт решает уверенность ──────────────

    /// Таблица кейсов. `confidence` взята из замеров на реальном сайдкаре
    /// (см. описание TD-20): тишина даёт 0.135–0.191, произнесённая вслух
    /// короткая реплика — 0.513 и выше.
    #[test]
    fn ambiguous_filler_table() {
        let cases: &[(&str, Option<f64>, bool, &str)] = &[
            // текст, confidence, ожидаем дроп, почему
            (
                "you",
                Some(0.135),
                true,
                "канонический вывод whisper на 3с тишины",
            ),
            (
                "you",
                Some(0.191),
                true,
                "то же на medium — худшая наблюдённая галлюцинация",
            ),
            (
                "you",
                Some(0.520),
                false,
                "реально произнесённое «You» — та же строка, вдвое увереннее",
            ),
            (
                "Thanks.",
                Some(0.699),
                false,
                "реальная реплика; раньше выживала случайно, из-за точки",
            ),
            (
                "thanks.",
                Some(0.12),
                true,
                "галлюцинация с точкой — раньше проходила мимо фильтра",
            ),
            ("Bye.", Some(0.767), false, "реальная реплика"),
            ("Goodbye", Some(0.696), false, "реальная реплика без точки"),
            (
                "- Thank you.",
                Some(0.713),
                false,
                "dialogue-маркер не меняет вердикт",
            ),
            (
                "Thank you",
                Some(0.05),
                true,
                "очень низкая уверенность — галлюцинация",
            ),
            (
                "you",
                None,
                false,
                "сигнала нет (чанк записан до TD-20) — сохраняем: \
                 потерянная реплика необратима, лишнее «you» безобидно",
            ),
            (
                "Спасибо.",
                Some(0.10),
                false,
                "не в списке спорных — низкая уверенность сама по себе \
                 не повод удалять речь",
            ),
            (
                "Thanks for watching",
                Some(0.99),
                true,
                "артефакт: высокая уверенность не спасает",
            ),
            (
                "[blank_audio]",
                Some(0.99),
                true,
                "артефакт: контекст не рассматривается",
            ),
            (
                "Редактор субтитров А.Семкин",
                Some(0.805),
                true,
                "субтитр-credit с тишины идёт с ВЫСОКОЙ уверенностью — \
                 именно поэтому substring-список обязан остаться",
            ),
        ];
        for (text, confidence, expect_drop, why) in cases {
            assert_eq!(
                is_hallucination(text, *confidence),
                *expect_drop,
                "{text:?} @ {confidence:?} — {why}"
            );
        }
    }

    #[test]
    fn threshold_boundary_is_exclusive_below() {
        // Ровно на пороге — сохраняем. Граница проверяется явно, чтобы
        // сдвиг константы не прошёл молча.
        assert!(!is_hallucination("bye", Some(MIN_FILLER_CONFIDENCE)));
        assert!(is_hallucination("bye", Some(MIN_FILLER_CONFIDENCE - 1e-9)));
    }

    #[test]
    fn ambiguous_list_cannot_swallow_longer_utterances() {
        // Нормализация трогает только пунктуацию по краям. Реплика, в
        // которой спорное слово лишь часть — не спорная формула.
        for text in [
            "you know, we should ship it",
            "thanks a lot for the update",
            "bye for now, talk tomorrow",
            "Thank you very much",
        ] {
            assert!(
                !is_hallucination(text, Some(0.01)),
                "{text:?} не должен дропаться даже при нулевой уверенности"
            );
        }
    }
}
