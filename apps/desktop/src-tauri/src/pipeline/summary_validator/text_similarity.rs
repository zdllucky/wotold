//! Примитивы текстового сходства для валидации рекапа: sliding-window
//! Levenshtein для проверки цитат и Jaccard по токенам для дедупа.
//!
//! Выделены из `summary_validator.rs` при TD-18 — тот упёрся в лимит когезии
//! 800 строк, и добавить в него cap длины цитаты стало нельзя (инженерное
//! правило 8). Граница модуля естественная: здесь чистые функции над строками,
//! ничего не знающие про `CallSummaryV2`; там — доменная валидация.
//!
//! Подмодуль `summary_validator`, а не сосед по `pipeline/` — `pipeline/mod.rs`
//! сам давно за лимитом (2461 строка), и добавить туда даже одну строку
//! `pub mod` нельзя. Заодно это честнее отражает область применения:
//! потребитель ровно один.
//!
//! Своя реализация, а не rapidfuzz/strsim: для порога 0.85–0.95 наивной
//! Левенштейн-similarity достаточно, а тянуть 50KB+ зависимости в offline-сборку
//! не хочется.

/// [TD-18] Потолок длины цитаты в символах. Стоимость сравнения —
/// O(haystack · needle²): часовой транскрипт (~60k символов) против
/// неусечённой 2000-символьной цитаты — порядка 2.4·10¹¹ операций, то есть
/// минуты на одном ядре. 200 символов достаточно, чтобы отличить настоящую
/// цитату от выдуманной.
///
/// Раньше этот cap был только на словах: комментарий у `levenshtein`
/// утверждал «m, n ≤ 200 (quote length cap)», а enforcement'а не было нигде —
/// `evidence.quote` приходит от LLM какой угодно длины.
const MAX_QUOTE_CHARS: usize = 200;

/// Усечь по границе символа, не по байту (`floor_char_boundary` до сих пор
/// nightly-only). Тот же приём, что в `local_engine::llm` после TD-15.
fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

/// Substring fuzzy match — возвращает best similarity score (0..1) для
/// `needle` против `haystack`. 1.0 = literal match found; 0.0 = ничего
/// близкого.
///
/// Algorithm: sliding-window Levenshtein на normalized текстах. Lower +
/// collapse whitespace на обоих перед сравнением — устраняет false
/// negatives от форматирования.
pub fn substring_fuzzy_score(needle: &str, haystack: &str) -> f32 {
    let needle_norm = normalize(needle);
    let haystack_norm = normalize(haystack);
    if needle_norm.is_empty() {
        return 0.0;
    }
    // Fast path: literal match → 1.0. Проверяется ДО усечения: дословно
    // найденная длинная цитата обязана давать 1.0 целиком, а не по первым
    // 200 символам. Заодно это те самые ~95% случаев, где fuzzy не нужен.
    if haystack_norm.contains(&needle_norm) {
        return 1.0;
    }
    // [TD-18] Дальше идёт квадратичная часть — только на усечённой цитате.
    let needle_norm = truncate_chars(&needle_norm, MAX_QUOTE_CHARS);
    let n_chars: Vec<char> = needle_norm.chars().collect();
    let h_chars: Vec<char> = haystack_norm.chars().collect();
    let n_len = n_chars.len();
    let h_len = h_chars.len();
    if h_len < n_len {
        // Haystack короче needle — partial best vs full needle.
        return partial_similarity(&n_chars, &h_chars);
    }
    let mut best = 0.0_f32;
    // Slide window len=n_len по haystack; на каждом window считаем
    // similarity = 1 - edit_distance / n_len.
    let max_start = h_len.saturating_sub(n_len);
    for start in 0..=max_start {
        let window = &h_chars[start..start + n_len];
        let sim = partial_similarity(&n_chars, window);
        if sim > best {
            best = sim;
            if best >= 0.999 {
                // Достаточно близко к 1.0, ранний выход.
                return best;
            }
        }
    }
    best
}

/// Levenshtein-similarity для two slices одной длины (approximately).
/// Returns 1 - normalized_edit_distance в [0, 1].
fn partial_similarity(a: &[char], b: &[char]) -> f32 {
    let dist = levenshtein(a, b);
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    1.0 - (dist as f32) / (max_len as f32)
}

/// Naive O(n·m) Levenshtein distance. `n` ограничен `MAX_QUOTE_CHARS`
/// вызывающим `substring_fuzzy_score`, `m` равен ему же (окно той же длины).
fn levenshtein(a: &[char], b: &[char]) -> usize {
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0_usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Normalize: lowercase + collapse whitespace + trim. Устраняет
/// false-negative от \r\n / leading spaces / capitalization.
pub fn normalize(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_was_space = false;
    for c in lower.chars() {
        if c.is_whitespace() {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Jaccard token overlap для dedup: разбивает обе строки на lowercase-токены,
/// returns intersection_size / union_size.
pub fn jaccard_token_overlap(a: &str, b: &str) -> f32 {
    let a_tokens: std::collections::HashSet<String> = a
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    let b_tokens: std::collections::HashSet<String> = b
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    if a_tokens.is_empty() && b_tokens.is_empty() {
        return 1.0;
    }
    let inter = a_tokens.intersection(&b_tokens).count();
    let union = a_tokens.union(&b_tokens).count();
    if union == 0 {
        return 0.0;
    }
    inter as f32 / union as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    // ──────── substring_fuzzy_score ────────

    #[test]
    fn literal_substring_match_returns_1() {
        let score = substring_fuzzy_score(
            "I'll send the proposal",
            "Customer: yes please. I'll send the proposal by tomorrow.",
        );
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn whitespace_difference_still_near_1() {
        // Cloud LLM может вернуть quote с одинарным пробелом, transcript — с
        // двойным. Normalize должен collapse'нуть.
        let score = substring_fuzzy_score("hello world", "alice: hello  world  back");
        assert!(score >= 0.99, "got {score}");
    }

    #[test]
    fn case_difference_normalized() {
        let score = substring_fuzzy_score("Hello World", "alice: hello world back");
        assert!(score >= 0.99, "got {score}");
    }

    #[test]
    fn pure_hallucination_low_score() {
        let score = substring_fuzzy_score(
            "we agreed to ship enterprise tier",
            "Alice: just here to say hi and goodbye.",
        );
        // Common tokens: zero. Полностью разные тексты.
        assert!(score < 0.5, "got {score}");
    }

    #[test]
    fn fuzzy_threshold_boundary_typo_passes() {
        // 1 typo на 24 chars = ~96% similarity → passes 0.90 threshold.
        let needle = "I will send the report"; // 22 chars
        let transcript = "Bob: yeah, I willl send the report tonight."; // typo: willl vs will
        let score = substring_fuzzy_score(needle, transcript);
        assert!(score >= 0.90, "expected ≥ 0.90, got {score}");
    }

    // ──────── [TD-18] quote length cap ────────

    #[test]
    fn truncate_chars_respects_multibyte_boundary() {
        // Регрессия: усечение по байтам разрубило бы кириллицу пополам и
        // паника «byte index N is not a char boundary». Каждая буква — 2
        // байта, значит граница 5 символов = байт 10, а не 5.
        let s = "привет мир";
        let cut = truncate_chars(s, 5);
        assert_eq!(cut, "приве");
        assert_eq!(cut.len(), 10, "5 двухбайтовых символов = 10 байт");
    }

    #[test]
    fn truncate_chars_passes_through_when_short() {
        assert_eq!(truncate_chars("short", 200), "short");
    }

    #[test]
    fn long_hallucinated_quote_is_capped_before_levenshtein() {
        // Регрессия TD-18: цитата от LLM ничем не ограничена по длине, а
        // fast-path `contains` на фабрикациях не срабатывает — значит в
        // квадратичную часть уходил весь needle целиком.
        //
        // Проверяем инвариант, а не время (тесты на время флаки). Конструкция:
        // 185 символов реальной цитаты + 1000 символов мусора. Haystack — 224
        // символа, реальная цитата внутри.
        //   без cap: n_len(1185) > h_len(224) → ветка `h_len < n_len`, счёт
        //            против ПОЛНОГО needle, дистанция ≈1000 → score ≈ 0.16;
        //   с cap:   n_len(200) ≤ 224 → sliding window находит цитату,
        //            расходится только хвост в 15 символов → score ≈ 0.93.
        let real = "мы обсудили условия поставки и сроки ".repeat(5);
        assert!(
            real.chars().count() < MAX_QUOTE_CHARS,
            "цитата влезает в cap"
        );
        let needle = format!("{real}{}", "ы".repeat(1000));
        let haystack = format!("Алиса: {real} и на этом всё, до связи завтра.");

        let score = substring_fuzzy_score(&needle, &haystack);
        assert!(
            score > 0.85,
            "усечённый needle обязан находить цитату sliding-window'ом, got {score}"
        );
    }

    #[test]
    fn verbatim_long_quote_still_scores_1() {
        // Cap не должен ломать честную длинную цитату: fast-path `contains`
        // стоит ДО усечения.
        let quote = "мы обсудили условия ".repeat(30); // 600 символов
        let haystack = format!("Боб: {quote} и на этом всё.");
        let score = substring_fuzzy_score(&quote, &haystack);
        assert!((score - 1.0).abs() < 1e-6, "got {score}");
    }

    // ──────── jaccard_token_overlap ────────

    #[test]
    fn jaccard_identical_is_1() {
        assert!((jaccard_token_overlap("ship the beta", "Ship The Beta") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn jaccard_disjoint_is_0() {
        assert_eq!(jaccard_token_overlap("alpha beta", "gamma delta"), 0.0);
    }
}
