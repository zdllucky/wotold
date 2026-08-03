//! [env-split] Разделение сред: dev-сборка и релиз не делят состояние.
//!
//! До разделения `tauri.conf.json` давал один `identifier` на обе сборки,
//! поэтому `app_data_dir()` у них совпадал. Dev накатывал в общую `app.db`
//! свежие миграции, после чего релизный бинарь падал на старте: sqlx видел
//! применённые версии, которых нет в его `migrations/`, `db::init` возвращал
//! Err, а `state::init(...)?` внутри `setup()` обрывал запуск — окно не
//! открывалось вовсе. По тому же корню сходились и другие пересечения:
//! `single_instance` ключуется по identifier (релиз, запущенный при живом dev,
//! просто фокусил чужое окно), общий `llama-server.pid`, общий `panic.log`,
//! общие записи keychain.
//!
//! Источник истины — профиль сборки: `tauri dev` собирает debug, `tauri build`
//! — release. `tauri.dev.conf.json` переопределяет `identifier` тем же
//! значением, что и [`DEV_IDENTIFIER`]; тест `dev_config_matches_dev_identifier`
//! ниже держит эти два файла в согласии, потому что разъехаться они могут
//! молча — приложение просто начнёт писать в третий каталог.

/// Bundle identifier релизной сборки. Совпадает с `tauri.conf.json`.
pub const PROD_IDENTIFIER: &str = "app.wotold.desktop";

/// Bundle identifier dev-сборки. Совпадает с `tauri.dev.conf.json`.
pub const DEV_IDENTIFIER: &str = "app.wotold.desktop.dev";

/// Identifier текущей сборки — каталог данных, `~/Library/Logs`, keychain.
///
/// Считается от профиля, а не от рантайм-флага: значение нужно `install_panic_hook`
/// до создания `AppHandle`, то есть до того, как конфиг Tauri вообще прочитан.
pub fn identifier() -> &'static str {
    if cfg!(debug_assertions) {
        DEV_IDENTIFIER
    } else {
        PROD_IDENTIFIER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_identifier(raw: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(raw).expect("конфиг — валидный JSON");
        value["identifier"]
            .as_str()
            .expect("в конфиге есть identifier")
            .to_string()
    }

    #[test]
    fn prod_config_matches_prod_identifier() {
        let raw = include_str!("../tauri.conf.json");
        assert_eq!(config_identifier(raw), PROD_IDENTIFIER);
    }

    #[test]
    fn dev_config_matches_dev_identifier() {
        let raw = include_str!("../tauri.dev.conf.json");
        assert_eq!(config_identifier(raw), DEV_IDENTIFIER);
    }

    /// Dev-каталог обязан быть отдельным, иначе разделение не состоялось.
    #[test]
    fn dev_and_prod_identifiers_differ() {
        assert_ne!(DEV_IDENTIFIER, PROD_IDENTIFIER);
        assert!(DEV_IDENTIFIER.starts_with(PROD_IDENTIFIER));
    }
}
