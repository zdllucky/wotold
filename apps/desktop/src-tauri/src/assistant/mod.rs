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
pub mod contacts_ctx;
pub mod direct;
pub mod embed_cache;
pub mod embedder;
#[cfg(test)]
mod eval;
pub mod fusion;
pub mod indexer;
pub mod lazy_provider;
pub mod passages;
pub mod period;
pub mod retrieval;
pub mod router;
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
// pub(crate): переиспользует router (WhenDiscussed без результатов).
pub(crate) const EMPTY_GLOBAL_TEXT: &str =
    "По звонкам ничего не найдено. Уточните имя участника, тему или период.";
const EMPTY_CALL_TEXT: &str = "В этом звонке этого не нашлось.";
// [B26.2] Явный период в вопросе, но звонков за него нет.
const EMPTY_PERIOD_TEXT: &str = "За этот период записанных звонков не нашлось.";

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

    // 1b. [M16.4] Интент-раутер: мета-вопросы (сколько/последний/какие
    // звонки, «когда обсуждали», call-summary) — детерминированно по
    // метаданным, без retrieval и почти всегда без LLM.
    match router::try_route(pool, &question, scope_call_id.as_deref(), embedder.clone()).await? {
        Some(router::RoutedAnswer::Direct { text, sources }) => {
            let ans = AssistantAnswer {
                kind: AssistantAnswerKind::Answer,
                text,
                sources,
                fragments: vec![],
                fragment_tokens: 0,
                window_tokens: WINDOW_TOKENS,
                escalate: None,
            };
            return finish(pool, chat.id, ans).await;
        }
        Some(router::RoutedAnswer::SummarizeCall { call_id }) => {
            // [M16.5] Рекап-путь: пассажи звонка по типам, мимо FTS —
            // «о чём звонок» не имеет лексического пересечения с контентом.
            let hits =
                crate::db::assistant_embeddings::list_call_passages_for_summary(pool, &call_id)
                    .await?;
            let ctx = budget::assemble(hits, retrieval::Scope::Call(&call_id));
            if !ctx.fragments.is_empty() {
                return llm_answer_path(
                    provider,
                    pool,
                    bus,
                    chat.id,
                    ctx,
                    &history,
                    &question,
                    HashMap::new(),
                )
                .await;
            }
            // Пассажей нет (звонок без артефактов) — обычный конвейер ниже.
        }
        None => {}
    }

    // 1c. [B26.2] Темпоральный префильтр: явный период («вчера», «в прошлом
    // месяце», «в июне») сужает архив до звонков за период. Пустой период →
    // честное «не нашлось» без похода в поиск и LLM.
    let period_filter = period::period_call_filter(pool, &question).await?;
    if let Some(set) = &period_filter {
        if set.is_empty() {
            let ans = AssistantAnswer {
                kind: AssistantAnswerKind::Empty,
                text: EMPTY_PERIOD_TEXT.to_string(),
                sources: vec![],
                fragments: vec![],
                fragment_tokens: 0,
                window_tokens: WINDOW_TOKENS,
                escalate: None,
            };
            return finish(pool, chat.id, ans).await;
        }
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
    let hits = retrieval::search_hybrid(
        pool,
        &question,
        scope,
        embedder,
        embed_cache::global(),
        period_filter.as_ref(),
    )
    .await?;
    let hits_total = hits.len();
    let mut ctx = budget::assemble(hits, scope);

    // [B26.5b] Инжект контактов: имя контакта в вопросе → карточка контакта
    // фрагментом-источником (kind=contact, sentinel call_id). Идёт ДО
    // empty-проверки: вопрос «чем занимается X» без звонков честно отвечается
    // карточкой.
    let (contact_hits, contact_titles) =
        contacts_ctx::contact_hits_for_question(pool, &question).await;
    budget::inject_priority_hits(&mut ctx, contact_hits);

    // [M16.1] Диагностика отбора — ТОЛЬКО id/метрики, без текста вопросов и
    // фрагментов (приватность контента, W5). Включается RUST_LOG=debug.
    if log::log_enabled!(log::Level::Debug) {
        let taken: Vec<String> = ctx
            .fragments
            .iter()
            .map(|f| {
                format!(
                    "{}:{}:{}@{:.4}",
                    f.id,
                    f.kind,
                    &f.call_id[..8.min(f.call_id.len())],
                    f.rank
                )
            })
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

    // 4-6. Общий LLM-хвост (титулы/даты → генерация → сборка → persist).
    llm_answer_path(
        provider,
        pool,
        bus,
        chat.id,
        ctx,
        &history,
        &question,
        contact_titles,
    )
    .await
}

/// [M16.5] Общий LLM-хвост `ask`: call_meta → generate_answer → сборка
/// AssistantAnswer → finish. Используется и обычным конвейером (после
/// retrieval+budget), и call-summary путём (рекап-пассажи мимо FTS).
#[allow(clippy::too_many_arguments)]
async fn llm_answer_path(
    provider: &dyn LlmProvider,
    pool: &SqlitePool,
    bus: &EventBus<'_>,
    chat_id: String,
    ctx: budget::BudgetedContext,
    history: &[AssistantMessage],
    question: &str,
    // [B26.5] sentinel-титулы контакт-фрагментов (call_meta их не резолвит).
    extra_titles: HashMap<String, String>,
) -> Result<AskOutcome, AppError> {
    // Титулы + даты звонков для промпта и источников ([M16.2] дата в
    // заголовке фрагмента — опора для «когда»-вопросов).
    let (mut titles, dates) = call_meta(pool, &ctx.fragments).await?;
    titles.extend(extra_titles);

    bus.assistant_status(&AssistantStatusEvent {
        chat_id: chat_id.clone(),
        phase: "generating",
    });
    let (text, used) =
        answer::generate_answer(provider, &ctx.fragments, &titles, &dates, history, question)
            .await?;

    // Сборка ответа: sources из used-индексов (детерминированно), fragments —
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
            text_truncated: false,
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
    finish(pool, chat_id, ans).await
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
    let app_data_dir = {
        let state = tauri::Manager::state::<crate::state::AppState>(app);
        state.app_data_dir.clone()
    };
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
    // [TD-23] Провайдер строится ЛЕНИВО, при первом обращении к модели.
    // Раньше он поднимался здесь, до роутера, и при невыбранном пресете
    // мета-вопросы, отказ и пустая ветка падали «модель не установлена» —
    // то есть весь смысл роутера M16.4 («нулевая латентность без LLM»).
    let provider = lazy_provider::LazyLocalProvider::new(
        pool.clone(),
        app.clone(),
        app_data_dir.clone(),
        queue_label,
    );
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
mod tests;
