use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerContact {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContactInput {
    pub display_name: String,
    pub org: Option<String>,
    pub role: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub display_name: String,
    pub is_owner: bool,
    pub org: Option<String>,
    pub role: Option<String>,
    pub attributes: serde_json::Value,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub identifiers: Vec<ContactIdentifier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactIdentifier {
    pub id: String,
    pub kind: String,
    pub value: String,
}

const OWNER_DEFAULT_NAME: &str = "Me";

/// Создать запись контакта-владельца при первом запуске или вернуть существующего.
/// См. M6.2 паспорта (`is_owner = 1`).
pub async fn ensure_owner_contact(pool: &SqlitePool) -> Result<OwnerContact, AppError> {
    let existing: Option<(String, String)> =
        sqlx::query_as("SELECT id, display_name FROM contacts WHERE is_owner = 1 LIMIT 1")
            .fetch_optional(pool)
            .await?;

    if let Some((id, display_name)) = existing {
        return Ok(OwnerContact { id, display_name });
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
         VALUES (?1, ?2, 1, '{}', ?3, ?3)",
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

/// Возвращает все контакты с прикреплёнными идентификаторами. owner — первым.
/// M7.4 паспорта.
pub async fn list_contacts(pool: &SqlitePool) -> Result<Vec<Contact>, AppError> {
    let contact_rows = sqlx::query(
        "SELECT id, display_name, is_owner, org, role, attributes, notes, created_at, updated_at
         FROM contacts
         ORDER BY is_owner DESC, display_name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;

    if contact_rows.is_empty() {
        return Ok(vec![]);
    }

    let id_rows = sqlx::query("SELECT id, contact_id, kind, value FROM contact_identifiers")
        .fetch_all(pool)
        .await?;

    let mut by_contact: HashMap<String, Vec<ContactIdentifier>> = HashMap::new();
    for row in id_rows {
        let contact_id: String = row.get("contact_id");
        by_contact
            .entry(contact_id)
            .or_default()
            .push(ContactIdentifier {
                id: row.get("id"),
                kind: row.get("kind"),
                value: row.get("value"),
            });
    }

    let contacts = contact_rows
        .into_iter()
        .map(|row| {
            let contact_id: String = row.get("id");
            let attrs_raw: Option<String> = row.get("attributes");
            let attributes = attrs_raw
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
            let identifiers = by_contact.remove(&contact_id).unwrap_or_default();
            Contact {
                display_name: row.get("display_name"),
                is_owner: row.get::<i64, _>("is_owner") == 1,
                org: row.get("org"),
                role: row.get("role"),
                attributes,
                notes: row.get("notes"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                identifiers,
                id: contact_id,
            }
        })
        .collect();

    Ok(contacts)
}

/// Создать контакт. display_name обязателен и не пустой (M7.4).
pub async fn create_contact(pool: &SqlitePool, input: ContactInput) -> Result<Contact, AppError> {
    let display_name = input.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(AppError::Other("display_name required".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let org = input
        .org
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let role = input
        .role
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let notes = input
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    sqlx::query(
        "INSERT INTO contacts (id, display_name, is_owner, org, role, notes, attributes, created_at, updated_at)
         VALUES (?1, ?2, 0, ?3, ?4, ?5, '{}', ?6, ?6)",
    )
    .bind(&id)
    .bind(&display_name)
    .bind(org)
    .bind(role)
    .bind(notes)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(Contact {
        id,
        display_name,
        is_owner: false,
        org: org.map(str::to_string),
        role: role.map(str::to_string),
        attributes: serde_json::Value::Object(serde_json::Map::new()),
        notes: notes.map(str::to_string),
        created_at: now.clone(),
        updated_at: now,
        identifiers: vec![],
    })
}

/// Переименовать контакт-владельца (используется в онбординге, M7.6).
pub async fn rename_owner_contact(
    pool: &SqlitePool,
    new_name: &str,
) -> Result<OwnerContact, AppError> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Other("display_name required".into()));
    }
    let now = chrono::Utc::now().to_rfc3339();

    let id: Option<String> =
        sqlx::query_scalar("SELECT id FROM contacts WHERE is_owner = 1 LIMIT 1")
            .fetch_optional(pool)
            .await?;
    let id = id.ok_or_else(|| AppError::Other("owner contact missing".into()))?;

    sqlx::query("UPDATE contacts SET display_name = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(trimmed)
        .bind(&now)
        .bind(&id)
        .execute(pool)
        .await?;

    Ok(OwnerContact {
        id,
        display_name: trimmed.to_string(),
    })
}

/// Удалить контакт. Контакт-владелец удалить нельзя (M6.2). ON DELETE CASCADE
/// в схеме автоматически зачистит contact_identifiers и voice_samples (M3.6 + C5).
pub async fn delete_contact(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
    let is_owner: Option<i64> = sqlx::query_scalar("SELECT is_owner FROM contacts WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    match is_owner {
        None => return Err(AppError::Other(format!("contact {id} not found"))),
        Some(1) => return Err(AppError::Other("cannot delete owner contact".into())),
        _ => {}
    }

    sqlx::query("DELETE FROM contacts WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}
