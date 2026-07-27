//! [M12.3] Выделение JSON-объекта из ответа локальной модели.
//!
//! [TD-41] Выделено из `local_engine/llm.rs` (1063 строки при лимите 800,
//! правило 8) вместе с тестами. Отдельным модулем ещё и потому, что это
//! разбор недоверенного вывода модели с ручной итерацией по байтам —
//! рассуждение о UTF-8-безопасности должно лежать рядом с тестами, которые
//! его держат. Логика не менялась.

/// Найти первый сбалансированный JSON-объект в строке. Модель может
/// выдать чуть-чуть мусора до/после; ищем по brace-counter.
///
/// # UTF-8 safety
///
/// Функция итерирует raw `u8`, но это безопасно для UTF-8 строк по
/// определению кодировки: continuation bytes (0x80..=0xBF) НЕ пересекаются
/// с ASCII-кодами которые мы трекаем (`"` 0x22, `{` 0x7B, `}` 0x7D, `\` 0x5C).
/// Любой multi-byte Unicode codepoint имеет ведущий byte ≥ 0xC0 — тоже вне
/// нашего набора. Поэтому мы не можем «случайно» войти в строку посреди
/// многобайтового символа. Регрессия покрыта `extract_json_handles_escaped_quote_in_string`
/// + `extract_json_handles_nested_braces` тестами.
pub(crate) fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            match b {
                b'\\' => escape = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return std::str::from_utf8(&bytes[start..=i])
                        .ok()
                        .map(String::from);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_json_object ─────────────────────────────────────────────

    #[test]
    fn extract_json_finds_object_among_prose() {
        let s = "leading garbage\n{\"title\":\"X\",\"summary\":\"Y\"}\ntrailing";
        let out = extract_json_object(s).unwrap();
        assert_eq!(out, "{\"title\":\"X\",\"summary\":\"Y\"}");
    }

    #[test]
    fn extract_json_handles_nested_braces() {
        let s = r#"{"a":{"b":{"c":1}},"d":"}}"}"#;
        let out = extract_json_object(s).unwrap();
        // вернёт весь объект, не зацикливается на внутренних `}`
        assert_eq!(out, s);
    }

    #[test]
    fn extract_json_handles_escaped_quote_in_string() {
        let s = r#"{"text":"He said \"hi\" then left"}"#;
        let out = extract_json_object(s).unwrap();
        assert_eq!(out, s);
    }

    #[test]
    fn extract_json_returns_none_when_no_brace() {
        assert!(extract_json_object("plain text no json").is_none());
    }

    #[test]
    fn extract_json_returns_none_when_unbalanced() {
        // Открывающая скобка без закрывающей → нет результата.
        assert!(extract_json_object("{\"a\":1").is_none());
    }
}
