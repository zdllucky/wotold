//! [TD-41] Тесты ядра ассистента (`ask_core`).
//!
//! Вынесены из `assistant/mod.rs`: код там 469 строк, тесты — 530, вместе
//! 1001 при лимите 800 (правило 8). Сценарные тесты одного и того же
//! `ask_core` доменной границы между собой не имеют.

use super::*;
use crate::db::assistant::{replace_call_passages, PassageInput};
use crate::db::test_support::fresh_db;
use crate::providers::llm::{LlmError, LlmRequest};
use async_trait::async_trait;
use std::sync::Mutex;

struct MockProvider {
    responses: Mutex<Vec<Result<serde_json::Value, LlmError>>>,
    captured: Mutex<Vec<LlmRequest>>,
}

impl MockProvider {
    fn scripted(responses: Vec<Result<serde_json::Value, LlmError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            captured: Mutex::new(Vec::new()),
        }
    }
    fn call_count(&self) -> usize {
        self.captured.lock().unwrap().len()
    }
    fn last_input(&self) -> String {
        self.captured.lock().unwrap().last().unwrap().input.clone()
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    async fn generate(&self, request: LlmRequest) -> Result<serde_json::Value, LlmError> {
        self.captured.lock().unwrap().push(request);
        let mut guard = self.responses.lock().unwrap();
        if guard.is_empty() {
            return Err(LlmError::Provider("no scripted response".into()));
        }
        guard.remove(0)
    }
}

async fn seed_call_with_passages(pool: &SqlitePool, id: &str, title: &str, texts: &[&str]) {
    sqlx::query(
        "INSERT INTO calls (id, title, started_at, status, path_label, created_at, updated_at)
         VALUES (?1, ?2, CURRENT_TIMESTAMP, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(id)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
    let inputs: Vec<PassageInput> = texts
        .iter()
        .enumerate()
        .map(|(i, t)| PassageInput {
            kind: AssistantPassageKind::Transcript,
            speaker: Some("owner".into()),
            start_ms: Some((i as i64) * 60_000),
            end_ms: Some((i as i64) * 60_000 + 30_000),
            text: t.to_string(),
            token_est: (t.len() / 4) as i64,
        })
        .collect();
    replace_call_passages(pool, id, &inputs).await.unwrap();
}

fn args(question: &str) -> AskArgs {
    AskArgs {
        chat_id: None,
        call_id: None,
        question: question.into(),
    }
}

#[tokio::test]
async fn refusal_short_circuits_without_llm() {
    let db = fresh_db().await;
    let mock = MockProvider::scripted(vec![]);
    let bus = EventBus::new(None);
    let out = ask_core(&mock, &db.pool, &bus, args("Напиши письмо Арману"))
        .await
        .unwrap();
    assert_eq!(mock.call_count(), 0, "LLM не должен вызываться");
    let ans = out.message.answer.as_ref().unwrap();
    assert_eq!(ans.kind, AssistantAnswerKind::Refusal);
    assert!(ans.fragments.is_empty());
    // Persist: user + assistant.
    let msgs = repo::get_chat_messages(&db.pool, &out.chat_id)
        .await
        .unwrap();
    assert_eq!(msgs.len(), 2);
}

#[tokio::test]
async fn empty_global_and_call_scope_with_escalation() {
    let db = fresh_db().await;
    seed_call_with_passages(&db.pool, "c1", "Синхрон", &["обсуждали бюджет"]).await;
    let mock = MockProvider::scripted(vec![]);
    let bus = EventBus::new(None);

    let g = ask_core(&mock, &db.pool, &bus, args("ксенофобия марсоходов"))
        .await
        .unwrap();
    let ga = g.message.answer.as_ref().unwrap();
    assert_eq!(ga.kind, AssistantAnswerKind::Empty);
    assert_eq!(ga.text, EMPTY_GLOBAL_TEXT);
    assert!(ga.escalate.is_none());

    let c = ask_core(
        &mock,
        &db.pool,
        &bus,
        AskArgs {
            chat_id: None,
            call_id: Some("c1".into()),
            question: "ксенофобия марсоходов".into(),
        },
    )
    .await
    .unwrap();
    let ca = c.message.answer.as_ref().unwrap();
    assert_eq!(ca.kind, AssistantAnswerKind::Empty);
    assert_eq!(ca.text, EMPTY_CALL_TEXT);
    assert_eq!(ca.escalate, Some(true));
    assert_eq!(mock.call_count(), 0);
}

#[tokio::test]
async fn happy_path_binds_sources_from_used_fragments() {
    let db = fresh_db().await;
    seed_call_with_passages(
        &db.pool,
        "c1",
        "Синхрон по пилоту",
        &["обсуждали бюджет пилота", "бюджет утвердили в среду"],
    )
    .await;
    let mock = MockProvider::scripted(vec![Ok(serde_json::json!({
        "answer": "Бюджет утвердили в среду.",
        "used_fragments": [2, 1, 99]
    }))]);
    let bus = EventBus::new(None);
    let out = ask_core(&mock, &db.pool, &bus, args("что с бюджетом?"))
        .await
        .unwrap();

    let ans = out.message.answer.as_ref().unwrap();
    assert_eq!(ans.kind, AssistantAnswerKind::Answer);
    assert_eq!(ans.text, "Бюджет утвердили в среду.");
    // Порядок модели: [2] раньше [1]; 99 отброшен клэмпом.
    assert_eq!(ans.sources.len(), 2);
    assert_eq!(ans.sources[0].start_ms, Some(60_000));
    assert_eq!(ans.sources[1].start_ms, Some(0));
    assert_eq!(ans.sources[0].call_title, "Синхрон по пилоту");
    assert!(!ans.fragments.is_empty());
    assert!(ans.fragment_tokens > 0);
    // Round-trip через persist.
    let msgs = repo::get_chat_messages(&db.pool, &out.chat_id)
        .await
        .unwrap();
    assert_eq!(msgs[1].answer.as_ref().unwrap(), ans);
    // Промпт содержит нумерованные фрагменты и вопрос.
    let input = mock.last_input();
    assert!(input.contains("[1] «Синхрон по пилоту»"));
    assert!(input.contains("ВОПРОС: что с бюджетом?"));
}

#[tokio::test]
async fn llm_error_keeps_user_message_only() {
    let db = fresh_db().await;
    seed_call_with_passages(&db.pool, "c1", "Синхрон", &["обсуждали бюджет"]).await;
    let mock = MockProvider::scripted(vec![Err(LlmError::Provider("boom".into()))]);
    let bus = EventBus::new(None);
    let err = ask_core(&mock, &db.pool, &bus, args("что с бюджетом?")).await;
    assert!(err.is_err());
    // User-сообщение сохранено, assistant — нет.
    let chats = repo::list_global_chats(&db.pool).await.unwrap();
    assert_eq!(chats.len(), 1);
    let msgs = repo::get_chat_messages(&db.pool, &chats[0].id)
        .await
        .unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].role, AssistantRole::User);
}

#[tokio::test]
async fn follow_up_in_existing_chat_carries_history() {
    let db = fresh_db().await;
    seed_call_with_passages(&db.pool, "c1", "Синхрон", &["обсуждали бюджет"]).await;
    let mock = MockProvider::scripted(vec![
        Ok(serde_json::json!({"answer": "Первый.", "used_fragments": [1]})),
        Ok(serde_json::json!({"answer": "Второй.", "used_fragments": [1]})),
    ]);
    let bus = EventBus::new(None);
    let first = ask_core(&mock, &db.pool, &bus, args("что с бюджетом?"))
        .await
        .unwrap();
    let second = ask_core(
        &mock,
        &db.pool,
        &bus,
        AskArgs {
            chat_id: Some(first.chat_id.clone()),
            call_id: None,
            question: "а бюджет когда?".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(second.chat_id, first.chat_id);
    // История первого обмена попала в промпт второго вызова.
    let input = mock.last_input();
    assert!(input.contains("Предыдущий диалог:"));
    assert!(input.contains("Пользователь: что с бюджетом?"));
    assert!(input.contains("Ассистент: Первый."));
    let msgs = repo::get_chat_messages(&db.pool, &first.chat_id)
        .await
        .unwrap();
    assert_eq!(msgs.len(), 4);
}

#[tokio::test]
async fn model_empty_answer_becomes_honest_no_direct_answer() {
    // [M16.2] Пустой answer → один retry с nudge; дважды пусто → честный
    // NO_DIRECT, теперь С fallback-источниками top-K (след для ручного
    // поиска вместо тупика).
    let db = fresh_db().await;
    seed_call_with_passages(&db.pool, "c1", "Синхрон", &["обсуждали бюджет"]).await;
    let mock = MockProvider::scripted(vec![
        Ok(serde_json::json!({"answer": "  ", "used_fragments": []})),
        Ok(serde_json::json!({"answer": "", "used_fragments": []})),
    ]);
    let bus = EventBus::new(None);
    let out = ask_core(&mock, &db.pool, &bus, args("что с бюджетом?"))
        .await
        .unwrap();
    let ans = out.message.answer.as_ref().unwrap();
    assert_eq!(ans.kind, AssistantAnswerKind::Answer);
    assert_eq!(ans.text, crate::assistant::answer::NO_DIRECT_ANSWER_TEXT);
    assert_eq!(mock.call_count(), 2, "ровно один retry");
    assert!(
        mock.last_input()
            .contains("Прямого ответа во фрагментах может не быть"),
        "retry несёт nudge-хвост"
    );
    assert!(
        !ans.sources.is_empty(),
        "NO_DIRECT теперь с fallback-источниками"
    );
    assert!(!ans.fragments.is_empty(), "контекст поиска сохраняется");
}

// [B26.2] Период без звонков → честный Empty без retrieval и LLM.
#[tokio::test]
async fn empty_period_answers_honestly_without_llm() {
    let db = fresh_db().await;
    seed_call_with_passages(&db.pool, "c1", "Синхрон", &["обсуждали бюджет"]).await;
    let mock = MockProvider::scripted(vec![]);
    let bus = EventBus::new(None);
    let out = ask_core(
        &mock,
        &db.pool,
        &bus,
        args("что обсуждали в прошлом году про бюджет"),
    )
    .await
    .unwrap();
    let ans = out.message.answer.as_ref().unwrap();
    assert_eq!(ans.kind, AssistantAnswerKind::Empty);
    assert_eq!(ans.text, EMPTY_PERIOD_TEXT);
    assert_eq!(mock.call_count(), 0);
}

// [B26.5b] Имя контакта в обычном вопросе → карточка контакта
// инжектится фрагментом-источником в контекст LLM.
#[tokio::test]
async fn contact_name_in_question_injects_card_fragment() {
    let db = fresh_db().await;
    seed_call_with_passages(&db.pool, "c1", "Синхрон", &["обсуждали бюджет проекта"]).await;
    sqlx::query(
        "INSERT INTO contacts (id, display_name, org, created_at, updated_at)
         VALUES ('ct1', 'Иван Петров', 'Acme', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let mock = MockProvider::scripted(vec![Ok(serde_json::json!({
        "answer": "Иван — контакт Acme, обсуждали бюджет.",
        "used_fragments": [1]
    }))]);
    let bus = EventBus::new(None);
    let out = ask_core(&mock, &db.pool, &bus, args("что Иван говорил про бюджет"))
        .await
        .unwrap();
    let ans = out.message.answer.as_ref().unwrap();
    assert!(
        mock.last_input().contains("Иван Петров — контакт, Acme"),
        "карточка контакта в промпте: {}",
        mock.last_input()
    );
    assert!(
        ans.fragments
            .iter()
            .any(|f| f.kind == AssistantPassageKind::Contact
                && f.call_id.starts_with("contact:")
                && f.call_title == "Иван Петров"),
        "контакт-фрагмент с sentinel и титулом-именем"
    );
}

// [M16.4] Мета-вопрос → детерминированный ответ роутера, LLM не зовётся.
#[tokio::test]
async fn meta_stats_question_answers_without_llm() {
    let db = fresh_db().await;
    seed_call_with_passages(&db.pool, "c1", "Синхрон", &["обсуждали бюджет"]).await;
    let mock = MockProvider::scripted(vec![]); // любой LLM-вызов упал бы
    let bus = EventBus::new(None);
    let out = ask_core(&mock, &db.pool, &bus, args("сколько звонков записано"))
        .await
        .unwrap();
    let ans = out.message.answer.as_ref().unwrap();
    assert!(ans.text.contains("Записано 1 звонков"), "{}", ans.text);
    assert_eq!(mock.call_count(), 0, "stats-интент не ходит в LLM");
}

// [M16.5] «о чём звонок» в call-scope → рекап-пассажи напрямую, мимо FTS
// (вопрос лексически НЕ пересекается с контентом — раньше был empty).
#[tokio::test]
async fn call_scope_summary_uses_recap_without_fts_match() {
    let db = fresh_db().await;
    sqlx::query(
        "INSERT INTO calls (id, title, started_at, status, path_label, created_at, updated_at)
         VALUES ('c1', 'Планёрка', CURRENT_TIMESTAMP, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    replace_call_passages(
        &db.pool,
        "c1",
        &[
            PassageInput {
                kind: AssistantPassageKind::Recap,
                speaker: None,
                start_ms: None,
                end_ms: None,
                text: "Договорились о пилоте и распределили задачи.".into(),
                token_est: 12,
            },
            PassageInput {
                kind: AssistantPassageKind::Transcript,
                speaker: Some("owner".into()),
                start_ms: Some(0),
                end_ms: Some(30_000),
                text: "длинная беседа про детали".into(),
                token_est: 8,
            },
        ],
    )
    .await
    .unwrap();

    let mock = MockProvider::scripted(vec![Ok(serde_json::json!({
        "answer": "Планёрка: пилот и задачи.",
        "used_fragments": [1]
    }))]);
    let bus = EventBus::new(None);
    let out = ask_core(
        &mock,
        &db.pool,
        &bus,
        AskArgs {
            chat_id: None,
            call_id: Some("c1".into()),
            question: "о чем звонок".into(),
        },
    )
    .await
    .unwrap();
    let ans = out.message.answer.as_ref().unwrap();
    assert_eq!(ans.text, "Планёрка: пилот и задачи.");
    let input = mock.last_input();
    assert!(
        input.contains("Договорились о пилоте"),
        "рекап обязан попасть в промпт: {input}"
    );
    assert!(
        input.contains("[1]"),
        "рекап первым (порядок kind-приоритета)"
    );
}

#[tokio::test]
async fn empty_then_answer_retry_succeeds() {
    // [M16.2] Первый проход пуст, retry дал ответ — юзер видит ответ,
    // не NO_DIRECT.
    let db = fresh_db().await;
    seed_call_with_passages(&db.pool, "c1", "Синхрон", &["обсуждали бюджет проекта"]).await;
    let mock = MockProvider::scripted(vec![
        Ok(serde_json::json!({"answer": "", "used_fragments": []})),
        Ok(serde_json::json!({"answer": "Бюджет обсуждали в звонке.", "used_fragments": [1]})),
    ]);
    let bus = EventBus::new(None);
    let out = ask_core(&mock, &db.pool, &bus, args("что с бюджетом?"))
        .await
        .unwrap();
    let ans = out.message.answer.as_ref().unwrap();
    assert_eq!(ans.text, "Бюджет обсуждали в звонке.");
    assert_eq!(mock.call_count(), 2);
    assert_eq!(ans.sources.len(), 1);
}

#[tokio::test]
async fn unknown_chat_id_is_not_found() {
    let db = fresh_db().await;
    let mock = MockProvider::scripted(vec![]);
    let bus = EventBus::new(None);
    let err = ask_core(
        &mock,
        &db.pool,
        &bus,
        AskArgs {
            chat_id: Some("nope".into()),
            call_id: None,
            question: "вопрос".into(),
        },
    )
    .await;
    assert!(matches!(err, Err(AppError::NotFound(_))));
}

/// [Gate Ph1 / M15.12] Живой e2e против КОПИИ реальной БД и вручную
/// поднятого llama-server (HTTP-путь провайдера не требует AppHandle).
/// Запуск только явно:
/// ```sh
/// WOTOLD_LIVE_DB_DIR=<dir-с-копией-app.db> WOTOLD_LIVE_LLM_URL=http://127.0.0.1:<порт из лога старта> \
///   cargo test --lib live_gate_ph1 -- --ignored --nocapture
/// ```
#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "live gate: требует WOTOLD_LIVE_DB_DIR + WOTOLD_LIVE_LLM_URL + запущенный llama-server"]
async fn live_gate_ph1() {
    let Ok(db_dir) = std::env::var("WOTOLD_LIVE_DB_DIR") else {
        panic!("set WOTOLD_LIVE_DB_DIR");
    };
    let Ok(url) = std::env::var("WOTOLD_LIVE_LLM_URL") else {
        panic!("set WOTOLD_LIVE_LLM_URL");
    };
    let pool = crate::db::init(std::path::Path::new(&db_dir))
        .await
        .expect("init db copy");
    let preset = crate::local_engine::preset::LocalEnginePreset::Balanced;
    let provider = crate::local_engine::llm::LocalLlamaProvider::for_preset(
        std::path::Path::new(&db_dir),
        preset.llm_model_id(),
    )
    .with_server(Some(crate::local_engine::llm::ServerHandle {
        url,
        // [TD-08] Ключ вручную поднятого сервера. Пусто — сервер без
        // LLAMA_API_KEY, авторизации не требует.
        api_key: std::env::var("WOTOLD_LIVE_LLM_KEY").unwrap_or_default(),
    }))
    .with_cache_prompt(true);
    let bus = EventBus::new(None);

    // 1. Refusal — мгновенно, без LLM.
    let t0 = std::time::Instant::now();
    let refusal = ask_core(&provider, &pool, &bus, args("Напиши письмо по итогам"))
        .await
        .unwrap();
    println!(
        "REFUSAL [{}ms]: {}",
        t0.elapsed().as_millis(),
        refusal.message.text
    );
    assert_eq!(
        refusal.message.answer.as_ref().unwrap().kind,
        AssistantAnswerKind::Refusal
    );

    // 2. Empty — честное «не найдено».
    let t0 = std::time::Instant::now();
    let empty = ask_core(
        &provider,
        &pool,
        &bus,
        args("квантовая телепортация хомяков"),
    )
    .await
    .unwrap();
    println!(
        "EMPTY [{}ms]: {}",
        t0.elapsed().as_millis(),
        empty.message.text
    );
    assert_eq!(
        empty.message.answer.as_ref().unwrap().kind,
        AssistantAnswerKind::Empty
    );

    // 3. Живой вопрос по реальным данным.
    let question =
        std::env::var("WOTOLD_LIVE_QUESTION").unwrap_or_else(|_| "О чём договорились?".into());
    let t0 = std::time::Instant::now();
    let out = ask_core(&provider, &pool, &bus, args(&question))
        .await
        .expect("live ask");
    let elapsed = t0.elapsed();
    let ans = out.message.answer.as_ref().unwrap();
    println!("QUESTION: {question}");
    println!("ANSWER [{:.1}s]: {}", elapsed.as_secs_f32(), ans.text);
    println!(
        "SOURCES: {:?}",
        ans.sources
            .iter()
            .map(|s| format!("{} @{:?}", s.call_title, s.start_ms))
            .collect::<Vec<_>>()
    );
    println!(
        "FRAGMENTS: {} · ~{} tokens · window {}",
        ans.fragments.len(),
        ans.fragment_tokens,
        ans.window_tokens
    );
    assert_eq!(ans.kind, AssistantAnswerKind::Answer);
    // sources пусты только на honest «нет прямого ответа» — печатаем, не валим.
    if ans.sources.is_empty() {
        println!("NOTE: model returned no-direct-answer (sources empty)");
    }
}
