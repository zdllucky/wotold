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
    /// [B23] vCard-метка (home/work). Старые payload'ы без поля валидны.
    #[serde(default)]
    pub label: Option<String>,
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
    /// [B23] Происхождение записи: 'local' | будущие 'imported:*' (M6.4).
    #[serde(default = "default_source")]
    pub source: String,
    /// [B23] Id записи у внешнего провайдера (NULL до появления импорта).
    #[serde(default)]
    pub external_id: Option<String>,
    /// [B23] etag/rev провайдера для change-detection при будущем синке.
    #[serde(default)]
    pub external_etag: Option<String>,
    pub identifiers: Vec<ContactIdentifier>,
}

fn default_source() -> String {
    "local".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactIdentifier {
    pub id: String,
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub label: Option<String>,
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
        "SELECT id, display_name, is_owner, org, role, attributes, notes, created_at, updated_at,
                source, external_id, external_etag
         FROM contacts
         ORDER BY is_owner DESC, display_name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;

    if contact_rows.is_empty() {
        return Ok(vec![]);
    }

    let id_rows = sqlx::query("SELECT id, contact_id, kind, value, label FROM contact_identifiers")
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
                label: row.get("label"),
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
                source: row.get("source"),
                external_id: row.get("external_id"),
                external_etag: row.get("external_etag"),
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
        "SELECT id, display_name, is_owner, org, role, attributes, notes, created_at, updated_at,
                source, external_id, external_etag
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
        sqlx::query("SELECT id, kind, value, label FROM contact_identifiers WHERE contact_id = ?1")
            .bind(&contact_id)
            .fetch_all(pool)
            .await?;

    let identifiers: Vec<ContactIdentifier> = id_rows
        .into_iter()
        .map(|r| ContactIdentifier {
            id: r.get("id"),
            kind: r.get("kind"),
            value: r.get("value"),
            label: r.get("label"),
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
        source: row.get("source"),
        external_id: row.get("external_id"),
        external_etag: row.get("external_etag"),
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

    insert_identifiers(&mut tx, &id, &normalize_identifiers(&input.identifiers)).await?;

    tx.commit().await?;

    get_contact(pool, &id)
        .await?
        .ok_or_else(|| AppError::Other(format!("contact {id} disappeared after insert")))
}

/// Обновить контакт. Identifiers — diff-preserve ([B23]): совпавшие по
/// (kind, value) строки сохраняют свой id (стабильная идентичность для
/// будущего синка, M6.4), label обновляется на месте, новые вставляются,
/// исчезнувшие удаляются — всё внутри транзакции. owner редактировать можно
/// (display_name; пользователь сам себе хозяин), но is_owner не меняется.
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
        return Err(AppError::NotFound(format!("contact {id}")));
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

    sync_identifiers(&mut tx, id, &normalize_identifiers(&input.identifiers)).await?;

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
        None => return Err(AppError::NotFound(format!("contact {id}"))),
        Some(1) => return Err(AppError::Other("cannot delete owner contact".into())),
        _ => {}
    }

    sqlx::query("DELETE FROM contacts WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

/// [B23] Нормализованный идентификатор после trim/фильтрации/дедупа.
struct NormalizedIdentifier {
    kind: String,
    value: String,
    label: Option<String>,
}

/// [B23] Trim kind/value, выкинуть пустые, схлопнуть дубли по (kind, value)
/// first-wins. Обязательно ДО любых INSERT: UNIQUE-индекс 0018 превращает
/// недедупленный payload в constraint-ошибку.
fn normalize_identifiers(identifiers: &[ContactIdentifierInput]) -> Vec<NormalizedIdentifier> {
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for identifier in identifiers {
        let kind = identifier.kind.trim().to_string();
        let value = identifier.value.trim().to_string();
        if kind.is_empty() || value.is_empty() {
            continue;
        }
        if !seen.insert((kind.clone(), value.clone())) {
            continue;
        }
        let label = identifier
            .label
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        out.push(NormalizedIdentifier { kind, value, label });
    }
    out
}

async fn insert_identifiers(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    contact_id: &str,
    identifiers: &[NormalizedIdentifier],
) -> Result<(), AppError> {
    for identifier in identifiers {
        sqlx::query(
            "INSERT INTO contact_identifiers (id, contact_id, kind, value, label)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(contact_id)
        .bind(&identifier.kind)
        .bind(&identifier.value)
        .bind(&identifier.label)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// [B23] Diff-preserve: строки с неизменной (kind, value) сохраняют id
/// (label — UPDATE при отличии), новые — INSERT, исчезнувшие — DELETE.
/// Plain INSERT (не OR IGNORE): payload уже нормализован, реальные ошибки
/// не маскируем.
///
/// TODO(M6.4): матчинг (kind, value) сейчас case-sensitive (BINARY, как и
/// UNIQUE-индекс). Внешний провайдер, прислав `John@Test.com` vs
/// `john@test.com`, не сматчится со строкой и получит новый id — перед
/// реализацией импорта нужна политика нормализации value (lowercase для
/// email; для произвольных kind — решить).
async fn sync_identifiers(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    contact_id: &str,
    incoming: &[NormalizedIdentifier],
) -> Result<(), AppError> {
    let existing_rows =
        sqlx::query("SELECT id, kind, value, label FROM contact_identifiers WHERE contact_id = ?1")
            .bind(contact_id)
            .fetch_all(&mut **tx)
            .await?;

    let mut existing: HashMap<(String, String), (String, Option<String>)> = existing_rows
        .into_iter()
        .map(|r| {
            (
                (r.get::<String, _>("kind"), r.get::<String, _>("value")),
                (
                    r.get::<String, _>("id"),
                    r.get::<Option<String>, _>("label"),
                ),
            )
        })
        .collect();

    for identifier in incoming {
        let key = (identifier.kind.clone(), identifier.value.clone());
        if let Some((row_id, old_label)) = existing.remove(&key) {
            if old_label != identifier.label {
                sqlx::query("UPDATE contact_identifiers SET label = ?1 WHERE id = ?2")
                    .bind(&identifier.label)
                    .bind(&row_id)
                    .execute(&mut **tx)
                    .await?;
            }
        } else {
            sqlx::query(
                "INSERT INTO contact_identifiers (id, contact_id, kind, value, label)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(contact_id)
            .bind(&identifier.kind)
            .bind(&identifier.value)
            .bind(&identifier.label)
            .execute(&mut **tx)
            .await?;
        }
    }

    for (_, (row_id, _)) in existing {
        sqlx::query("DELETE FROM contact_identifiers WHERE id = ?1")
            .bind(&row_id)
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
                label: None,
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

    // ── [B23] Sync-ready: diff-preserve идентификаторов + новые колонки ──

    fn ident(kind: &str, value: &str) -> ContactIdentifierInput {
        ContactIdentifierInput {
            kind: kind.into(),
            value: value.into(),
            label: None,
        }
    }

    fn input_with_idents(name: &str, idents: Vec<ContactIdentifierInput>) -> ContactInput {
        ContactInput {
            display_name: name.into(),
            org: None,
            role: None,
            notes: None,
            identifiers: idents,
            attributes: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn create_contact_defaults_source_local() {
        let db = fresh_db().await;
        let c = create_contact(&db.pool, empty_input("Bob")).await.unwrap();
        assert_eq!(c.source, "local");
        assert!(c.external_id.is_none());
        assert!(c.external_etag.is_none());
    }

    #[tokio::test]
    async fn update_contact_preserves_unchanged_identifier_ids() {
        let db = fresh_db().await;
        let created = create_contact(
            &db.pool,
            input_with_idents(
                "Ivan",
                vec![ident("phone", "+7700"), ident("email", "i@a.kz")],
            ),
        )
        .await
        .unwrap();
        let phone_id = created
            .identifiers
            .iter()
            .find(|i| i.kind == "phone")
            .unwrap()
            .id
            .clone();
        let email_id = created
            .identifiers
            .iter()
            .find(|i| i.kind == "email")
            .unwrap()
            .id
            .clone();

        let updated = update_contact(
            &db.pool,
            &created.id,
            input_with_idents(
                "Ivan",
                vec![
                    ident("phone", "+7700"),
                    ident("email", "i@a.kz"),
                    ident("telegram", "@ivan"),
                ],
            ),
        )
        .await
        .unwrap();

        assert_eq!(updated.identifiers.len(), 3);
        let phone_after = updated
            .identifiers
            .iter()
            .find(|i| i.kind == "phone")
            .unwrap();
        let email_after = updated
            .identifiers
            .iter()
            .find(|i| i.kind == "email")
            .unwrap();
        assert_eq!(
            phone_after.id, phone_id,
            "unchanged identifier keeps stable id"
        );
        assert_eq!(
            email_after.id, email_id,
            "unchanged identifier keeps stable id"
        );
        assert!(updated.identifiers.iter().any(|i| i.kind == "telegram"));
    }

    #[tokio::test]
    async fn update_contact_deletes_removed_identifiers() {
        let db = fresh_db().await;
        let created = create_contact(
            &db.pool,
            input_with_idents(
                "Ivan",
                vec![ident("phone", "+7700"), ident("email", "i@a.kz")],
            ),
        )
        .await
        .unwrap();
        let email_id = created
            .identifiers
            .iter()
            .find(|i| i.kind == "email")
            .unwrap()
            .id
            .clone();

        let updated = update_contact(
            &db.pool,
            &created.id,
            input_with_idents("Ivan", vec![ident("email", "i@a.kz")]),
        )
        .await
        .unwrap();

        assert_eq!(updated.identifiers.len(), 1);
        assert_eq!(updated.identifiers[0].id, email_id);
    }

    #[tokio::test]
    async fn create_and_update_dedup_duplicate_identifiers_in_payload() {
        let db = fresh_db().await;
        // Дубль в create-payload → одна строка (иначе UNIQUE-индекс упадёт).
        let created = create_contact(
            &db.pool,
            input_with_idents(
                "Ivan",
                vec![ident("phone", "+7700"), ident("phone", "+7700")],
            ),
        )
        .await
        .unwrap();
        assert_eq!(created.identifiers.len(), 1);

        // Дубль в update-payload — тоже без ошибки, одна строка.
        let updated = update_contact(
            &db.pool,
            &created.id,
            input_with_idents(
                "Ivan",
                vec![ident("phone", "+7700"), ident("phone", "+7700")],
            ),
        )
        .await
        .unwrap();
        assert_eq!(updated.identifiers.len(), 1);
    }

    #[tokio::test]
    async fn update_contact_updates_label_in_place() {
        let db = fresh_db().await;
        let created = create_contact(
            &db.pool,
            input_with_idents("Ivan", vec![ident("phone", "+7700")]),
        )
        .await
        .unwrap();
        let phone_id = created.identifiers[0].id.clone();

        let updated = update_contact(
            &db.pool,
            &created.id,
            input_with_idents(
                "Ivan",
                vec![ContactIdentifierInput {
                    kind: "phone".into(),
                    value: "+7700".into(),
                    label: Some("work".into()),
                }],
            ),
        )
        .await
        .unwrap();

        assert_eq!(updated.identifiers.len(), 1);
        assert_eq!(updated.identifiers[0].id, phone_id, "label change keeps id");
        assert_eq!(updated.identifiers[0].label.as_deref(), Some("work"));
    }

    #[tokio::test]
    async fn unique_index_blocks_duplicate_identifier_rows() {
        let db = fresh_db().await;
        let created = create_contact(
            &db.pool,
            input_with_idents("Ivan", vec![ident("phone", "+7700")]),
        )
        .await
        .unwrap();

        // Сырой INSERT второй идентичной (contact_id, kind, value) строки
        // должен упасть об contact_identifiers_uniq (0018).
        let res = sqlx::query(
            "INSERT INTO contact_identifiers (id, contact_id, kind, value)
             VALUES (?1, ?2, 'phone', '+7700')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&created.id)
        .execute(&db.pool)
        .await;
        assert!(
            res.is_err(),
            "UNIQUE(contact_id, kind, value) must reject dup"
        );
    }

    #[tokio::test]
    async fn migration_0018_dedups_preexisting_duplicate_identifiers() {
        // Отдельный in-memory pool: применяем ТОЛЬКО 0001, создаём дубли
        // (как их копил legacy replace-all), затем применяем 0018 и убеждаемся
        // что дедуп прошёл и UNIQUE-индекс создался.
        use sqlx::sqlite::SqlitePoolOptions;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        let m0001 = include_str!("../../migrations/0001_initial.sql");
        sqlx::raw_sql(m0001).execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES ('c1', 'Ivan', 0, '{}', 'now', 'now')",
        )
        .execute(&pool)
        .await
        .unwrap();
        for i in 0..3 {
            sqlx::query(
                "INSERT INTO contact_identifiers (id, contact_id, kind, value)
                 VALUES (?1, 'c1', 'phone', '+7700')",
            )
            .bind(format!("dup-{i}"))
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO contact_identifiers (id, contact_id, kind, value)
             VALUES ('uniq-1', 'c1', 'email', 'i@a.kz')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let m0018 = include_str!("../../migrations/0018_contacts_sync_ready.sql");
        sqlx::raw_sql(m0018).execute(&pool).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contact_identifiers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2, "3 дубля схлопнуты в 1 + 1 уникальная");

        // Выжила самая ранняя строка (MIN(rowid) = первый INSERT).
        let survivor: String =
            sqlx::query_scalar("SELECT id FROM contact_identifiers WHERE kind = 'phone'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(survivor, "dup-0");
    }
}
