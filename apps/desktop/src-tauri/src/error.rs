/// Унифицированная ошибка приложения. Сериализуется в строку, чтобы Tauri-команды
/// возвращали человекочитаемое сообщение во фронт.
///
/// [Phase 1 R6] Typed variants для часто-используемых случаев — раньше всё
/// летело через `Other(String)`, frontend не мог retry-логику ветвить.
/// Сериализация остаётся строковая → frontend (`humanError`) backwards-compat,
/// миграция callsite'ов постепенная.
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

    /// Resource (row, file, key) не найден. UI отрендерит «не найдено»
    /// вместо technical reason. Использовать вместо `Other(format!("X not found"))`.
    #[error("not found: {0}")]
    NotFound(String),

    /// Операция запрошена в неподходящем состоянии (нет аудио для reprocess,
    /// нет transcript для regenerate, и т.п.). Не баг, не fatal — юзер
    /// делает не то.
    #[error("precondition failed: {0}")]
    PreconditionFailed(String),

    /// Разрешения OS / Permissions / BYO key отсутствуют. UI знает что надо
    /// открыть Settings.
    #[error("permission: {0}")]
    Permission(String),

    /// Партнёрский провайдер (Soniox/Gladia/Anthropic/Groq) или прокси
    /// ответил ошибкой. Часто retry'able, retry-policy решает выше.
    #[error("provider: {0}")]
    Provider(String),

    /// Legacy bucket — постепенно мигрируется в typed variants.
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
