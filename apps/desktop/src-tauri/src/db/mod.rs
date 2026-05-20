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
mod voice_samples;

pub use action_items::{list_action_items, replace_action_items, ActionItem, ActionItemInput};
pub use calls::{
    auto_bind_owner_speaker, confirm_call_speaker, delete_call_and_samples,
    ensure_call_speakers_present, fail_recording, fail_recording_with_reason, finish_recording,
    get_call, insert_recording, insert_speaker_suggestions, list_call_speakers, list_calls,
    mark_call_ready, set_call_meta, set_recap_failed_reason, sweep_stale_calls,
    unbind_call_speaker, Call, CallSpeakerView,
};
pub use contacts::{
    create_contact, delete_contact, ensure_owner_contact, list_contacts, rename_owner_contact,
    update_contact, Contact, ContactInput, OwnerContact,
};
pub use settings::{get_setting, set_setting};
pub use voice_samples::{delete_voice_sample, list_voice_samples, VoiceSampleView};

const DB_FILE: &str = "app.db";

pub async fn init(app_data_dir: &Path) -> Result<SqlitePool, AppError> {
    let path = app_data_dir.join(DB_FILE);

    // [B16 audit P0] integrity_check перед открытием — если БД corrupt (partial
    // WAL write, force-quit во время migrations), переименовываем в *.corrupt-{ts}
    // и стартуем с пустой. Юзер увидит модал в UI через event 'db:reset' что
    // была пересборка БД (пока без модала — TODO в #refactor).
    if path.exists() {
        if let Err(e) = quick_integrity_check(&path).await {
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            let corrupt_path = app_data_dir.join(format!("{DB_FILE}.corrupt-{ts}"));
            log::error!(
                "SQLite integrity check failed: {e}. Renaming {} → {}",
                path.display(),
                corrupt_path.display()
            );
            std::fs::rename(&path, &corrupt_path).ok();
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        // [B16 audit P1] busy_timeout 5s — concurrent writes (sweep + pipeline)
        // могут upcкать SQLITE_BUSY; даём wait вместо мгновенного fail.
        .busy_timeout(std::time::Duration::from_secs(5))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

/// Открывает БД на одиночный pragma integrity_check; если не вернёт 'ok' —
/// файл повреждён. Используем минимальный pool (1 connection) + закрываем
/// сразу после check чтобы не держать handle.
async fn quick_integrity_check(path: &Path) -> Result<(), AppError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(3));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let row: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await?;
    pool.close().await;
    if row.0 != "ok" {
        return Err(AppError::Other(format!("integrity_check returned: {}", row.0)));
    }
    Ok(())
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
