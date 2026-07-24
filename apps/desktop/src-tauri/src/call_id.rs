//! [TD-05] Валидированный идентификатор звонка + общий path-guard.
//!
//! Проблема, которую закрывает модуль: `call_id` приходит из webview сырой
//! строкой и попадал прямо в `PathBuf::join`. `join("..")` уводит на уровень
//! выше (`calls/..` = `app_data_dir`), а `join("/etc")` — абсолютный путь —
//! **заменяет** базу целиком. В связке с `remove_dir_all` это стирало всю БД
//! и все записи; на чтении давало произвольный `recap.md`/`transcript.md`.
//!
//! Решение — «parse, don't validate»: сырая строка превращается в [`CallId`]
//! ровно один раз, на границе доверия, и дальше по коду ходит уже тип,
//! который невозможно сконструировать из мусора. `CallStore` принимает только
//! `&CallId`, поэтому забыть проверку в новом callsite нельзя — не скомпилится.
//!
//! [`ensure_path_under`] переехал сюда из `local_engine::llm` (там он был
//! `pub(super)` внутри macos-gated модуля, а `call_store` кросс-платформенный).

use std::path::{Component, Path};

use crate::AppError;

/// Идентификатор звонка, про который уже известно, что он безопасен как
/// сегмент пути: канонический lowercase-hyphenated UUID и ничего больше.
///
/// Конструируется либо [`CallId::parse`] (недоверенный вход — webview, MCP,
/// CLI-аргументы), либо [`CallId::from_db`] (строка, прочитанная из нашей же
/// БД). Оба пути дают один и тот же тип — разница только в том, кто отвечает
/// за валидность.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallId(String);

impl CallId {
    /// Разобрать недоверенную строку. Ошибка — если это не канонический UUID.
    ///
    /// Строгость намеренная: `Uuid::parse_str` сам по себе принимает и
    /// `urn:uuid:…`, и `{…}`-форму, и uppercase. Все они — валидные UUID, но
    /// как имя директории дают путь, которого на диске нет, то есть тихий
    /// «not found» вместо честной ошибки. Поэтому сверяем ещё и то, что вход
    /// побайтово совпадает с канонической записью.
    pub fn parse(raw: &str) -> Result<Self, AppError> {
        let parsed = uuid::Uuid::parse_str(raw)
            .map_err(|_| AppError::NotFound(format!("invalid call id: {raw}")))?;
        if parsed.to_string() != raw {
            return Err(AppError::NotFound(format!(
                "call id must be canonical lowercase-hyphenated uuid: {raw}"
            )));
        }
        Ok(Self(raw.to_string()))
    }

    /// Обернуть id, прочитанный из нашей БД.
    ///
    /// Инфаллибельно — в `calls.id` пишет только `insert_recording`
    /// (`Uuid::new_v4().to_string()`), других источников нет, и ни одна
    /// миграция строк туда не вставляет. Не вызывать на данных из webview:
    /// для них есть [`CallId::parse`].
    pub(crate) fn from_db(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// [Security M-3] Defense-in-depth: проверить что path не содержит `..`
/// сегментов И начинается с разрешённого prefix. Capability validator
/// `^[A-Za-z0-9._/\-]+$` пропускает `../../etc/passwd` — это последняя
/// граница. Канонических `.canonicalize()` НЕ делаем (path может не
/// существовать на момент проверки — например, output stem whisper-cli).
///
/// Returns Err если найден `..` сегмент или prefix не совпадает.
pub fn ensure_path_under(path: &Path, allowed_prefix: &Path) -> Result<(), String> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("path {} contains '..' segment", path.display()));
    }
    if !path.starts_with(allowed_prefix) {
        return Err(format!(
            "path {} not under prefix {}",
            path.display(),
            allowed_prefix.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Канонический v4 для тестов, которым нужен валидный id.
    // Содержит hex-буквы намеренно: на чисто цифровом uuid `to_uppercase()`
    // равен оригиналу, и тест на uppercase-форму ничего не проверял бы.
    pub const TEST_UUID: &str = "a1b2c3d4-e5f6-4a1b-8c2d-3e4f5a6b7c8d";

    #[test]
    fn parse_accepts_canonical_v4() {
        let id = CallId::parse(TEST_UUID).expect("канонический uuid");
        assert_eq!(id.as_str(), TEST_UUID);
        assert_eq!(id.to_string(), TEST_UUID);
    }

    #[test]
    fn parse_accepts_real_generated_id() {
        let generated = uuid::Uuid::new_v4().to_string();
        assert!(CallId::parse(&generated).is_ok(), "id из insert_recording");
    }

    #[test]
    fn parse_rejects_traversal_segments() {
        for raw in ["..", "../..", "a/../../b", "calls/../../etc"] {
            assert!(
                CallId::parse(raw).is_err(),
                "{raw} должен быть отклонён — это path traversal"
            );
        }
    }

    #[test]
    fn parse_rejects_absolute_path() {
        // `join("/etc")` заменяет базу целиком — самый опасный случай.
        assert!(CallId::parse("/etc").is_err());
        assert!(CallId::parse("/etc/passwd").is_err());
    }

    #[test]
    fn parse_rejects_empty_and_plain_strings() {
        for raw in ["", " ", "c1", "abc", "ghost", "call-a"] {
            assert!(CallId::parse(raw).is_err(), "{raw} не uuid");
        }
    }

    #[test]
    fn parse_rejects_noncanonical_uuid_forms() {
        // Все три — валидные UUID для `parse_str`, но как имя директории дают
        // несуществующий путь. Отклоняем, чтобы не получить тихий not-found.
        let simple = TEST_UUID.replace('-', "");
        assert!(CallId::parse(&simple).is_err(), "simple-форма без дефисов");
        assert!(
            CallId::parse(&TEST_UUID.to_uppercase()).is_err(),
            "uppercase"
        );
        assert!(
            CallId::parse(&format!("urn:uuid:{TEST_UUID}")).is_err(),
            "urn-форма"
        );
        assert!(
            CallId::parse(&format!("{{{TEST_UUID}}}")).is_err(),
            "braced"
        );
    }

    #[test]
    fn parse_rejects_uuid_with_path_suffix() {
        assert!(CallId::parse(&format!("{TEST_UUID}/../../etc")).is_err());
        assert!(CallId::parse(&format!("{TEST_UUID}\0")).is_err());
    }

    #[test]
    fn from_db_wraps_without_validation() {
        // Контракт: доверенный конструктор не проверяет — это осознанно.
        assert_eq!(CallId::from_db("c1").as_str(), "c1");
    }

    // ── ensure_path_under ───────────────────────────────────────────────

    #[test]
    fn ensure_path_under_accepts_path_inside_prefix() {
        assert!(ensure_path_under(
            Path::new("/data/local_engine/models/whisper-small.bin"),
            Path::new("/data/local_engine"),
        )
        .is_ok());
    }

    #[test]
    fn ensure_path_under_rejects_dotdot_segment() {
        let err = ensure_path_under(
            Path::new("/data/local_engine/../etc/passwd"),
            Path::new("/data/local_engine"),
        )
        .expect_err("`..` сегмент → Err");
        assert!(err.contains("'..' segment"));
    }

    #[test]
    fn ensure_path_under_rejects_path_outside_prefix() {
        let err = ensure_path_under(Path::new("/etc/passwd"), Path::new("/data/local_engine"))
            .expect_err("вне prefix → Err");
        assert!(err.contains("not under prefix"));
    }

    #[test]
    fn ensure_path_under_handles_relative_paths_safely() {
        // Relative paths не starts_with absolute prefix — должны быть отклонены.
        let err = ensure_path_under(
            Path::new("models/whisper.bin"),
            Path::new("/data/local_engine"),
        )
        .expect_err("relative → Err");
        assert!(err.contains("not under prefix"));
    }

    #[test]
    fn parsed_id_always_lands_under_calls_root() {
        // Инвариант связки CallId + ensure_path_under: что бы ни распарсилось,
        // результат join остаётся под корнем.
        let root = PathBuf::from("/data/calls");
        let id = CallId::parse(TEST_UUID).unwrap();
        let dir = root.join(id.as_str());
        assert!(ensure_path_under(&dir, &root).is_ok());
        assert_eq!(dir, PathBuf::from(format!("/data/calls/{TEST_UUID}")));
    }
}
