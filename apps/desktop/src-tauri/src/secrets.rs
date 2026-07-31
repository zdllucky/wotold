//! Generic keychain-seam для будущих внешних интеграций.
//!
//! Cloud/BYO-провайдеры (Soniox/Gladia/Anthropic) и proxy-auth session удалены
//! при переходе на local-only. Механизм безопасного хранения секретов оставлен
//! как задел под будущую опциональную интеграцию «внешний Claude-софт для
//! распознавания/транскрипции» (пользовательские ключи/токены): значения живут
//! ТОЛЬКО в системном Keychain (macOS) / Credential Manager (Windows) / Secret
//! Service (Linux) через `keyring` crate — никогда в БД, env-файлах или логах.
//!
//! W5: security-sensitive. Пока потребителей нет — `#[allow(dead_code)]` на
//! публичном API (reserved seam), удалить при подключении первой интеграции.

use keyring::Entry;

use crate::AppError;

fn entry(account: &str) -> Result<Entry, AppError> {
    // [env-split] Service — по identifier сборки: иначе dev перезаписывает
    // боевые токены пользователя своими тестовыми под тем же именем.
    Entry::new(crate::app_env::identifier(), account)
        .map_err(|e| AppError::Other(format!("keychain entry init: {e}")))
}

/// Записать секрет под generic account-именем. Пустое значение отвергается.
/// Затирает старое значение. Никогда не логирует само значение.
#[allow(dead_code)]
pub fn set_secret(account: &str, value: &str) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::Other("empty secret value".into()));
    }
    entry(account)?
        .set_password(value)
        .map_err(|e| AppError::Other(format!("keychain set: {e}")))?;
    log::info!("secret updated for {account} (value length withheld)");
    Ok(())
}

/// Прочитать секрет. `Ok(None)` если записи нет или значение пустое.
#[allow(dead_code)]
pub fn read_secret(account: &str) -> Result<Option<String>, AppError> {
    match entry(account)?.get_password() {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Other(format!("keychain read: {e}"))),
    }
}

/// Есть ли секрет — без чтения значения (для UI status badge).
#[allow(dead_code)]
pub fn has_secret(account: &str) -> Result<bool, AppError> {
    Ok(read_secret(account)?.is_some())
}

/// Удалить секрет. Idempotent — отсутствие записи не считается ошибкой.
#[allow(dead_code)]
pub fn delete_secret(account: &str) -> Result<(), AppError> {
    match entry(account)?.delete_credential() {
        Ok(()) => {
            log::info!("secret deleted for {account}");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Other(format!("keychain delete: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip_if_no_keychain() -> bool {
        // [CI-fix] На GitHub macOS runners `Entry::new()` СОЗДАЁТСЯ успешно,
        // но реальные keychain-операции блокируются навсегда — нет
        // интерактивной keychain-сессии для unlock authorization → тесты
        // висят. Безусловный skip на CI (`CI=true` ставит GitHub Actions).
        if std::env::var("CI").is_ok() {
            return true;
        }
        Entry::new("app.wotold.desktop.test.probe", "probe").is_err()
    }

    #[test]
    fn set_empty_value_returns_error() {
        if skip_if_no_keychain() {
            return;
        }
        let err = set_secret("wotold_test_seam", "").unwrap_err();
        assert!(matches!(err, AppError::Other(_)));
    }

    #[test]
    fn set_read_has_delete_roundtrip() {
        if skip_if_no_keychain() {
            return;
        }
        let account = "wotold_test_seam_roundtrip";
        let _ = delete_secret(account);
        assert!(!has_secret(account).unwrap());

        set_secret(account, "sk-test-roundtrip-7726").unwrap();
        assert!(has_secret(account).unwrap());
        assert_eq!(
            read_secret(account).unwrap().as_deref(),
            Some("sk-test-roundtrip-7726")
        );

        delete_secret(account).unwrap();
        assert!(!has_secret(account).unwrap());
        // Второй delete idempotent.
        delete_secret(account).unwrap();
    }
}
