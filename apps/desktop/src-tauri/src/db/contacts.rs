use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerContact {
    pub id: String,
    pub display_name: String,
}

const OWNER_DEFAULT_NAME: &str = "Me";

/// Создать запись контакта-владельца при первом запуске или вернуть существующего.
/// См. M6.2 паспорта (`is_owner = 1`).
pub async fn ensure_owner_contact(pool: &SqlitePool) -> Result<OwnerContact, AppError> {
    let existing: Option<(String, String)> = sqlx::query_as(
        "SELECT id, display_name FROM contacts WHERE is_owner = 1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    if let Some((id, display_name)) = existing {
        return Ok(OwnerContact { id, display_name });
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)\n         VALUES (?1, ?2, 1, '{}', ?3, ?3)",
    )
    .bind(&id)
    .bind(OWNER_DEFAULT_NAME)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(OwnerContact {
        id,
        display_name: OWNER_DEFAULT_NAME.to_string(),
    })
}
