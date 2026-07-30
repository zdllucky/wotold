//! [M15.8] Команды ассистента — тонкие обёртки над `crate::assistant` и
//! `db::assistant`. Логика (retrieval/budget/LLM/persist) покрыта тестами
//! ядра (`assistant::ask_core`, repository) — здесь только валидация границы
//! и проброс. `assistant_ask` — macOS-only (R9: локальный движок).

use tauri::State;

use crate::assistant::types::{AssistantChatMeta, AssistantIndexStats, AssistantMessage};
use crate::db::assistant as repo;
use crate::state::AppState;
use crate::AppError;

/// Максимум длины вопроса на границе (символов). Retrieval лимитирует токены
/// сам, но простыню режем до входа в конвейер.
const QUESTION_MAX_CHARS: usize = 2_000;

/// Валидация вопроса на границе. Pure — тестируется без State/AppHandle.
fn validate_question(question: &str) -> Result<(), AppError> {
    let q = question.trim();
    if q.is_empty() {
        return Err(AppError::Other("assistant: пустой вопрос".into()));
    }
    // Дешёвый байтовый гейт до посимвольного счёта (кириллица ≤4 байта/симв).
    if q.len() > QUESTION_MAX_CHARS * 4 || q.chars().count() > QUESTION_MAX_CHARS {
        return Err(AppError::Other(format!(
            "assistant: вопрос длиннее {QUESTION_MAX_CHARS} символов"
        )));
    }
    Ok(())
}

/// [B26.4] Лимит превью фрагмента «Контекста поиска» на wire (символов).
/// Полный текст остаётся в answer_json; фронт догружает лениво.
const FRAGMENT_PREVIEW_CHARS: usize = 280;
/// Гистерезис: текст чуть длиннее лимита не усекаем (флаг ради 40 символов
/// не стоит round-trip'а).
const FRAGMENT_PREVIEW_HYSTERESIS: usize = 60;

/// Усечение по символам до последнего пробела + «…». None — усекать нечего.
fn truncate_fragment_text(text: &str) -> Option<String> {
    if text.chars().count() <= FRAGMENT_PREVIEW_CHARS + FRAGMENT_PREVIEW_HYSTERESIS {
        return None;
    }
    let cut: String = text.chars().take(FRAGMENT_PREVIEW_CHARS).collect();
    // До границы слова, но не дальше половины превью (одно гига-слово).
    let end = match cut.rfind(char::is_whitespace) {
        Some(i) if i >= cut.len() / 2 => i,
        _ => cut.len(),
    };
    Some(format!("{}…", cut[..end].trim_end()))
}

/// [B26.4] Усечь фрагменты ответа перед отдачей на фронт (persist не
/// трогается — полный текст остаётся в answer_json).
fn truncate_answer_for_wire(msg: &mut AssistantMessage) {
    if let Some(ans) = msg.answer.as_mut() {
        for f in ans.fragments.iter_mut() {
            if let Some(short) = truncate_fragment_text(&f.text) {
                f.text = short;
                f.text_truncated = true;
            }
        }
    }
}

/// Чип «в поиске X из Y звонков · ЧЧ ч ММ мин».
#[tauri::command]
pub async fn assistant_index_stats(
    state: State<'_, AppState>,
) -> Result<AssistantIndexStats, AppError> {
    repo::index_stats(&state.db).await
}

/// Глобальные чаты раздела, свежие сверху.
#[tauri::command]
pub async fn assistant_list_chats(
    state: State<'_, AppState>,
) -> Result<Vec<AssistantChatMeta>, AppError> {
    repo::list_global_chats(&state.db).await
}

/// Сообщения чата по порядку.
#[tauri::command]
pub async fn assistant_get_chat(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<Vec<AssistantMessage>, AppError> {
    let mut messages = repo::get_chat_messages(&state.db, &chat_id).await?;
    messages.iter_mut().for_each(truncate_answer_for_wire);
    Ok(messages)
}

/// Тред звонка (chat + messages), если существует.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantCallThread {
    pub chat: AssistantChatMeta,
    pub messages: Vec<AssistantMessage>,
}

#[tauri::command]
pub async fn assistant_get_call_thread(
    state: State<'_, AppState>,
    call_id: String,
) -> Result<Option<AssistantCallThread>, AppError> {
    let Some(chat) = repo::get_call_chat(&state.db, &call_id).await? else {
        return Ok(None);
    };
    let mut messages = repo::get_chat_messages(&state.db, &chat.id).await?;
    messages.iter_mut().for_each(truncate_answer_for_wire);
    Ok(Some(AssistantCallThread { chat, messages }))
}

/// Удалить чат (messages каскадом). Идемпотентно.
#[tauri::command]
pub async fn assistant_delete_chat(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<(), AppError> {
    repo::delete_chat(&state.db, &chat_id).await
}

/// Вопрос ассистенту: классификатор → retrieval → budget → LLM → persist.
/// Возвращает готовое assistant-сообщение (user-сообщение уже в БД —
/// фронт рендерит оптимистично и подтягивает тред).
#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn assistant_ask(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    args: crate::assistant::AskArgs,
) -> Result<crate::assistant::AskOutcome, AppError> {
    validate_question(&args.question)?;
    let mut outcome = crate::assistant::ask(&app, &state.db, args).await?;
    // [B26.4] Паритет live/history: усечение фрагментов ПОСЛЕ persist.
    truncate_answer_for_wire(&mut outcome.message);
    Ok(outcome)
}

/// [B26.4] Полный текст фрагмента «Контекста поиска» (ленивая подгрузка;
/// на wire фрагменты усечены). Индекс стабилен: answer_json заморожен на
/// момент persist, переиндексации его не трогают.
#[tauri::command]
pub async fn assistant_get_fragment_text(
    state: State<'_, AppState>,
    message_id: String,
    fragment_index: usize,
) -> Result<String, AppError> {
    let ans = crate::db::assistant_embeddings::get_message_answer(&state.db, &message_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("assistant message {message_id}")))?;
    ans.fragments
        .get(fragment_index)
        .map(|f| f.text.clone())
        .ok_or_else(|| AppError::NotFound(format!("fragment {fragment_index}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_question_boundaries() {
        assert!(validate_question("нормальный вопрос").is_ok());
        assert!(validate_question("  \n\t ").is_err());
        assert!(validate_question("").is_err());
        let exact = "д".repeat(QUESTION_MAX_CHARS);
        assert!(validate_question(&exact).is_ok());
        let over = "д".repeat(QUESTION_MAX_CHARS + 1);
        assert!(validate_question(&over).is_err());
        // Байтовый гейт: ASCII-простыня длиннее 4×cap режется до счёта символов.
        let ascii_wall = "a".repeat(QUESTION_MAX_CHARS * 4 + 1);
        assert!(validate_question(&ascii_wall).is_err());
    }

    // [B26.4] Усечение превью фрагмента.
    #[test]
    fn truncate_fragment_boundaries() {
        // Короткий и «в гистерезисе» — не трогаем.
        assert!(truncate_fragment_text("короткий текст").is_none());
        let borderline = "д".repeat(FRAGMENT_PREVIEW_CHARS + FRAGMENT_PREVIEW_HYSTERESIS);
        assert!(truncate_fragment_text(&borderline).is_none());

        // Длинный — режется по границе слова с «…», короче лимита.
        let long = "слово ".repeat(100); // 600 симв
        let cut = truncate_fragment_text(&long).unwrap();
        assert!(cut.ends_with('…'));
        assert!(cut.chars().count() <= FRAGMENT_PREVIEW_CHARS + 1);
        // Перед «…» — ЦЕЛОЕ слово (режем по пробелу, не посреди слова).
        assert!(
            cut.trim_end_matches('…').ends_with("слово"),
            "режем по пробелу: {cut}"
        );

        // Кириллица без пробелов (гига-слово) — режется посимвольно, не паникует.
        let wall = "ы".repeat(500);
        let cut = truncate_fragment_text(&wall).unwrap();
        assert_eq!(cut.chars().count(), FRAGMENT_PREVIEW_CHARS + 1);
    }
}
