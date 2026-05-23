use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: String,
    pub call_id: String,
    pub text: String,
    pub owner_contact_id: Option<String>,
    pub due: Option<String>,
    pub done: bool,
    // [M14 T-02] V2 enrichment fields (migration 0015). Все nullable —
    // legacy rows (schema v1) имеют NULL; новые cloud v2 заполняют.
    pub owner_confidence: Option<f64>,
    pub due_confidence: Option<f64>,
    /// 'commitment' | 'proposal' | 'idea'. DB default 'commitment'.
    pub category: Option<String>,
    pub evidence_quote: Option<String>,
    pub evidence_speaker: Option<String>,
    pub evidence_start_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActionItemInput {
    pub text: String,
    pub owner_contact_id: Option<String>,
    pub due: Option<String>,
    // [M14 T-02] V2 fields — None для legacy callsites; cloud v2 fills.
    #[serde(default)]
    pub owner_confidence: Option<f64>,
    #[serde(default)]
    pub due_confidence: Option<f64>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub evidence_quote: Option<String>,
    #[serde(default)]
    pub evidence_speaker: Option<String>,
    #[serde(default)]
    pub evidence_start_ms: Option<i64>,
}

/// Вернуть все action_items одного звонка отсортированными по порядку вставки.
pub async fn list_action_items(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Vec<ActionItem>, AppError> {
    // [M14 T-02] Расширенный SELECT — добавлены v2 поля. Tuple type:
    // (id, call_id, text, owner_contact_id, due, done, owner_confidence,
    //  due_confidence, category, evidence_quote, evidence_speaker, evidence_start_ms).
    type Row = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        i64,
        Option<f64>,
        Option<f64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
    );

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, call_id, text, owner_contact_id, due, done,
                owner_confidence, due_confidence, category,
                evidence_quote, evidence_speaker, evidence_start_ms
         FROM action_items
         WHERE call_id = ?1
         ORDER BY rowid",
    )
    .bind(call_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ActionItem {
            id: r.0,
            call_id: r.1,
            text: r.2,
            owner_contact_id: r.3,
            due: r.4,
            done: r.5 != 0,
            owner_confidence: r.6,
            due_confidence: r.7,
            category: r.8,
            evidence_quote: r.9,
            evidence_speaker: r.10,
            evidence_start_ms: r.11,
        })
        .collect())
}

/// Заменяет все action_items для конкретного звонка (replace-all
/// стратегия — рекап перегенерируется целиком, не diff'ом). Транзакция.
pub async fn replace_action_items(
    pool: &SqlitePool,
    call_id: &str,
    items: &[ActionItemInput],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM action_items WHERE call_id = ?1")
        .bind(call_id)
        .execute(&mut *tx)
        .await?;
    for item in items {
        let text = item.text.trim();
        if text.is_empty() {
            continue;
        }
        // [M14 T-02] INSERT включает v2 поля. Legacy callers передают None
        // → SQLite NULL; DB default 'commitment' для category применяется
        // если bind тоже None (мы передаём явно для clarity).
        sqlx::query(
            "INSERT INTO action_items (
                id, call_id, text, owner_contact_id, due, done,
                owner_confidence, due_confidence, category,
                evidence_quote, evidence_speaker, evidence_start_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(call_id)
        .bind(text)
        .bind(item.owner_contact_id.as_deref())
        .bind(item.due.as_deref())
        .bind(item.owner_confidence)
        .bind(item.due_confidence)
        .bind(item.category.as_deref())
        .bind(item.evidence_quote.as_deref())
        .bind(item.evidence_speaker.as_deref())
        .bind(item.evidence_start_ms)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
