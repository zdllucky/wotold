//! [M12.3 / TD-15] Сборка prompt'а для локальной модели и безопасная обрезка
//! строк по границе символа.
//!
//! [TD-41] Выделено из `local_engine/llm.rs` (1063 строки при лимите 800,
//! правило 8) вместе с тестами. Обрезка живёт здесь же не для красоты: она
//! применяется к stderr-хвосту в аварийных ветках, и её тесты — регрессия на
//! панику «byte index is not a char boundary», которая случалась ровно там,
//! где юзер вместо ошибки получал панику таски. Логика не менялась.

use crate::providers::llm::LlmRequest;

/// Собрать финальный prompt: system + двойной перенос + transcript.
pub(crate) fn build_prompt(request: &LlmRequest) -> String {
    let mut s = String::with_capacity(request.system.len() + request.input.len() + 4);
    s.push_str(&request.system);
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s.push('\n');
    s.push_str(&request.input);
    s
}

/// [TD-15] Обрезать строку по границе символа, не длиннее `max_bytes`.
///
/// Раньше здесь стоял `&s[..s.len().min(512)]` — байтовый срез по строке из
/// `from_utf8_lossy`. Если байт 512 попадал внутрь многобайтового символа
/// (кириллица в путях GGUF, либо 3-байтовый U+FFFD, который сам lossy и
/// вставляет), это паниковало «byte index 512 is not a char boundary» — ровно
/// в аварийных ветках (exit code != 0, timeout), то есть вместо внятной
/// ошибки юзер получал панику async-таски пайплайна.
///
/// `str::floor_char_boundary` до сих пор нестабилен (проверено на rustc 1.95),
/// поэтому откатываемся вручную.
pub(crate) fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // [TD-15] truncate_at_char_boundary — паника в error-path
    // ============================================================

    #[test]
    fn truncate_returns_whole_string_when_under_limit() {
        assert_eq!(truncate_at_char_boundary("short", 512), "short");
        assert_eq!(truncate_at_char_boundary("", 512), "");
    }

    #[test]
    fn truncate_cyrillic_over_limit_does_not_panic() {
        // Регрессия: `&s[..s.len().min(512)]` паниковал, если байт 512 попадал
        // внутрь многобайтового символа.
        //
        // ВАЖНО про конструкцию: одной кириллицы мало. Все её символы
        // двухбайтовые, поэтому границы стоят на чётных смещениях, а 512 —
        // чётное: срез бы прошёл и тест ничего не проверял. Ведущий ASCII-байт
        // сдвигает границы на нечётные, и 512 гарантированно попадает ВНУТРЬ
        // символа. (Проверено: без сдвига тест зеленел на сломанном коде.)
        let s = format!("x{}", "стдерр".repeat(200));
        assert!(s.len() > 512);
        assert!(!s.is_char_boundary(512), "512 обязан быть внутри символа");
        let out = truncate_at_char_boundary(&s, 512);
        assert!(out.len() <= 512);
        assert!(s.starts_with(out), "префикс исходной строки");
        // Результат — валидный &str: перекодировка туда-обратно без потерь.
        assert_eq!(out, String::from_utf8(out.as_bytes().to_vec()).unwrap());
    }

    #[test]
    fn truncate_lands_exactly_on_boundary() {
        // "аб" = 4 байта (2+2). Лимит 3 попадает в середину 'б' → откат до 2.
        assert_eq!(truncate_at_char_boundary("аб", 3), "а");
        assert_eq!(truncate_at_char_boundary("аб", 2), "а");
        assert_eq!(truncate_at_char_boundary("аб", 1), "");
        assert_eq!(truncate_at_char_boundary("аб", 0), "");
    }

    #[test]
    fn truncate_handles_replacement_char() {
        // from_utf8_lossy вставляет U+FFFD (3 байта) — он сам может попасть на
        // границу лимита.
        let lossy = String::from_utf8_lossy(&[0xFF, 0xFE, 0xFD]).into_owned();
        for limit in 0..=lossy.len() {
            let out = truncate_at_char_boundary(&lossy, limit);
            assert!(out.len() <= limit);
        }
    }

    // ── build_prompt ────────────────────────────────────────────────────

    #[test]
    fn build_prompt_concatenates_system_and_input() {
        let req = LlmRequest {
            model: None,
            system: "SYS".into(),
            input: "BODY".into(),
            max_tokens: None,
            grammar: None,
            json_schema: None,
        };
        let p = build_prompt(&req);
        assert!(p.starts_with("SYS"));
        assert!(p.ends_with("BODY"));
        assert!(
            p.contains("SYS\n\nBODY"),
            "missing blank separator, got: {p:?}"
        );
    }

    #[test]
    fn build_prompt_normalizes_trailing_newline() {
        // system уже с newline → не плодим лишних \n\n\n
        let req = LlmRequest {
            model: None,
            system: "SYS\n".into(),
            input: "BODY".into(),
            max_tokens: None,
            grammar: None,
            json_schema: None,
        };
        let p = build_prompt(&req);
        assert!(p.contains("SYS\n\nBODY"));
        assert!(!p.contains("SYS\n\n\nBODY"));
    }
}
