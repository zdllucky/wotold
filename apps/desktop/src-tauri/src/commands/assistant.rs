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
    repo::get_chat_messages(&state.db, &chat_id).await
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
    let messages = repo::get_chat_messages(&state.db, &chat.id).await?;
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
    crate::assistant::ask(&app, &state.db, args).await
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
}
