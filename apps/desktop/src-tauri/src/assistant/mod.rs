//! [M15] Ассистент — локальный RAG-чат по звонкам.
//!
//! PRD: docs/M15_ASSISTANT_PRD.md. Конвейер `ask` (§4.2):
//! классификатор → retrieval → budget → LLM (json_schema) → детерминированная
//! привязка источников → persist. Модули по фазам:
//! - M15.1 `types` · M15.3 `indexer` · M15.4 `classifier`
//! - M15.5 `retrieval` · M15.6 `budget` · M15.7 `answer` + `ask` (здесь)
//! - Ph2: `embedder`

pub mod answer;
pub mod budget;
pub mod classifier;
pub mod embed_cache;
pub mod embedder;
#[cfg(test)]
mod eval;
pub mod fusion;
pub mod indexer;
pub mod retrieval;
pub mod types;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::assistant::types::{
    AssistantAnswer, AssistantAnswerKind, AssistantFragment, AssistantMessage,
    AssistantPassageKind, AssistantRole, AssistantSource,
};
use crate::db::assistant as repo;
use crate::events::{AssistantStatusEvent, EventBus};
use crate::providers::llm::LlmProvider;
use crate::AppError;

/// Окно локальной модели (llm.rs DEFAULT_CTX_SIZE) — в контракт ответа.
const WINDOW_TOKENS: u32 = 8_192;

/// Тексты SPEC §1/§4 (хендофф) — деловой регистр, дословно.
const REFUSAL_TEXT: &str = "Составление текстов — вне области ассистента. Область: поиск и \
                            разбор информации в записанных звонках. Могу собрать факты — \
                            решения, задачи, сроки.";
const EMPTY_GLOBAL_TEXT: &str =
    "По звонкам ничего не найдено. Уточните имя участника, тему или период.";
const EMPTY_CALL_TEXT: &str = "В этом звонке этого не нашлось.";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskArgs {
    /// Продолжить существующий чат (глобальный или тред звонка).
    pub chat_id: Option<String>,
    /// Тред звонка (создаётся при первом вопросе). Игнорируется если chat_id задан.
    pub call_id: Option<String>,
    pub question: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskOutcome {
    pub chat_id: String,
    pub message: AssistantMessage,
}

/// Ядро `ask` — провайдер инжектится (тесты: MockProvider; прод: `ask`).
/// EventBus с `None`-handle — no-op, тесты не требуют AppHandle.
/// Без эмбеддера (retrieval = чистый BM25) — вход тестов Ph1 и live-gate;
/// прод идёт через `ask_core_with` (отсюда allow: в non-test сборке
/// вызовов нет).
#[allow(dead_code)]
pub async fn ask_core(
    provider: &dyn LlmProvider,
    pool: &SqlitePool,
    bus: &EventBus<'_>,
    args: AskArgs,
) -> Result<AskOutcome, AppError> {
    ask_core_with(provider, pool, bus, args, None).await
}

/// [M15.11] Полное ядро: + опциональный эмбеддер для гибридного retrieval
/// (None → BM25, PRD §6.3 graceful degradation).
pub async fn ask_core_with(
    provider: &dyn LlmProvider,
    pool: &SqlitePool,
    bus: &EventBus<'_>,
    args: AskArgs,
    embedder: Option<std::sync::Arc<dyn embedder::TextEmbedder>>,
) -> Result<AskOutcome, AppError> {
    let question = args.question.trim().to_string();
    if question.is_empty() {
        return Err(AppError::Other("assistant: empty question".into()));
    }

    // Чат: источник истины по scope — chat.call_id из БД, не аргументы.
    let chat = match (&args.chat_id, &args.call_id) {
        (Some(chat_id), _) => match repo::get_chat_meta(pool, chat_id).await? {
            Some(meta) => meta,
            None => return Err(AppError::NotFound(format!("assistant chat {chat_id}"))),
        },
        (None, Some(call_id)) => repo::get_or_create_call_chat(pool, call_id, &question).await?,
        (None, None) => repo::create_global_chat(pool, &question).await?,
    };
    let scope_call_id = chat.call_id.clone();

    // История ДО append текущего вопроса (последние пары берёт answer::build_input).
    let history = repo::get_chat_messages(pool, &chat.id).await?;
    repo::append_message(pool, &chat.id, AssistantRole::User, &question, None).await?;

    // 1. Классификация: генеративный запрос → отказ БЕЗ retrieval (SPEC §1).
    if classifier::is_generative(&question) {
        let ans = AssistantAnswer {
            kind: AssistantAnswerKind::Refusal,
            text: REFUSAL_TEXT.to_string(),
            sources: vec![],
            fragments: vec![],
            fragment_tokens: 0,
            window_tokens: WINDOW_TOKENS,
            escalate: None,
        };
        return finish(pool, chat.id, ans).await;
    }

    // 2. Retrieval + budget.
    bus.assistant_status(&AssistantStatusEvent {
        chat_id: chat.id.clone(),
        phase: "retrieving",
    });
    let scope = match scope_call_id.as_deref() {
        Some(id) => retrieval::Scope::Call(id),
        None => retrieval::Scope::Global,
    };
    let hits =
        retrieval::search_hybrid(pool, &question, scope, embedder, embed_cache::global()).await?;
    let hits_total = hits.len();
    let ctx = budget::assemble(hits, scope);

    // [M16.1] Диагностика отбора — ТОЛЬКО id/метрики, без текста вопросов и
    // фрагментов (приватность контента, W5). Включается RUST_LOG=debug.
    if log::log_enabled!(log::Level::Debug) {
        let taken: Vec<String> = ctx
            .fragments
            .iter()
            .map(|f| format!("{}:{}:{}@{:.4}", f.id, f.kind, &f.call_id[..8.min(f.call_id.len())], f.rank))
            .collect();
        log::debug!(
            "assistant retrieval: expr={:?} hits={hits_total} taken={} [{}] skipped dedup={} cap={} budget={} tokens={}",
            retrieval::build_match_expr(&question),
            ctx.fragments.len(),
            taken.join(", "),
            ctx.skipped_dedup,
            ctx.skipped_cap,
            ctx.skipped_budget,
            ctx.token_total,
        );
    }

    // 3. Пусто → честное «не найдено» (+эскалация в call-scope).
    if ctx.fragments.is_empty() {
        let is_call = scope_call_id.is_some();
        let ans = AssistantAnswer {
            kind: AssistantAnswerKind::Empty,
            text: if is_call {
                EMPTY_CALL_TEXT
            } else {
                EMPTY_GLOBAL_TEXT
            }
            .to_string(),
            sources: vec![],
            fragments: vec![],
            fragment_tokens: 0,
            window_tokens: WINDOW_TOKENS,
            escalate: is_call.then_some(true),
        };
        return finish(pool, chat.id, ans).await;
    }

    // 4. Титулы + даты звонков для промпта и источников ([M16.2] дата в
    // заголовке фрагмента — опора для «когда»-вопросов).
    let (titles, dates) = call_meta(pool, &ctx.fragments).await?;

    // 5. LLM.
    bus.assistant_status(&AssistantStatusEvent {
        chat_id: chat.id.clone(),
        phase: "generating",
    });
    let (text, used) =
        answer::generate_answer(provider, &ctx.fragments, &titles, &dates, &history, &question)
            .await?;

    // 6. Сборка ответа: sources из used-индексов (детерминированно), fragments —
    // весь контекст (блок «Контекст поиска» в UI).
    let sources = build_sources(&ctx.fragments, &titles, &used);
    let fragments = ctx
        .fragments
        .iter()
        .map(|f| AssistantFragment {
            call_id: f.call_id.clone(),
            call_title: titles
                .get(&f.call_id)
                .cloned()
                .unwrap_or_else(|| f.call_id.clone()),
            kind: AssistantPassageKind::parse(&f.kind).unwrap_or(AssistantPassageKind::Transcript),
            speaker: f.speaker.clone(),
            start_ms: f.start_ms,
            text: f.text.clone(),
        })
        .collect();
    let ans = AssistantAnswer {
        kind: AssistantAnswerKind::Answer,
        text,
        sources,
        fragments,
        fragment_tokens: ctx.token_total.max(0) as u32,
        window_tokens: WINDOW_TOKENS,
        escalate: None,
    };
    finish(pool, chat.id, ans).await
}

/// Persist assistant-ответа + результат — общий хвост всех веток ask_core.
async fn finish(
    pool: &SqlitePool,
    chat_id: String,
    ans: AssistantAnswer,
) -> Result<AskOutcome, AppError> {
    let message = repo::append_message(
        pool,
        &chat_id,
        AssistantRole::Assistant,
        &ans.text,
        Some(&ans),
    )
    .await?;
    Ok(AskOutcome { chat_id, message })
}

/// Продакшн-обёртка: собирает локальный провайдер (resident-aware) и зовёт ядро.
#[cfg(target_os = "macos")]
pub async fn ask(
    app: &tauri::AppHandle,
    pool: &SqlitePool,
    args: AskArgs,
) -> Result<AskOutcome, AppError> {
    use crate::pipeline::PipelineSettings;

    let s = PipelineSettings::load(pool).await?;
    let app_data_dir = {
        let state = tauri::Manager::state::<crate::state::AppState>(app);
        state.app_data_dir.clone()
    };
    let (provider, _preset) =
        crate::pipeline::build_local_llm_provider(pool, &app_data_dir, app, &s).await?;
    // Метка очереди — из chat.call_id (истина в БД): follow-up в треде звонка
    // приходит только с chat_id, args.call_id тогда пуст.
    let queue_label = match (&args.chat_id, &args.call_id) {
        (Some(chat_id), _) => repo::get_chat_meta(pool, chat_id)
            .await?
            .and_then(|c| c.call_id),
        (None, Some(call_id)) => Some(call_id.clone()),
        (None, None) => None,
    }
    .unwrap_or_else(|| "assistant".to_string());
    // cache_prompt: стабильный префикс [system][fragments] переживает
    // follow-up-ходы на resident-сервере (PRD §6.4).
    let provider = provider.with_call(queue_label).with_cache_prompt(true);
    let bus = EventBus::new(Some(app));
    // [M15.11] Гибридный retrieval, если эмбеддер доступен (feature + модель).
    let text_embedder = embedder::shared(&app_data_dir).await;
    ask_core_with(&provider, pool, &bus, args, text_embedder).await
}

/// [M16.2] «2026-07-01T09:29:36…» → «01.07.2026» для заголовка фрагмента.
fn fmt_call_date(started_at: &str) -> Option<String> {
    let d = started_at.get(..10)?;
    let mut it = d.split('-');
    let (y, m, day) = (it.next()?, it.next()?, it.next()?);
    if y.len() != 4 || m.len() != 2 || day.len() != 2 {
        return None;
    }
    Some(format!("{day}.{m}.{y}"))
}

/// Титулы + даты звонков одним батч-SELECT (id IN (...)); fallback титула —
/// call_id, отсутствие даты — просто нет записи в dates-мапе.
async fn call_meta(
    pool: &SqlitePool,
    fragments: &[crate::db::assistant::PassageHit],
) -> Result<(HashMap<String, String>, HashMap<String, String>), AppError> {
    let mut ids: Vec<&str> = fragments.iter().map(|f| f.call_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    // Fallback на call_id для всех; найденные титулы перезапишут.
    let mut titles: HashMap<String, String> = ids
        .iter()
        .map(|id| (id.to_string(), id.to_string()))
        .collect();
    let mut dates: HashMap<String, String> = HashMap::new();
    if ids.is_empty() {
        return Ok((titles, dates));
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT id, title, started_at FROM calls WHERE id IN ({placeholders})");
    let mut query = sqlx::query_as::<_, (String, Option<String>, String)>(&sql);
    for id in &ids {
        query = query.bind(*id);
    }
    for (id, title, started_at) in query.fetch_all(pool).await? {
        if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
            titles.insert(id.clone(), t);
        }
        if let Some(d) = fmt_call_date(&started_at) {
            dates.insert(id, d);
        }
    }
    Ok((titles, dates))
}

/// Источники из used-индексов: порядок модели, дедуп по (call_id, start_ms).
/// Дедуп сознательно схлопывает и разные structured-пассажи одного звонка без
/// таймкода: их чипы в UI визуально идентичны («Название» без т/к) — дубль
/// был бы шумом. Сами пассажи остаются в fragments («Контекст поиска»).
fn build_sources(
    fragments: &[crate::db::assistant::PassageHit],
    titles: &HashMap<String, String>,
    used: &[usize],
) -> Vec<AssistantSource> {
    let mut out: Vec<AssistantSource> = Vec::new();
    for &i in used {
        let Some(f) = fragments.get(i) else { continue };
        if out
            .iter()
            .any(|s| s.call_id == f.call_id && s.start_ms == f.start_ms)
        {
            continue;
        }
        out.push(AssistantSource {
            call_id: f.call_id.clone(),
            call_title: titles
                .get(&f.call_id)
                .cloned()
                .unwrap_or_else(|| f.call_id.clone()),
            start_ms: f.start_ms,
        });
    }
    out
}

#[cfg(test)]
mod tests {
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
            mock.last_input().contains("Прямого ответа во фрагментах может не быть"),
            "retry несёт nudge-хвост"
        );
        assert!(
            !ans.sources.is_empty(),
            "NO_DIRECT теперь с fallback-источниками"
        );
        assert!(!ans.fragments.is_empty(), "контекст поиска сохраняется");
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
    /// WOTOLD_LIVE_DB_DIR=<dir-с-копией-app.db> WOTOLD_LIVE_LLM_URL=http://127.0.0.1:47331 \
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
        .with_server(Some(url))
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
}
