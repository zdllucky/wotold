/// Унифицированная ошибка приложения. Сериализуется в строку, чтобы Tauri-команды
/// возвращали человекочитаемое сообщение во фронт.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("init: {0}")]
    Init(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("db: {0}")]
    Db(#[from] sqlx::Error),

    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
