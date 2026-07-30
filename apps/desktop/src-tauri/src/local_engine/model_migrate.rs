//! Переезд файлов моделей между раскладками на диске.
//!
//! Голосовой эмбеддер (WeSpeaker) жил в собственной качалке `voice_model.rs`
//! по пути `$APP_DATA/models/embedder.onnx`. После переноса записи в
//! `MODEL_CATALOG` канонический путь — `models::model_path(dir,
//! "voice-embedder")`, то есть `$APP_DATA/local_engine/models/voice-embedder.bin`.
//! Без переноса апгрейд выглядел бы как «модуль пропал»: файл на диске есть,
//! но каталог смотрит в другое место и требует повторные 26 MB.

use std::path::Path;

use super::models::{self, ModelId};

/// Старое расположение эмбеддера (до переноса в каталог).
fn legacy_voice_embedder_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("models").join("embedder.onnx")
}

/// Перенести легаси-файл эмбеддера на каталожный путь.
///
/// Идемпотентна: если нового файла уже нет, а старый есть — переносим; если
/// новый на месте — только убираем легаси-обрубок докачки. SHA не проверяем:
/// это делает штатная стартовая проверка целостности, уже умеющая помечать
/// файл как tampered. Ошибку не поднимаем выше — приложение обязано
/// подняться, а не-перенесённый файл означает лишь повторное скачивание.
pub fn migrate_legacy_voice_embedder(app_data_dir: &Path) {
    let legacy = legacy_voice_embedder_path(app_data_dir);
    let legacy_partial = legacy.with_extension("onnx.partial");
    let dest = models::model_path(app_data_dir, ModelId::VOICE_EMBEDDER.as_str());

    if dest.exists() {
        if legacy.exists() || legacy_partial.exists() {
            let _ = std::fs::remove_file(&legacy);
            let _ = std::fs::remove_file(&legacy_partial);
            log::info!("migrate_voice_embedder: каталожный файл на месте, легаси-копия удалена");
        }
        return;
    }
    // Обрубок прежней качалки бесполезен: она не умела докачивать с середины.
    let _ = std::fs::remove_file(&legacy_partial);
    if !legacy.exists() {
        return;
    }
    if let Some(parent) = dest.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("migrate_voice_embedder: mkdir {}: {e}", parent.display());
            return;
        }
    }
    match std::fs::rename(&legacy, &dest) {
        Ok(()) => log::info!(
            "migrate_voice_embedder: {} → {}",
            legacy.display(),
            dest.display()
        ),
        Err(e) => log::warn!(
            "migrate_voice_embedder: rename {} → {}: {e} — модуль будет скачан заново",
            legacy.display(),
            dest.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn dest_of(dir: &Path) -> std::path::PathBuf {
        models::model_path(dir, ModelId::VOICE_EMBEDDER.as_str())
    }

    #[test]
    fn moves_legacy_file_to_catalog_path() {
        let dir = tempdir().unwrap();
        let legacy = legacy_voice_embedder_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"onnx-bytes").unwrap();

        migrate_legacy_voice_embedder(dir.path());

        assert!(!legacy.exists(), "легаси-файл должен исчезнуть");
        assert_eq!(std::fs::read(dest_of(dir.path())).unwrap(), b"onnx-bytes");
    }

    #[test]
    fn is_idempotent_and_second_run_keeps_catalog_file() {
        let dir = tempdir().unwrap();
        let legacy = legacy_voice_embedder_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"onnx-bytes").unwrap();

        migrate_legacy_voice_embedder(dir.path());
        migrate_legacy_voice_embedder(dir.path());

        assert_eq!(std::fs::read(dest_of(dir.path())).unwrap(), b"onnx-bytes");
    }

    #[test]
    fn catalog_file_wins_and_legacy_copy_is_removed() {
        let dir = tempdir().unwrap();
        let legacy = legacy_voice_embedder_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"stale").unwrap();
        let dest = dest_of(dir.path());
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"fresh").unwrap();

        migrate_legacy_voice_embedder(dir.path());

        assert!(!legacy.exists(), "легаси-копия занимает место зря");
        assert_eq!(std::fs::read(&dest).unwrap(), b"fresh");
    }

    #[test]
    fn legacy_partial_is_dropped_without_creating_dest() {
        let dir = tempdir().unwrap();
        let legacy = legacy_voice_embedder_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let partial = legacy.with_extension("onnx.partial");
        std::fs::write(&partial, b"half").unwrap();

        migrate_legacy_voice_embedder(dir.path());

        assert!(!partial.exists(), "обрубок прежней качалки не докачиваем");
        assert!(!dest_of(dir.path()).exists());
    }

    #[test]
    fn no_files_at_all_is_a_noop() {
        let dir = tempdir().unwrap();
        migrate_legacy_voice_embedder(dir.path());
        assert!(!dest_of(dir.path()).exists());
    }
}
