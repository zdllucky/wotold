//! [B23] Идентификаторы контакта: нормализация payload'а и diff-синхронизация
//! строк `contact_identifiers`.
//!
//! [TD-41] Выделено из `db/contacts.rs` (899 строк при лимите 800, правило 8)
//! вместе с тестами. Граница естественная: у идентификаторов свой инвариант —
//! UNIQUE-индекс миграции 0018 превращает недедупленный payload в
//! constraint-ошибку, а обновление обязано сохранять id неизменившихся строк
//! (иначе они «моргают» в UI и ломают внешнюю синхронизацию). Логика не
//! менялась.

use std::collections::HashMap;

use sqlx::Row;

use crate::AppError;

use super::contacts::ContactIdentifierInput;

/// [B23] Нормализованный идентификатор после trim/фильтрации/дедупа.
pub(crate) struct NormalizedIdentifier {
    kind: String,
    value: String,
    label: Option<String>,
}

/// [B23] Trim kind/value, выкинуть пустые, схлопнуть дубли по (kind, value)
/// first-wins. Обязательно ДО любых INSERT: UNIQUE-индекс 0018 превращает
/// недедупленный payload в constraint-ошибку.
pub(crate) fn normalize_identifiers(
    identifiers: &[ContactIdentifierInput],
) -> Vec<NormalizedIdentifier> {
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

pub(crate) async fn insert_identifiers(
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
pub(crate) async fn sync_identifiers(
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

#[cfg(test)]
mod tests {
    use crate::db::contacts::{create_contact, update_contact};
    use crate::db::contacts::{ContactIdentifierInput, ContactInput};
    use crate::db::test_support::fresh_db;

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
