//! [security-scan W5 / H1] Фоновая проверка целостности моделей на старте.
//!
//! Находка ревью: SHA256 объявлен «единственной защитой от подмены
//! release-файла», но проверяется он **только на пути скачивания**. Дальше на
//! каждом прогоне пайплайна работает `check_status_fast` — сравнение размера.
//! Файл того же размера, подменённый локально после скачивания, считался
//! `Present` бесконечно и уходил прямиком в парсеры GGUF/ONNX, то есть в C++,
//! у которого своя история memory-corruption.
//!
//! Почему не проверять хэш на каждом прогоне: это 1.5–6 ГБ чтения перед каждым
//! звонком, ровно та причина, по которой быстрый путь и появился (см. коммент
//! в `models.rs`). Компромисс здесь: полный SHA считается **один раз на версию
//! файла** в фоне на старте, а результат кэшируется по «размер+mtime». Файл не
//! менялся — проверки нет; файл подменили — mtime/размер другие, и на
//! следующем старте это вскроется.
//!
//! Что делаем при несовпадении: **не удаляем**. Пользовательский файл в 6 ГБ
//! сносить по своей инициативе нельзя (вдруг это ручная подмена ради
//! эксперимента). Пишем `log::error!` и оставляем след в настройках, чтобы UI
//! мог предложить перекачать.

use std::path::Path;

use sqlx::SqlitePool;

use crate::{db, AppError};

use super::model_catalog::MODEL_CATALOG;
use super::models::{check_status, model_path, ModelStatus};

/// Ключ настройки с отметкой «эта версия файла проверена».
fn verified_key(id: &str) -> String {
    format!("local_engine.model_verified.{id}")
}

/// Отпечаток версии файла: размер и время модификации. Не криптография —
/// признак «файл тот же самый», чтобы не пересчитывать SHA на каждом старте.
async fn file_fingerprint(path: &Path) -> Option<String> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("{}:{}", meta.len(), mtime))
}

/// Проверить целостность всех моделей, лежащих на диске. Возвращает число
/// файлов, не прошедших проверку.
///
/// Вызывается фоном на старте — не блокирует показ окна.
pub async fn verify_present_models(pool: &SqlitePool, app_data_dir: &Path) -> usize {
    let mut failed = 0usize;
    for entry in MODEL_CATALOG.iter() {
        let id = entry.id.as_str();
        let path = model_path(app_data_dir, id);
        let Some(fingerprint) = file_fingerprint(&path).await else {
            continue; // модели нет на диске — проверять нечего
        };
        let key = verified_key(id);
        match db::get_setting(pool, &key).await {
            Ok(Some(seen)) if seen == fingerprint => continue,
            Ok(_) => {}
            Err(e) => {
                // Не смогли прочитать отметку — проверим ещё раз, это дешевле
                // ошибки в другую сторону.
                log::warn!("model_integrity: чтение отметки {id}: {e}");
            }
        }

        match check_status(app_data_dir, id).await {
            Ok(ModelStatus::Present { .. }) => {
                if let Err(e) = db::set_setting(pool, &key, &fingerprint).await {
                    log::warn!("model_integrity: отметку {id} не записали: {e}");
                }
                log::info!("model_integrity: {id} — SHA256 совпал");
            }
            Ok(ModelStatus::Corrupted { expected, got, .. }) => {
                failed += 1;
                // error, а не warn: это либо битый файл, либо подмена, и в
                // обоих случаях модель дальше идёт в нативный парсер.
                log::error!(
                    "model_integrity: {id} НЕ прошёл проверку (ожидали {expected}, получили {got}) — \
                     файл не удалён, перекачайте модель в Настройках → Движок"
                );
                let _ = db::set_setting(pool, &key, "FAILED").await;
            }
            Ok(_) => {}
            Err(e) => log::warn!("model_integrity: проверка {id} не удалась: {e}"),
        }
    }
    failed
}

/// [W5] Прошла ли модель последнюю проверку целостности. `false` только при
/// явном провале: «ещё не проверяли» — не повод блокировать работу.
pub async fn is_known_tampered(pool: &SqlitePool, id: &str) -> Result<bool, AppError> {
    Ok(db::get_setting(pool, &verified_key(id)).await?.as_deref() == Some("FAILED"))
}

/// Снять метку провала после успешной перекачки файла.
///
/// Без этого модель, однажды забракованная проверкой, оставалась
/// «подменённой» до следующего старта приложения: файл уже заменён, а гейт
/// готовности продолжал его отвергать, и баннер «не хватает софта» не гас.
/// Отпечаток берётся у нового файла — если он не читается, метку просто
/// удаляем, и проверка пересчитает её на следующем старте.
pub async fn mark_reverified(pool: &SqlitePool, app_data_dir: &Path, id: &str) {
    let key = verified_key(id);
    let value = file_fingerprint(&model_path(app_data_dir, id)).await;
    let write = match value {
        Some(fp) => db::set_setting(pool, &key, &fp).await,
        None => db::set_setting(pool, &key, "").await,
    };
    if let Err(e) = write {
        log::warn!("model_integrity: отметку {id} не обновили после перекачки: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    #[tokio::test]
    async fn missing_models_are_not_reported_as_failures() {
        // Пустой каталог на диске: проверять нечего, и это не провал.
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(verify_present_models(&db.pool, tmp.path()).await, 0);
    }

    #[tokio::test]
    async fn tampered_flag_is_off_until_a_check_actually_fails() {
        let db = fresh_db().await;
        // Ничего не проверяли — модель не считается подменённой.
        assert!(!is_known_tampered(&db.pool, "whisper-medium").await.unwrap());
        db::set_setting(
            &db.pool,
            "local_engine.model_verified.whisper-medium",
            "FAILED",
        )
        .await
        .unwrap();
        assert!(is_known_tampered(&db.pool, "whisper-medium").await.unwrap());
        // Успешная проверка (отпечаток) снимает флаг.
        db::set_setting(
            &db.pool,
            "local_engine.model_verified.whisper-medium",
            "123:456",
        )
        .await
        .unwrap();
        assert!(!is_known_tampered(&db.pool, "whisper-medium").await.unwrap());
    }

    #[tokio::test]
    async fn reverify_clears_the_failed_marker_after_a_redownload() {
        // Регрессия: успешная перекачка не снимала метку провала, и гейт
        // готовности отвергал уже заменённый файл до следующего старта.
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        db::set_setting(
            &db.pool,
            "local_engine.model_verified.pyannote-segmentation",
            "FAILED",
        )
        .await
        .unwrap();
        assert!(is_known_tampered(&db.pool, "pyannote-segmentation")
            .await
            .unwrap());

        let path = model_path(tmp.path(), "pyannote-segmentation");
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"fresh").await.unwrap();
        mark_reverified(&db.pool, tmp.path(), "pyannote-segmentation").await;

        assert!(!is_known_tampered(&db.pool, "pyannote-segmentation")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn fingerprint_changes_when_file_changes() {
        // Отпечаток обязан меняться при изменении файла — иначе подмену
        // «того же размера» не поймает даже следующий старт.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("m.bin");
        tokio::fs::write(&path, b"aaaa").await.unwrap();
        let first = file_fingerprint(&path).await.unwrap();

        tokio::fs::write(&path, b"bbbbbb").await.unwrap();
        let bigger = file_fingerprint(&path).await.unwrap();
        assert_ne!(first, bigger, "другой размер — другой отпечаток");

        assert!(file_fingerprint(&tmp.path().join("нет.bin"))
            .await
            .is_none());
    }
}
