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

    /// Тело plist без XML-комментариев.
    ///
    /// Проверять голым `contains` нельзя: комментарий в шапке `Info.plist`
    /// сам объясняет, зачем нужен `NSMicrophoneUsageDescription`, и подстрока
    /// находилась в нём даже после удаления настоящего ключа — сторож
    /// оставался зелёным ровно в том случае, ради которого написан.
    fn plist_body(raw: &str) -> String {
        let mut out = String::new();
        let mut rest = raw;
        while let Some(start) = rest.find("<!--") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 4..];
            match after.find("-->") {
                Some(end) => rest = &after[end + 3..],
                None => return out,
            }
        }
        out.push_str(rest);
        out
    }

    /// [perm-usage] Без этого ключа macOS убивает процесс, дотянувшийся до
    /// микрофона (SIGABRT, namespace TCC): TCC атрибутирует запрос сайдкара
    /// ответственному процессу, то есть приложению. Ключ легко потерять при
    /// правке бандла — сломается при этом не сборка, а запись у пользователя.
    #[test]
    fn plist_declares_microphone_usage() {
        let body = plist_body(include_str!("../Info.plist"));
        assert!(body.contains("<key>NSMicrophoneUsageDescription</key>"));
        let (_, after) = body
            .split_once("<key>NSMicrophoneUsageDescription</key>")
            .expect("ключ есть");
        let (_, value) = after.split_once("<string>").expect("у ключа есть значение");
        let (text, _) = value.split_once("</string>").expect("значение закрыто");
        assert!(
            !text.trim().is_empty(),
            "пустое usage-описание macOS не примет"
        );
    }

    /// Сам сторож обязан ловить пропажу ключа — иначе он декоративный.
    #[test]
    fn plist_body_ignores_comments() {
        let only_comment = "<!-- NSMicrophoneUsageDescription упомянут в тексте -->\n<dict/>";
        assert!(!plist_body(only_comment).contains("NSMicrophoneUsageDescription"));
    }

    /// Этот plist только домешивает ключи — и в бандл релиза, и в секцию
    /// `__TEXT,__info_plist` debug-бинаря. Появится тут `CFBundleIdentifier` —
    /// он перебьёт сгенерированный бандлером, и релиз уедет в чужой каталог
    /// данных, ровно тем механизмом, от которого уводит разделение сред.
    #[test]
    fn plist_does_not_override_identifier() {
        let raw = include_str!("../Info.plist");
        assert!(!raw.contains("<key>CFBundleIdentifier</key>"));
    }
}
