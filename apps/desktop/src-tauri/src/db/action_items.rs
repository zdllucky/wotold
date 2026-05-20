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
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionItemInput {
    pub text: String,
    pub owner_contact_id: Option<String>,
    pub due: Option<String>,
}

/// Вернуть все action_items одного звонка отсортированными по порядку вставки.
pub async fn list_action_items(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Vec<ActionItem>, AppError> {
    type Row = (String, String, String, Option<String>, Option<String>, i64);

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, call_id, text, owner_contact_id, due, done
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
        sqlx::query(
            "INSERT INTO action_items (id, call_id, text, owner_contact_id, due, done)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(call_id)
        .bind(text)
        .bind(item.owner_contact_id.as_deref())
        .bind(item.due.as_deref())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
