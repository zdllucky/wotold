use sqlx::SqlitePool;

use crate::AppError;

/// Key-value хранилище приложения (раздел 6.2 schema). Используется для
/// флагов вроде onboarding_done, выбора провайдеров (M7.5), последних значений UX.
pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, AppError> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(v)
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    #[tokio::test]
    async fn get_missing_key_returns_none() {
        let db = fresh_db().await;
        let v = get_setting(&db.pool, "no_such_key").await.unwrap();
        assert!(v.is_none());
    }

    #[tokio::test]
    async fn set_then_get_roundtrip() {
        let db = fresh_db().await;
        set_setting(&db.pool, "stt_provider", "soniox")
            .await
            .unwrap();
        let v = get_setting(&db.pool, "stt_provider").await.unwrap();
        assert_eq!(v.as_deref(), Some("soniox"));
    }

    #[tokio::test]
    async fn set_upserts_existing_key() {
        let db = fresh_db().await;
        set_setting(&db.pool, "llm_model", "claude-sonnet-4-5")
            .await
            .unwrap();
        set_setting(&db.pool, "llm_model", "claude-sonnet-4-6")
            .await
            .unwrap();
        let v = get_setting(&db.pool, "llm_model").await.unwrap();
        assert_eq!(v.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[tokio::test]
    async fn set_handles_empty_value() {
        let db = fresh_db().await;
        set_setting(&db.pool, "k", "").await.unwrap();
        assert_eq!(
            get_setting(&db.pool, "k").await.unwrap().as_deref(),
            Some("")
        );
    }
}
