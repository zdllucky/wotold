use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

use crate::AppError;

mod action_items;
mod calls;
mod contacts;
mod settings;

pub use action_items::{list_action_items, replace_action_items, ActionItem, ActionItemInput};
pub use calls::{
    confirm_call_speaker, delete_call_and_samples, fail_recording, fail_recording_with_reason,
    finish_recording, get_call, insert_recording, insert_speaker_suggestions, list_call_speakers,
    list_calls, mark_call_ready, set_call_meta, sweep_stale_calls, unbind_call_speaker, Call,
    CallSpeakerView,
};
pub use contacts::{
    create_contact, delete_contact, ensure_owner_contact, list_contacts, rename_owner_contact,
    update_contact, Contact, ContactInput, OwnerContact,
};
pub use settings::{get_setting, set_setting};

const DB_FILE: &str = "app.db";

pub async fn init(app_data_dir: &Path) -> Result<SqlitePool, AppError> {
    let path = app_data_dir.join(DB_FILE);
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use tempfile::TempDir;

    pub struct TestDb {
        pub pool: SqlitePool,
        _dir: TempDir,
    }

    pub async fn fresh_db() -> TestDb {
        let dir = tempfile::tempdir().expect("create temp dir");
        let pool = init(dir.path()).await.expect("init db");
        TestDb { pool, _dir: dir }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::fresh_db;
    use sqlx::Row;

    #[tokio::test]
    async fn init_runs_migrations_and_creates_settings_table() {
        let db = fresh_db().await;
        let row = sqlx::query(
            "SELECT count(*) AS n FROM sqlite_master WHERE type='table' AND name='settings'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        let n: i64 = row.try_get("n").unwrap();
        assert_eq!(n, 1, "settings table must exist after migrations");
    }

    #[tokio::test]
    async fn init_enables_foreign_keys() {
        let db = fresh_db().await;
        let row = sqlx::query("PRAGMA foreign_keys")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let on: i64 = row.try_get(0).unwrap();
        assert_eq!(on, 1);
    }
}
