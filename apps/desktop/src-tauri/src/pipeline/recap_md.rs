//! [B20.3] Детерминированное render-side выделение известных имён в markdown
//! рекапа. JSON-контракт summary не трогаем (строки остаются plain text) —
//! `**bold**` добавляется только при сборке recap.md.
//!
//! Ограничение (by design, MVP): матчинг exact-form — русские склонения
//! («Ивана», «Иваном») не распознаются, стемминга нет. Слабая local-модель
//! пишет имена в именительном падеже достаточно часто, чтобы это окупалось.

/// Минимальная длина имени/first-name токена для матчинга — отсекает
/// инициалы («И.») и служебные частицы.
const MIN_NAME_CHARS: usize = 3;

/// Выделить `**жирным**` первое вхождение каждого известного имени в `text`.
///
/// Правила:
/// - кандидаты = полные display_name + first-name токены (≥3 символов),
///   longest-first (чтобы «Глеб Гусак» матчился раньше «Глеб»);
/// - только whole-word (Unicode-границы): «Иван» не матчит «Иванов»;
/// - пропускаем вхождения, уже обёрнутые в `*`/`` ` `` (не вкладываем бold).
// Имя с markdown-метасимволами не болдим: обёртка вокруг сырого бэктика,
// звёздочки или подчёркивания рассинхронизирует парсинг всего документа
// (bold-маркеры проглатываются code-span'ом и т.п.).
fn is_markdown_safe(name: &str) -> bool {
    !name.contains(['*', '`', '_', '[', ']', '\\'])
}

pub(crate) fn bold_known_names(text: &str, names: &[String]) -> String {
    let mut candidates: Vec<String> = Vec::new();
    for raw in names {
        let full = raw.trim();
        if !is_markdown_safe(full) {
            continue;
        }
        if full.chars().count() >= MIN_NAME_CHARS {
            candidates.push(full.to_string());
        }
        if let Some(first) = full.split_whitespace().next() {
            if first != full && first.chars().count() >= MIN_NAME_CHARS {
                candidates.push(first.to_string());
            }
        }
    }
    // Length-desc + лексикографический tiebreak: настоящие дубликаты (имя
    // и в participants, и в contacts) становятся смежными — иначе dedup()
    // (только consecutive) их пропускал бы и болдил имя дважды.
    candidates.sort_by(|a, b| {
        b.chars()
            .count()
            .cmp(&a.chars().count())
            .then_with(|| a.cmp(b))
    });
    candidates.dedup();

    let mut result = text.to_string();
    for cand in &candidates {
        result = bold_first_valid_occurrence(&result, cand);
    }
    result
}

/// Границы слова + не внутри уже существующей markdown-разметки.
fn is_boundary(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(c) => !c.is_alphanumeric() && c != '*' && c != '`',
    }
}

fn bold_first_valid_occurrence(text: &str, name: &str) -> String {
    for (idx, matched) in text.match_indices(name) {
        let before = text[..idx].chars().next_back();
        let after = text[idx + matched.len()..].chars().next();
        if is_boundary(before) && is_boundary(after) {
            return format!(
                "{}**{}**{}",
                &text[..idx],
                matched,
                &text[idx + matched.len()..]
            );
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bolds_name_at_start_middle_end() {
        let n = names(&["Иван"]);
        assert_eq!(
            bold_known_names("Иван начал встречу", &n),
            "**Иван** начал встречу"
        );
        assert_eq!(
            bold_known_names("Потом Иван ушёл", &n),
            "Потом **Иван** ушёл"
        );
        assert_eq!(
            bold_known_names("Решение принял Иван", &n),
            "Решение принял **Иван**"
        );
    }

    #[test]
    fn does_not_bold_inside_longer_word() {
        let n = names(&["Иван"]);
        assert_eq!(bold_known_names("Иванов доложил", &n), "Иванов доложил");
    }

    #[test]
    fn full_name_wins_over_first_name_token() {
        let n = names(&["Глеб Гусак"]);
        assert_eq!(
            bold_known_names("Глеб Гусак согласился", &n),
            "**Глеб Гусак** согласился"
        );
    }

    #[test]
    fn first_name_token_matches_alone() {
        let n = names(&["Глеб Гусак"]);
        assert_eq!(
            bold_known_names("Глеб согласился", &n),
            "**Глеб** согласился"
        );
    }

    #[test]
    fn only_first_occurrence_bolded() {
        let n = names(&["Иван"]);
        assert_eq!(
            bold_known_names("Иван сказал, что Иван успеет", &n),
            "**Иван** сказал, что Иван успеет"
        );
    }

    #[test]
    fn skips_already_bolded() {
        let n = names(&["Иван"]);
        assert_eq!(
            bold_known_names("**Иван** уже жирный", &n),
            "**Иван** уже жирный"
        );
    }

    #[test]
    fn empty_names_is_identity() {
        assert_eq!(
            bold_known_names("текст без правок", &[]),
            "текст без правок"
        );
    }

    #[test]
    fn short_tokens_ignored() {
        // Инициалы/двухбуквенные не матчим.
        let n = names(&["Ян"]);
        assert_eq!(bold_known_names("Ян пришёл", &n), "Ян пришёл");
    }

    #[test]
    fn multiple_names_each_bolded() {
        let n = names(&["Иван Петров", "Мария Ли"]);
        assert_eq!(
            bold_known_names("Иван передал задачу, Мария приняла", &n),
            "**Иван** передал задачу, **Мария** приняла"
        );
    }

    #[test]
    fn punctuation_is_a_boundary() {
        let n = names(&["Иван"]);
        assert_eq!(bold_known_names("Спасибо, Иван!", &n), "Спасибо, **Иван**!");
    }

    #[test]
    fn duplicate_name_in_list_still_bolds_only_first_occurrence() {
        // Имя и в participants, и в contacts (типовой случай) + чужое имя той
        // же длины между ними — dedup обязан схлопнуть дубликат.
        let n = names(&["Alice", "Maria", "Alice"]);
        assert_eq!(
            bold_known_names("Alice opened. Maria agreed. Later Alice confirmed.", &n),
            "**Alice** opened. **Maria** agreed. Later Alice confirmed."
        );
    }

    #[test]
    fn markdown_metachars_in_name_skip_bolding() {
        let n = names(&["Alex`Coder", "Иван"]);
        assert_eq!(
            bold_known_names("Alex`Coder и Иван договорились", &n),
            "Alex`Coder и **Иван** договорились"
        );
    }
}
