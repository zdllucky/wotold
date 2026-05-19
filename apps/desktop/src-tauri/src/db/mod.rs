use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

use crate::AppError;

mod contacts;
mod settings;

pub use contacts::{
    create_contact, delete_contact, ensure_owner_contact, list_contacts, rename_owner_contact,
    Contact, ContactInput, OwnerContact,
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
