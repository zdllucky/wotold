//! [T7] Нативное уведомление ОС с текстом из фронтенда.
//!
//! # Почему текст приходит из webview
//!
//! Инженерное правило 4: все user-visible строки идут через `t()` и три
//! локали. Уведомления, собранные в Rust, это правило нарушали — до T7
//! `call_detect.rs` держал русские литералы прямо в коде, и казахский с
//! английским видели русский баннер.
//!
//! Плагин уведомлений живёт в Rust, поэтому фронт зовёт эту команду вместо
//! JS-плагина: ни новой зависимости в `package.json`, ни второй копии строк.
//!
//! # Граница доверия
//!
//! Строки приходят из webview (правило 7). В путь и в SQL они не попадают, но
//! уходят в системный API — обрезаем длину, чтобы случайный многомегабайтный
//! текст не ушёл в центр уведомлений, и режем переводы строк в заголовке.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::AppError;

/// Практический предел: macOS всё равно обрежет длинный баннер, а нам нужно
/// не пустить в системный API произвольный объём.
const MAX_TITLE_CHARS: usize = 120;
const MAX_BODY_CHARS: usize = 400;

#[tauri::command]
pub async fn show_notification(
    app: AppHandle,
    title: String,
    body: String,
) -> Result<(), AppError> {
    let title = clamp(&title, MAX_TITLE_CHARS).replace(['\n', '\r'], " ");
    let body = clamp(&body, MAX_BODY_CHARS);
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| AppError::Other(format!("notification failed: {e}")))
}

/// Обрезка по символам, а не по байтам: `String::truncate` на границе
/// многобайтового символа паникует, а тексты у нас кириллические.
fn clamp(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_cuts_by_chars_not_bytes() {
        // «Привет» — 12 байт, 6 символов. Побайтовая обрезка на 5 уронила бы
        // процесс на границе символа.
        assert_eq!(clamp("Привет", 5), "Приве");
        assert_eq!(clamp("Привет", 100), "Привет");
        assert_eq!(clamp("", 10), "");
    }

    #[test]
    fn clamp_is_a_noop_for_short_strings() {
        assert_eq!(clamp("ok", MAX_TITLE_CHARS), "ok");
    }
}
