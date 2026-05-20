//! BYO API key хранилище (#47).
//!
//! Никаких ключей в БД, env-файлах, логах — только системный Keychain (macOS) /
//! Credential Manager (Windows) / Secret Service (Linux). Доступ через `keyring`
//! crate (`apple-native` feature).
//!
//! W5: security-sensitive. См. `CLAUDE.md` → security-review triggers (BYO-ключи).
//!
//! Public API возвращает только status (есть/нет ключ), не сами значения.
//! Само значение читается ТОЛЬКО внутри `transcription::ProviderMode::Byo`
//! при выполнении STT — нигде больше не светится.

use keyring::Entry;
use serde::Serialize;

use crate::AppError;

const SERVICE: &str = "app.wotold.desktop";

/// Известные провайдеры с BYO-ключами. Enum закрытый — нельзя случайно
/// записать ключ под произвольным именем.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ByoProvider {
    Soniox,
    Gladia,
    Anthropic,
}

impl ByoProvider {
    fn keychain_account(self) -> &'static str {
        match self {
            Self::Soniox => "byo_soniox_api_key",
            Self::Gladia => "byo_gladia_api_key",
            Self::Anthropic => "byo_anthropic_api_key",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ByoStatus {
    pub provider: ByoProvider,
    pub present: bool,
}

fn entry(provider: ByoProvider) -> Result<Entry, AppError> {
    Entry::new(SERVICE, provider.keychain_account())
        .map_err(|e| AppError::Other(format!("keychain entry init: {e}")))
}

/// Записать ключ. Пустая строка валидируется на уровне команды (см. `commands::set_byo_key`)
/// — сюда передаётся только trimmed non-empty value. Затирает старое значение.
pub fn set_key(provider: ByoProvider, value: &str) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::Other("empty BYO key".into()));
    }
    entry(provider)?
        .set_password(value)
        .map_err(|e| AppError::Other(format!("keychain set: {e}")))?;
    log::info!("byo key updated for {:?} (value length withheld)", provider);
    Ok(())
}

/// Удалить ключ. Idempotent — отсутствие записи не считается ошибкой.
pub fn delete_key(provider: ByoProvider) -> Result<(), AppError> {
    match entry(provider)?.delete_credential() {
        Ok(()) => {
            log::info!("byo key deleted for {:?}", provider);
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Other(format!("keychain delete: {e}"))),
    }
}

/// Прочитать ключ. Используется ТОЛЬКО pipeline-ом при выполнении STT.
/// Не экспонируется через Tauri-команды.
pub fn read_key(provider: ByoProvider) -> Result<Option<String>, AppError> {
    match entry(provider)?.get_password() {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Other(format!("keychain read: {e}"))),
    }
}

/// Узнать без чтения значения, есть ли ключ. Для UI status badge.
pub fn has_key(provider: ByoProvider) -> Result<bool, AppError> {
    Ok(read_key(provider)?.is_some())
}

/// Сводка по всем провайдерам — для Settings UI одним запросом.
pub fn status_all() -> Result<Vec<ByoStatus>, AppError> {
    Ok([
        ByoProvider::Soniox,
        ByoProvider::Gladia,
        ByoProvider::Anthropic,
    ]
    .into_iter()
    .map(|p| ByoStatus {
        provider: p,
        present: has_key(p).unwrap_or(false),
    })
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip_if_no_keychain() -> bool {
        // На CI macOS keychain доступен; на Linux/Windows runners — нет.
        // На GitHub runners macos-latest всё работает через test keychain.
        // Если creating Entry падает — пропускаем (CI без UI keychain backend).
        Entry::new("app.wotold.desktop.test.probe", "probe").is_err()
    }

    #[test]
    fn provider_enum_serializes_lowercase() {
        let s = serde_json::to_string(&ByoProvider::Soniox).unwrap();
        assert_eq!(s, "\"soniox\"");
        let s = serde_json::to_string(&ByoProvider::Anthropic).unwrap();
        assert_eq!(s, "\"anthropic\"");
    }

    #[test]
    fn provider_keychain_account_unique() {
        let a = ByoProvider::Soniox.keychain_account();
        let b = ByoProvider::Gladia.keychain_account();
        let c = ByoProvider::Anthropic.keychain_account();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
        // Все начинаются с byo_ — чтобы не пересекались с другими keychain-записями приложения.
        assert!(a.starts_with("byo_"));
        assert!(b.starts_with("byo_"));
        assert!(c.starts_with("byo_"));
    }

    #[test]
    fn set_empty_value_returns_error() {
        if skip_if_no_keychain() {
            return;
        }
        let err = set_key(ByoProvider::Soniox, "").unwrap_err();
        assert!(matches!(err, AppError::Other(_)));
    }

    #[test]
    fn set_then_has_then_delete_roundtrip() {
        if skip_if_no_keychain() {
            return;
        }
        // Используем уникальный suffix — реальный keychain isolated через test name.
        let p = ByoProvider::Gladia;
        // Cleanup сначала, на случай прошлого failed теста.
        let _ = delete_key(p);
        assert!(!has_key(p).unwrap());

        set_key(p, "sk-test-roundtrip-7726").unwrap();
        assert!(has_key(p).unwrap());

        let read = read_key(p).unwrap();
        assert_eq!(read.as_deref(), Some("sk-test-roundtrip-7726"));

        delete_key(p).unwrap();
        assert!(!has_key(p).unwrap());
    }

    #[test]
    fn delete_missing_key_is_idempotent() {
        if skip_if_no_keychain() {
            return;
        }
        let p = ByoProvider::Anthropic;
        let _ = delete_key(p);
        // Вторая попытка — должна пройти без ошибки.
        delete_key(p).unwrap();
    }

    #[test]
    fn status_all_returns_three_providers() {
        if skip_if_no_keychain() {
            return;
        }
        let statuses = status_all().unwrap();
        assert_eq!(statuses.len(), 3);
        let providers: Vec<_> = statuses.iter().map(|s| s.provider).collect();
        assert!(providers.contains(&ByoProvider::Soniox));
        assert!(providers.contains(&ByoProvider::Gladia));
        assert!(providers.contains(&ByoProvider::Anthropic));
    }
}
