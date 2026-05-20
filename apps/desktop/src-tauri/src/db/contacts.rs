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
pub struct ContactIdentifierInput {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContactInput {
    pub display_name: String,
    pub org: Option<String>,
    pub role: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub identifiers: Vec<ContactIdentifierInput>,
    #[serde(default)]
    pub attributes: serde_json::Value,
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
            let attributes = parse_attributes(attrs_raw.as_deref());
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

/// Один контакт по id, с прикреплёнными identifiers.
pub async fn get_contact(pool: &SqlitePool, id: &str) -> Result<Option<Contact>, AppError> {
    let row = sqlx::query(
        "SELECT id, display_name, is_owner, org, role, attributes, notes, created_at, updated_at
         FROM contacts WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let contact_id: String = row.get("id");
    let attrs_raw: Option<String> = row.get("attributes");
    let attributes = parse_attributes(attrs_raw.as_deref());

    let id_rows =
        sqlx::query("SELECT id, kind, value FROM contact_identifiers WHERE contact_id = ?1")
            .bind(&contact_id)
            .fetch_all(pool)
            .await?;

    let identifiers: Vec<ContactIdentifier> = id_rows
        .into_iter()
        .map(|r| ContactIdentifier {
            id: r.get("id"),
            kind: r.get("kind"),
            value: r.get("value"),
        })
        .collect();

    Ok(Some(Contact {
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
    }))
}

/// Создать контакт. display_name обязателен. identifiers и attributes —
/// опциональны (по умолчанию пустой массив / `{}`). M7.4 + M6.1.
pub async fn create_contact(pool: &SqlitePool, input: ContactInput) -> Result<Contact, AppError> {
    let display_name = input.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(AppError::Other("display_name required".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let attributes_str = serialize_attributes(&input.attributes)?;

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

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO contacts (id, display_name, is_owner, org, role, notes, attributes, created_at, updated_at)
         VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?7)",
    )
    .bind(&id)
    .bind(&display_name)
    .bind(org)
    .bind(role)
    .bind(notes)
    .bind(&attributes_str)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    insert_identifiers(&mut tx, &id, &input.identifiers).await?;

    tx.commit().await?;

    get_contact(pool, &id)
        .await?
        .ok_or_else(|| AppError::Other(format!("contact {id} disappeared after insert")))
}

/// Обновить контакт. Identifiers replace-all (удаляем все и вставляем новые
/// внутри транзакции). owner редактировать можно (display_name; пользователь
/// сам себе хозяин), но is_owner не меняется.
pub async fn update_contact(
    pool: &SqlitePool,
    id: &str,
    input: ContactInput,
) -> Result<Contact, AppError> {
    let display_name = input.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(AppError::Other("display_name required".into()));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let attributes_str = serialize_attributes(&input.attributes)?;

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

    let mut tx = pool.begin().await?;

    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM contacts WHERE id = ?1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(AppError::Other(format!("contact {id} not found")));
    }

    sqlx::query(
        "UPDATE contacts
         SET display_name = ?1,
             org = ?2,
             role = ?3,
             notes = ?4,
             attributes = ?5,
             updated_at = ?6
         WHERE id = ?7",
    )
    .bind(&display_name)
    .bind(org)
    .bind(role)
    .bind(notes)
    .bind(&attributes_str)
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM contact_identifiers WHERE contact_id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    insert_identifiers(&mut tx, id, &input.identifiers).await?;

    tx.commit().await?;

    get_contact(pool, id)
        .await?
        .ok_or_else(|| AppError::Other(format!("contact {id} disappeared after update")))
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

async fn insert_identifiers(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    contact_id: &str,
    identifiers: &[ContactIdentifierInput],
) -> Result<(), AppError> {
    for identifier in identifiers {
        let kind = identifier.kind.trim();
        let value = identifier.value.trim();
        if kind.is_empty() || value.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO contact_identifiers (id, contact_id, kind, value)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(contact_id)
        .bind(kind)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn parse_attributes(raw: Option<&str>) -> serde_json::Value {
    raw.and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()))
}

fn serialize_attributes(value: &serde_json::Value) -> Result<String, AppError> {
    if value.is_null() {
        return Ok("{}".to_string());
    }
    Ok(serde_json::to_string(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use serde_json::json;

    fn empty_input(name: &str) -> ContactInput {
        ContactInput {
            display_name: name.into(),
            org: None,
            role: None,
            notes: None,
            identifiers: vec![],
            attributes: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn ensure_owner_creates_then_returns_existing() {
        let db = fresh_db().await;
        let a = ensure_owner_contact(&db.pool).await.unwrap();
        let b = ensure_owner_contact(&db.pool).await.unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.display_name, OWNER_DEFAULT_NAME);
    }

    #[tokio::test]
    async fn rename_owner_updates_name_and_rejects_empty() {
        let db = fresh_db().await;
        ensure_owner_contact(&db.pool).await.unwrap();

        let renamed = rename_owner_contact(&db.pool, "Damir").await.unwrap();
        assert_eq!(renamed.display_name, "Damir");

        let err = rename_owner_contact(&db.pool, "   ").await;
        assert!(err.is_err(), "empty name must error");
    }

    #[tokio::test]
    async fn create_contact_persists_identifiers_and_attributes() {
        let db = fresh_db().await;
        let input = ContactInput {
            display_name: "Ivan".into(),
            org: Some("Acme".into()),
            role: Some("CTO".into()),
            notes: Some("VIP".into()),
            identifiers: vec![ContactIdentifierInput {
                kind: "email".into(),
                value: "ivan@acme.kz".into(),
            }],
            attributes: json!({"linkedin": "ivan"}),
        };
        let created = create_contact(&db.pool, input).await.unwrap();
        assert_eq!(created.display_name, "Ivan");
        assert_eq!(created.identifiers.len(), 1);
        assert_eq!(created.identifiers[0].value, "ivan@acme.kz");
        assert_eq!(created.attributes["linkedin"], "ivan");
        assert!(!created.is_owner);
    }

    #[tokio::test]
    async fn list_contacts_includes_owner_first() {
        let db = fresh_db().await;
        ensure_owner_contact(&db.pool).await.unwrap();
        create_contact(&db.pool, empty_input("Bob")).await.unwrap();
        let all = list_contacts(&db.pool).await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].is_owner, "owner first per ordering");
    }

    #[tokio::test]
    async fn delete_contact_removes_row_but_not_owner() {
        let db = fresh_db().await;
        let owner = ensure_owner_contact(&db.pool).await.unwrap();
        let c = create_contact(&db.pool, empty_input("Bob")).await.unwrap();

        delete_contact(&db.pool, &c.id).await.unwrap();
        let after = list_contacts(&db.pool).await.unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, owner.id);
    }

    #[test]
    fn parse_attributes_handles_null_and_invalid() {
        let v = parse_attributes(None);
        assert!(v.is_object());
        let v = parse_attributes(Some("not json"));
        assert!(v.is_object());
        let v = parse_attributes(Some("{\"k\":\"v\"}"));
        assert_eq!(v["k"], "v");
    }

    #[test]
    fn serialize_attributes_handles_null() {
        let s = serialize_attributes(&serde_json::Value::Null).unwrap();
        assert_eq!(s, "{}");
    }
}
