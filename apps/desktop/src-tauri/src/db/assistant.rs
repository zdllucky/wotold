//! [M15.2] DB helpers ассистента: чаты, сообщения, пассажи индекса (0019).
//!
//! ВАЖНО: запись в `assistant_passages` — ТОЛЬКО через этот модуль
//! (`replace_call_passages` / `delete_call_passages`). FTS-таблица
//! `assistant_fts` синхронизируется триггерами миграции 0019 — прямые
//! INSERT/UPDATE мимо репозитория рискуют обойти инварианты token_est/kind.
//!
//! PRD: docs/M15_ASSISTANT_PRD.md §5.1, §7.

// [M15.2] Production callers (indexer M15.3, retrieval M15.5, commands M15.8)
// подключаются в следующих задачах.
#![allow(dead_code)]

use chrono::Utc;
use sqlx::SqlitePool;

use crate::assistant::types::{
    AssistantAnswer, AssistantChatMeta, AssistantIndexStats, AssistantMessage,
    AssistantPassageKind, AssistantRole,
};
use crate::AppError;

/// Максимум символов заголовка чата (SPEC §3: вопрос, усечённый до ~42).
const CHAT_TITLE_MAX_CHARS: usize = 42;

/// Заголовок чата из первого вопроса: trim + усечение по границе символа + «…».
pub fn chat_title_from_question(question: &str) -> String {
    let q = question.trim();
    if q.chars().count() <= CHAT_TITLE_MAX_CHARS {
        return q.to_string();
    }
    let cut: String = q.chars().take(CHAT_TITLE_MAX_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

// Конвенция кодовой базы (db/contacts.rs, pipeline/mod.rs): chrono to_rfc3339().
fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

// ── Чаты ──────────────────────────────────────────────────────────────

/// Создать глобальный чат (`call_id IS NULL`).
pub async fn create_global_chat(
    pool: &SqlitePool,
    question: &str,
) -> Result<AssistantChatMeta, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let title = chat_title_from_question(question);
    let now = now_iso();
    sqlx::query(
        "INSERT INTO assistant_chats (id, call_id, title, created_at, updated_at)
         VALUES (?1, NULL, ?2, ?3, ?3)",
    )
    .bind(&id)
    .bind(&title)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(AssistantChatMeta {
        id,
        call_id: None,
        title,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// Получить (или создать) единственный тред звонка. Идемпотентно:
/// повторный вызов возвращает существующий чат, title не перезаписывается.
pub async fn get_or_create_call_chat(
    pool: &SqlitePool,
    call_id: &str,
    question: &str,
) -> Result<AssistantChatMeta, AppError> {
    if let Some(existing) = get_call_chat(pool, call_id).await? {
        return Ok(existing);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let title = chat_title_from_question(question);
    let now = now_iso();
    // Гонка двух вызовов упирается в partial-UNIQUE(call_id) — проигравший
    // перечитывает существующую строку.
    let inserted = sqlx::query(
        "INSERT INTO assistant_chats (id, call_id, title, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT DO NOTHING",
    )
    .bind(&id)
    .bind(call_id)
    .bind(&title)
    .bind(&now)
    .execute(pool)
    .await?;
    if inserted.rows_affected() == 0 {
        // Конфликт: строка существовала на момент INSERT. Если перечитка её не
        // видит (конкурентный delete звонка/чата в окне между запросами) —
        // честная ошибка, а не фантомный chat_id, которого нет в БД.
        return get_call_chat(pool, call_id).await?.ok_or_else(|| {
            AppError::Other(format!(
                "assistant chat for call {call_id} vanished during creation"
            ))
        });
    }
    Ok(AssistantChatMeta {
        id,
        call_id: Some(call_id.to_string()),
        title,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// Чат по id (глобальный или тред звонка).
pub async fn get_chat_meta(
    pool: &SqlitePool,
    chat_id: &str,
) -> Result<Option<AssistantChatMeta>, AppError> {
    let row: Option<(String, Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT id, call_id, title, created_at, updated_at FROM assistant_chats WHERE id = ?1",
    )
    .bind(chat_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| AssistantChatMeta {
        id: r.0,
        call_id: r.1,
        title: r.2,
        created_at: r.3,
        updated_at: r.4,
    }))
}

/// Тред звонка, если существует.
pub async fn get_call_chat(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Option<AssistantChatMeta>, AppError> {
    let row: Option<(String, Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT id, call_id, title, created_at, updated_at FROM assistant_chats WHERE call_id = ?1",
    )
    .bind(call_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| AssistantChatMeta {
        id: r.0,
        call_id: r.1,
        title: r.2,
        created_at: r.3,
        updated_at: r.4,
    }))
}

/// Глобальные чаты раздела, свежие сверху (updated_at DESC).
pub async fn list_global_chats(pool: &SqlitePool) -> Result<Vec<AssistantChatMeta>, AppError> {
    let rows: Vec<(String, Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT id, call_id, title, created_at, updated_at
         FROM assistant_chats
         WHERE call_id IS NULL
         ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| AssistantChatMeta {
            id: r.0,
            call_id: r.1,
            title: r.2,
            created_at: r.3,
            updated_at: r.4,
        })
        .collect())
}

/// Удалить чат (messages каскадом).
pub async fn delete_chat(pool: &SqlitePool, chat_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM assistant_chats WHERE id = ?1")
        .bind(chat_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Сообщения ─────────────────────────────────────────────────────────

/// Добавить сообщение в чат: order_idx = max+1, updated_at чата бампается.
pub async fn append_message(
    pool: &SqlitePool,
    chat_id: &str,
    role: AssistantRole,
    text: &str,
    answer: Option<&AssistantAnswer>,
) -> Result<AssistantMessage, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_iso();
    let answer_json = answer.map(serde_json::to_string).transpose()?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO assistant_messages (id, chat_id, role, text, answer_json, order_idx, created_at)
         VALUES (
           ?1, ?2, ?3, ?4, ?5,
           (SELECT COALESCE(MAX(order_idx), -1) + 1 FROM assistant_messages WHERE chat_id = ?2),
           ?6
         )",
    )
    .bind(&id)
    .bind(chat_id)
    .bind(role.as_str())
    .bind(text)
    .bind(answer_json.as_deref())
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE assistant_chats SET updated_at = ?2 WHERE id = ?1")
        .bind(chat_id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(AssistantMessage {
        id,
        role,
        text: text.to_string(),
        answer: answer.cloned(),
        created_at: now,
    })
}

/// Сообщения чата по порядку.
pub async fn get_chat_messages(
    pool: &SqlitePool,
    chat_id: &str,
) -> Result<Vec<AssistantMessage>, AppError> {
    let rows: Vec<(String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, role, text, answer_json, created_at
         FROM assistant_messages
         WHERE chat_id = ?1
         ORDER BY order_idx ASC",
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let role = AssistantRole::parse(&r.1)
            .ok_or_else(|| AppError::Other(format!("unknown assistant role: {}", r.1)))?;
        let answer = match r.3 {
            Some(json) => Some(
                serde_json::from_str::<AssistantAnswer>(&json)
                    .map_err(|e| AppError::Other(format!("bad answer_json: {e}")))?,
            ),
            None => None,
        };
        out.push(AssistantMessage {
            id: r.0,
            role,
            text: r.2,
            answer,
            created_at: r.4,
        });
    }
    Ok(out)
}

// ── Пассажи индекса ───────────────────────────────────────────────────

/// Input пассажа при индексации (M15.3 передаёт из passage builder).
/// `kind` — enum: невалидное значение не соберётся, CHECK в SQL не сработает.
#[derive(Debug, Clone)]
pub struct PassageInput {
    pub kind: AssistantPassageKind,
    pub speaker: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub text: String,
    pub token_est: i64,
}

/// Заменить все пассажи звонка (DELETE + INSERT + upsert index_state, одна tx).
/// Возвращает (passage_count, token_total).
pub async fn replace_call_passages(
    pool: &SqlitePool,
    call_id: &str,
    passages: &[PassageInput],
) -> Result<(i64, i64), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM assistant_passages WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    let mut count = 0i64;
    let mut tokens = 0i64;
    for p in passages {
        let text = p.text.trim();
        if text.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO assistant_passages (call_id, kind, speaker, start_ms, end_ms, text, token_est)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(call_id)
        .bind(p.kind.as_str())
        .bind(p.speaker.as_deref())
        .bind(p.start_ms)
        .bind(p.end_ms)
        .bind(text)
        .bind(p.token_est)
        .execute(&mut *tx)
        .await?;
        count += 1;
        tokens += p.token_est;
    }
    sqlx::query(
        "INSERT INTO assistant_index_state (call_id, indexed_at, passage_count, token_total)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(call_id) DO UPDATE SET
           indexed_at = excluded.indexed_at,
           passage_count = excluded.passage_count,
           token_total = excluded.token_total",
    )
    .bind(call_id)
    .bind(now_iso())
    .bind(count)
    .bind(tokens)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((count, tokens))
}

/// Деиндексация звонка (reprocess): пассажи + index_state. FTS чистят триггеры.
pub async fn delete_call_passages(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM assistant_passages WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM assistant_index_state WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Сбросить только index_state (self-heal: звонок вернётся в backfill-sweep).
/// Пассажи НЕ трогаем — до переиндексации поиск работает по старым.
pub async fn clear_index_state(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM assistant_index_state WHERE call_id = ?1")
        .bind(call_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Хит полнотекстового поиска. rank = bm25 (меньше = релевантнее).
#[derive(Debug, Clone)]
pub struct PassageHit {
    pub id: i64,
    pub call_id: String,
    pub kind: String,
    pub speaker: Option<String>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub text: String,
    pub token_est: i64,
    pub rank: f64,
}

/// FTS5 MATCH по индексу.
///
/// `match_expr` ДОЛЖЕН быть заранее экранирован вызывающей стороной
/// (retrieval M15.5: каждый токен в кавычках) — сырой пользовательский ввод
/// сюда не передавать (MATCH-синтаксис-инъекция).
pub async fn search_fts(
    pool: &SqlitePool,
    match_expr: &str,
    limit: i64,
    only_call: Option<&str>,
    exclude_call: Option<&str>,
) -> Result<Vec<PassageHit>, AppError> {
    type Row = (
        i64,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        String,
        i64,
        f64,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT p.id, p.call_id, p.kind, p.speaker, p.start_ms, p.end_ms,
                p.text, p.token_est, bm25(assistant_fts) AS rank
         FROM assistant_fts
         JOIN assistant_passages p ON p.id = assistant_fts.rowid
         WHERE assistant_fts MATCH ?1
           AND (?2 IS NULL OR p.call_id = ?2)
           AND (?3 IS NULL OR p.call_id <> ?3)
         ORDER BY rank ASC
         LIMIT ?4",
    )
    .bind(match_expr)
    .bind(only_call)
    .bind(exclude_call)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PassageHit {
            id: r.0,
            call_id: r.1,
            kind: r.2,
            speaker: r.3,
            start_ms: r.4,
            end_ms: r.5,
            text: r.6,
            token_est: r.7,
            rank: r.8,
        })
        .collect())
}

/// Статистика для чипа «в поиске X из Y звонков · ЧЧ ч ММ мин».
/// X = проиндексированные ready-звонки, Y = все звонки, duration — по X.
pub async fn index_stats(pool: &SqlitePool) -> Result<AssistantIndexStats, AppError> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT COUNT(*) FROM assistant_index_state),
           (SELECT COUNT(*) FROM calls),
           (SELECT COALESCE(SUM(c.duration_sec), 0)
              FROM assistant_index_state s JOIN calls c ON c.id = s.call_id)",
    )
    .fetch_one(pool)
    .await?;
    Ok(AssistantIndexStats {
        indexed_calls: row.0 as u32,
        total_calls: row.1 as u32,
        total_duration_sec: row.2.max(0) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::types::{AssistantAnswerKind, AssistantSource};
    use crate::db::test_support::fresh_db;

    async fn insert_dummy_call(pool: &SqlitePool, id: &str, duration_sec: i64) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, ?2, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .bind(duration_sec)
        .execute(pool)
        .await
        .unwrap();
    }

    fn passage(kind: AssistantPassageKind, text: &str, start_ms: Option<i64>) -> PassageInput {
        PassageInput {
            kind,
            speaker: Some("Дмитрий Петров".into()),
            start_ms,
            end_ms: start_ms.map(|s| s + 10_000),
            text: text.into(),
            token_est: (text.len() / 4) as i64,
        }
    }

    fn sample_answer() -> AssistantAnswer {
        AssistantAnswer {
            kind: AssistantAnswerKind::Answer,
            text: "Ответ по фрагментам.".into(),
            sources: vec![AssistantSource {
                call_id: "c1".into(),
                call_title: "Синхрон".into(),
                start_ms: Some(48_000),
            }],
            fragments: vec![],
            fragment_tokens: 1_400,
            window_tokens: 8_192,
            escalate: None,
        }
    }

    // ── Заголовок чата ──

    #[test]
    fn title_short_question_untouched() {
        assert_eq!(
            chat_title_from_question("  Задачи Дмитрия  "),
            "Задачи Дмитрия"
        );
    }

    #[test]
    fn title_truncates_cyrillic_on_char_boundary() {
        let q = "Какие задачи взял на себя Дмитрий на этой неделе по проекту?";
        let t = chat_title_from_question(q);
        assert!(t.ends_with('…'), "got: {t}");
        assert!(t.chars().count() <= 42, "len: {}", t.chars().count());
    }

    // ── Чаты ──

    #[tokio::test]
    async fn global_chats_listed_fresh_first() {
        let db = fresh_db().await;
        let a = create_global_chat(&db.pool, "первый вопрос").await.unwrap();
        let b = create_global_chat(&db.pool, "второй вопрос").await.unwrap();
        // Активность в чате A делает его свежее B.
        append_message(&db.pool, &a.id, AssistantRole::User, "ещё вопрос", None)
            .await
            .unwrap();
        let list = list_global_chats(&db.pool).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, a.id);
        assert_eq!(list[1].id, b.id);
        assert!(list.iter().all(|c| c.call_id.is_none()));
    }

    #[tokio::test]
    async fn call_thread_is_unique_and_idempotent() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        let first = get_or_create_call_chat(&db.pool, "c1", "О чём договорились?")
            .await
            .unwrap();
        let second = get_or_create_call_chat(&db.pool, "c1", "Другой вопрос")
            .await
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.title, "О чём договорились?"); // title не перезаписан
                                                         // Тред звонка не попадает в глобальный список.
        assert!(list_global_chats(&db.pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_chat_cascades_messages() {
        let db = fresh_db().await;
        let chat = create_global_chat(&db.pool, "вопрос").await.unwrap();
        append_message(&db.pool, &chat.id, AssistantRole::User, "вопрос", None)
            .await
            .unwrap();
        delete_chat(&db.pool, &chat.id).await.unwrap();
        let msgs = get_chat_messages(&db.pool, &chat.id).await.unwrap();
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn deleting_call_cascades_thread_passages_state() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        let chat = get_or_create_call_chat(&db.pool, "c1", "вопрос")
            .await
            .unwrap();
        append_message(&db.pool, &chat.id, AssistantRole::User, "вопрос", None)
            .await
            .unwrap();
        replace_call_passages(
            &db.pool,
            "c1",
            &[passage(
                AssistantPassageKind::Transcript,
                "окно сегментации плывёт",
                Some(48_000),
            )],
        )
        .await
        .unwrap();

        sqlx::query("DELETE FROM calls WHERE id = 'c1'")
            .execute(&db.pool)
            .await
            .unwrap();

        assert!(get_call_chat(&db.pool, "c1").await.unwrap().is_none());
        assert!(get_chat_messages(&db.pool, &chat.id)
            .await
            .unwrap()
            .is_empty());
        let hits = search_fts(&db.pool, "\"сегментации\"", 10, None, None)
            .await
            .unwrap();
        assert!(hits.is_empty(), "FTS must be cleaned by cascade triggers");
        let stats = index_stats(&db.pool).await.unwrap();
        assert_eq!(stats.indexed_calls, 0);
    }

    // ── Сообщения ──

    #[tokio::test]
    async fn messages_keep_order_and_answer_json() {
        let db = fresh_db().await;
        let chat = create_global_chat(&db.pool, "вопрос").await.unwrap();
        append_message(
            &db.pool,
            &chat.id,
            AssistantRole::User,
            "Какие задачи?",
            None,
        )
        .await
        .unwrap();
        let ans = sample_answer();
        append_message(
            &db.pool,
            &chat.id,
            AssistantRole::Assistant,
            &ans.text,
            Some(&ans),
        )
        .await
        .unwrap();

        let msgs = get_chat_messages(&db.pool, &chat.id).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, AssistantRole::User);
        assert!(msgs[0].answer.is_none());
        assert_eq!(msgs[1].role, AssistantRole::Assistant);
        let restored = msgs[1].answer.as_ref().expect("answer_json restored");
        assert_eq!(restored, &ans);
    }

    // ── Пассажи + FTS ──

    #[tokio::test]
    async fn fts_trigger_sync_insert_replace_delete() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;

        // Insert → MATCH находит.
        replace_call_passages(
            &db.pool,
            "c1",
            &[
                passage(
                    AssistantPassageKind::Transcript,
                    "обсуждали приватность и локальный режим",
                    Some(62_000),
                ),
                passage(
                    AssistantPassageKind::Decision,
                    "показываем локальный режим на демо",
                    None,
                ),
            ],
        )
        .await
        .unwrap();
        let hits = search_fts(&db.pool, "\"приватность\"", 10, None, None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].call_id, "c1");
        assert_eq!(hits[0].kind, "transcript");
        assert_eq!(hits[0].start_ms, Some(62_000));

        // Re-replace (идемпотентная переиндексация) → старый текст исчез.
        replace_call_passages(
            &db.pool,
            "c1",
            &[passage(
                AssistantPassageKind::Recap,
                "итог: пилот на двадцать мест",
                None,
            )],
        )
        .await
        .unwrap();
        let old = search_fts(&db.pool, "\"приватность\"", 10, None, None)
            .await
            .unwrap();
        assert!(old.is_empty(), "replaced passages must leave FTS");
        let new = search_fts(&db.pool, "\"пилот\"", 10, None, None)
            .await
            .unwrap();
        assert_eq!(new.len(), 1);

        // Deindex → пусто.
        delete_call_passages(&db.pool, "c1").await.unwrap();
        let after = search_fts(&db.pool, "\"пилот\"", 10, None, None)
            .await
            .unwrap();
        assert!(after.is_empty());
        assert_eq!(index_stats(&db.pool).await.unwrap().indexed_calls, 0);
    }

    #[tokio::test]
    async fn fts_scope_filters_only_and_exclude() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        insert_dummy_call(&db.pool, "c2", 900).await;
        replace_call_passages(
            &db.pool,
            "c1",
            &[passage(
                AssistantPassageKind::Transcript,
                "бюджет проекта",
                Some(1_000),
            )],
        )
        .await
        .unwrap();
        replace_call_passages(
            &db.pool,
            "c2",
            &[passage(
                AssistantPassageKind::Transcript,
                "бюджет отдела",
                Some(2_000),
            )],
        )
        .await
        .unwrap();

        let only = search_fts(&db.pool, "\"бюджет\"", 10, Some("c1"), None)
            .await
            .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].call_id, "c1");

        let excl = search_fts(&db.pool, "\"бюджет\"", 10, None, Some("c1"))
            .await
            .unwrap();
        assert_eq!(excl.len(), 1);
        assert_eq!(excl[0].call_id, "c2");
    }

    #[tokio::test]
    async fn fts_prefix_query_matches_russian_inflection() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        replace_call_passages(
            &db.pool,
            "c1",
            &[passage(
                AssistantPassageKind::Transcript,
                "говорили о приватности данных",
                Some(5_000),
            )],
        )
        .await
        .unwrap();
        // Префикс-запрос (заготовка под M15.5): "приватн"* ловит «приватности».
        let hits = search_fts(&db.pool, "\"приватн\"*", 10, None, None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn replace_skips_blank_text_and_counts_tokens() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        let (count, tokens) = replace_call_passages(
            &db.pool,
            "c1",
            &[
                passage(AssistantPassageKind::Transcript, "непустой текст", Some(0)),
                passage(AssistantPassageKind::Transcript, "   ", Some(1_000)),
            ],
        )
        .await
        .unwrap();
        assert_eq!(count, 1);
        assert!(tokens > 0);
        let stats = index_stats(&db.pool).await.unwrap();
        assert_eq!(stats.indexed_calls, 1);
        assert_eq!(stats.total_calls, 1);
        assert_eq!(stats.total_duration_sec, 600);
    }

    // ── Ревью M15.2: конкурентность, malformed MATCH, пустые состояния ──

    #[tokio::test]
    async fn concurrent_get_or_create_yields_single_thread() {
        let db = fresh_db().await;
        insert_dummy_call(&db.pool, "c1", 600).await;
        let (a, b) = tokio::join!(
            get_or_create_call_chat(&db.pool, "c1", "вопрос из таска A"),
            get_or_create_call_chat(&db.pool, "c1", "вопрос из таска B"),
        );
        let a = a.unwrap();
        let b = b.unwrap();
        assert_eq!(a.id, b.id, "оба таска должны увидеть один тред");
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM assistant_chats WHERE call_id = 'c1'")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn concurrent_appends_keep_order_idx_contiguous() {
        let db = fresh_db().await;
        let chat = create_global_chat(&db.pool, "вопрос").await.unwrap();
        let mut handles = Vec::new();
        for i in 0..8 {
            let pool = db.pool.clone();
            let chat_id = chat.id.clone();
            handles.push(tokio::spawn(async move {
                append_message(
                    &pool,
                    &chat_id,
                    AssistantRole::User,
                    &format!("msg {i}"),
                    None,
                )
                .await
            }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        let idxs: Vec<(i64,)> = sqlx::query_as(
            "SELECT order_idx FROM assistant_messages WHERE chat_id = ?1 ORDER BY order_idx ASC",
        )
        .bind(&chat.id)
        .fetch_all(&db.pool)
        .await
        .unwrap();
        let got: Vec<i64> = idxs.into_iter().map(|r| r.0).collect();
        assert_eq!(
            got,
            (0..8).collect::<Vec<i64>>(),
            "order_idx без дублей и дыр"
        );
    }

    #[tokio::test]
    async fn malformed_match_expr_is_err_not_panic() {
        let db = fresh_db().await;
        // Сырой MATCH-синтаксис (незакрытая кавычка/оператор) → Err, не паника.
        let res = search_fts(&db.pool, "\"unclosed AND (", 10, None, None).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn index_stats_on_empty_db_is_zeroes() {
        let db = fresh_db().await;
        let stats = index_stats(&db.pool).await.unwrap();
        assert_eq!(stats.indexed_calls, 0);
        assert_eq!(stats.total_calls, 0);
        assert_eq!(stats.total_duration_sec, 0);
    }

    #[tokio::test]
    async fn delete_chat_nonexistent_is_idempotent_ok() {
        let db = fresh_db().await;
        delete_chat(&db.pool, "no-such-chat").await.unwrap();
    }
}
