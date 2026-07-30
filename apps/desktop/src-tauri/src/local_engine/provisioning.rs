//! Обеспечение движка модулями: докачать обязательное, освободить место,
//! посчитать размеры для подписей кнопок.
//!
//! # Один вход вместо перечисления моделей на фронте
//!
//! Раньше фронт сам решал, что качать: собирал список из своей копии
//! preset-раскладки и дёргал `model_download` по одному id. Отсюда и брались
//! расхождения («скачали не всё») и невидимые прогрессы. Теперь качает бэкенд
//! по единому обязательному списку, а UI смотрит на снимок готовности.
//!
//! # Сеть только по явному действию
//!
//! Ни одна функция здесь не зовётся сама по себе на старте. Приложение
//! локальное, и разовое скачивание моделей — единственный сетевой поток;
//! запускать его без нажатия пользователя нельзя.

use std::path::Path;
use std::sync::OnceLock;

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::AppError;

use super::model_catalog::{lookup, ModelKind, MODEL_CATALOG};
use super::models::{self, ModelId, ModelStatus};
use super::preset::LocalEnginePreset;
use super::readiness;

/// Лок single-flight на всю докачку. Обычное нажатие «Скачать» вторым кликом
/// не должно поднимать вторую очередь: per-id мьютексы внутри `download`
/// защищают файлы, но не защищают от двойного прохода по списку.
fn ensure_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Докачать все недостающие обязательные модули.
///
/// Последовательно (не параллельно): модули весят гигабайты, и три
/// одновременных потока на бытовом канале дают только более рваный прогресс.
/// Прогресс идёт существующими событиями `model:progress` / `model:done` —
/// UI уже умеет их читать.
///
/// Ошибка одного модуля не останавливает остальные: пользователю полезнее
/// докачать что получилось и увидеть одну честную ошибку, чем встать на первом
/// же обрыве. Возвращается первая ошибка — как повод показать «Повторить».
pub async fn ensure_required(
    pool: &SqlitePool,
    app_data_dir: &Path,
    app: &AppHandle,
) -> Result<(), AppError> {
    // Ждём предыдущую докачку, а не отказываем сразу. Отказ означал бы «вызов
    // ничего не сделал», и вызывающий не мог отличить это от «качать было
    // нечего»; хуже — пока шла прошлая очередь, пользователь мог сменить
    // размер движка, и его моделей в ней не было вовсе. Дождавшись, мы
    // пересчитываем снимок и качаем то, что нужно сейчас.
    let _guard = ensure_lock().lock().await;

    let snapshot = readiness::compute(pool, app_data_dir).await?;
    if snapshot.preset.is_none() {
        return Err(AppError::Other(
            "local_engine_preset_not_set: сначала выберите размер движка".into(),
        ));
    }

    let mut first_error: Option<AppError> = None;
    for missing in &snapshot.missing {
        log::info!(
            "ensure_required: качаем {} ({} МБ)",
            missing.id,
            missing.bytes_total / 1024 / 1024
        );
        match models::download(app_data_dir, missing.id, Some(app)).await {
            Ok(_) => {
                // Успешная докачка снимает метку подмены: без этого файл,
                // однажды забракованный проверкой целостности, оставался
                // «подменённым» до следующего старта, и баннер не гас даже
                // после замены файла.
                super::model_integrity::mark_reverified(pool, app_data_dir, missing.id).await;
            }
            Err(e) => {
                log::warn!("ensure_required: {} не скачан: {e}", missing.id);
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }

    // Пересчитываем и поднимаем припаркованные, даже если качать было нечего:
    // движок мог стать готовым мимо этой команды (перекачали модуль поштучно,
    // вернули прежний размер), и тогда звонки так и висели бы до перезапуска.
    // Ошибку отдаём после подъёма — часть модулей могла лечь успешно.
    readiness::recompute_and_resume(app).await;

    if let Some(e) = first_error {
        return Err(e);
    }
    Ok(())
}

/// Какие модели можно удалить, не ломая текущую конфигурацию.
///
/// Чистая функция: решает по каталогу и активному размеру. Базовые модули и
/// модели активного размера не трогаются никогда — иначе «освободить место»
/// превращалось бы в «сломать движок».
pub(crate) fn removable_ids(preset: LocalEnginePreset) -> Vec<&'static str> {
    let required: Vec<&'static str> = readiness::required_ids(preset)
        .iter()
        .map(|id| id.as_str())
        .collect();
    MODEL_CATALOG
        .iter()
        .filter(|entry| matches!(entry.kind, ModelKind::Stt | ModelKind::Llm))
        .map(|entry| entry.id.as_str())
        .filter(|id| !required.contains(id))
        .collect()
}

/// Сколько байт занимают удаляемые модели прямо сейчас (только те, что
/// действительно лежат на диске). Для подписи кнопки «Освободить N».
pub async fn reclaimable_bytes(pool: &SqlitePool, app_data_dir: &Path) -> Result<u64, AppError> {
    let Some(preset) = readiness::active_preset(pool).await? else {
        return Ok(0);
    };
    let mut total = 0u64;
    for id in removable_ids(preset) {
        if let Ok(ModelStatus::Present { bytes_total, .. }) =
            models::check_status_fast(app_data_dir, id).await
        {
            total += bytes_total;
        }
    }
    Ok(total)
}

/// Удалить модели неактивных размеров. Возвращает освобождённые байты.
///
/// Авто-удаления при смене размера нет намеренно (R12-bis): решение об
/// удалении гигабайт принимает пользователь явным действием.
pub async fn free_space(
    pool: &SqlitePool,
    app_data_dir: &Path,
    app: &AppHandle,
) -> Result<u64, AppError> {
    let preset = readiness::active_preset(pool).await?.ok_or_else(|| {
        AppError::Other("local_engine_preset_not_set: сначала выберите размер движка".into())
    })?;
    let mut freed = 0u64;
    for id in removable_ids(preset) {
        let present_bytes = match models::check_status_fast(app_data_dir, id).await {
            Ok(ModelStatus::Present { bytes_total, .. }) => bytes_total,
            // Битый файл тоже место занимает — удаляем и его, но в отчёт
            // ставим фактический размер каталога, а не остаток докачки.
            Ok(ModelStatus::Corrupted { bytes_done, .. }) => bytes_done,
            _ => continue,
        };
        match models::delete(app_data_dir, id).await {
            Ok(()) => freed += present_bytes,
            Err(e) => log::warn!("free_space: {id} не удалён: {e}"),
        }
    }
    if freed > 0 {
        log::info!("free_space: освобождено {} МБ", freed / 1024 / 1024);
        readiness::recompute_and_emit(app);
    }
    Ok(freed)
}

/// Размеры одного варианта движка — для подписей «Скачать (~N ГБ)».
///
/// Считается по каталогу, а не по константам в UI: там были захардкожены
/// 1.2 / 2.4 / 5.5 ГБ, которые не совпадали с реальностью даже без базовых
/// модулей.
#[derive(Debug, Clone, Serialize)]
pub struct PresetSizeSpec {
    pub preset: LocalEnginePreset,
    pub whisper_model_id: &'static str,
    pub llm_model_id: &'static str,
    /// Модели самого размера.
    pub preset_bytes: u64,
    /// Обязательные базовые модули (одни и те же для всех размеров).
    pub base_bytes: u64,
    pub total_bytes: u64,
}

fn bytes_of(id: ModelId) -> u64 {
    lookup(id.as_str()).map(|e| e.size_bytes).unwrap_or(0)
}

/// Размеры всех трёх вариантов.
pub fn preset_specs() -> Vec<PresetSizeSpec> {
    let base_bytes: u64 = readiness::base_model_ids()
        .iter()
        .copied()
        .map(bytes_of)
        .sum();
    [
        LocalEnginePreset::Light,
        LocalEnginePreset::Balanced,
        LocalEnginePreset::Quality,
    ]
    .into_iter()
    .map(|preset| {
        let preset_bytes: u64 = preset
            .required_model_ids()
            .iter()
            .copied()
            .map(bytes_of)
            .sum();
        PresetSizeSpec {
            preset,
            whisper_model_id: preset.whisper_model_id().as_str(),
            llm_model_id: preset.llm_model_id().as_str(),
            preset_bytes,
            base_bytes,
            total_bytes: preset_bytes + base_bytes,
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removable_never_includes_active_or_base_models() {
        for preset in [
            LocalEnginePreset::Light,
            LocalEnginePreset::Balanced,
            LocalEnginePreset::Quality,
        ] {
            let removable = removable_ids(preset);
            for required in readiness::required_ids(preset) {
                assert!(
                    !removable.contains(&required.as_str()),
                    "{} обязателен при {:?}, удалять нельзя",
                    required.as_str(),
                    preset
                );
            }
            for base in readiness::base_model_ids() {
                assert!(
                    !removable.contains(&base.as_str()),
                    "{} — базовый модуль, удалять нельзя",
                    base.as_str()
                );
            }
        }
    }

    #[test]
    fn removable_is_exactly_the_other_two_sizes() {
        // Каталог: 3 whisper + 3 целевых LLM + draft. Draft базовый, поэтому
        // при любом размере удаляемых ровно 4 — по две модели двух других.
        let removable = removable_ids(LocalEnginePreset::Balanced);
        assert_eq!(removable.len(), 4, "получили {removable:?}");
        assert!(removable.contains(&"whisper-small"));
        assert!(removable.contains(&"whisper-large-v3"));
        assert!(removable.contains(&"qwen25-1_5b"));
        assert!(removable.contains(&"qwen25-7b"));
        assert!(!removable.contains(&"qwen25-0_5b"), "draft-модель базовая");
    }

    #[test]
    fn preset_sizes_grow_with_quality_and_include_base() {
        let specs = preset_specs();
        assert_eq!(specs.len(), 3);
        let base = specs[0].base_bytes;
        assert!(base > 0, "базовые модули не могут весить ноль");
        for s in &specs {
            assert_eq!(s.base_bytes, base, "база одинакова для всех размеров");
            assert_eq!(s.total_bytes, s.preset_bytes + s.base_bytes);
        }
        assert!(specs[0].total_bytes < specs[1].total_bytes);
        assert!(specs[1].total_bytes < specs[2].total_bytes);
    }

    #[test]
    fn every_preset_is_heavier_than_the_old_hardcoded_ui_number() {
        // Регрессия против возврата к константам 1.2 / 2.4 / 5.5 ГБ, которые
        // жили в онбординге: они занижали все три размера — база не
        // учитывалась вовсе, и кнопка обещала меньше, чем скачается.
        let specs = preset_specs();
        let gb = |b: u64| b as f64 / 1024f64.powi(3);
        for (spec, old_label) in specs.iter().zip([1.2, 2.4, 5.5]) {
            let real = gb(spec.total_bytes);
            assert!(
                real > old_label,
                "{:?}: реально {real:.2} ГБ, старая подпись обещала {old_label} ГБ",
                spec.preset
            );
        }
    }

    #[test]
    fn spec_serializes_with_string_ids() {
        let json = serde_json::to_value(&preset_specs()[0]).unwrap();
        assert_eq!(json["preset"], "light");
        assert_eq!(json["whisper_model_id"], "whisper-small");
        assert_eq!(json["llm_model_id"], "qwen25-1_5b");
        assert!(json["total_bytes"].as_u64().unwrap() > 0);
    }

    /// Регрессия: ранний выход «уже готов» пропускал подъём припаркованных
    /// звонков. Кнопка на странице такого звонка ведёт именно сюда, и если
    /// движок стал готов другим путём (поштучная перекачка, возврат прежнего
    /// размера), нажатие возвращало Ok, ничего не делая, — звонок висел
    /// сломанным до перезапуска приложения.
    #[test]
    fn ready_snapshot_must_not_short_circuit_before_resume() {
        let src = include_str!("provisioning.rs");
        let body = src
            .split_once("pub async fn ensure_required(")
            .expect("функция на месте")
            .1;
        let body = body.split_once("\n}\n").expect("конец функции").0;
        assert!(
            body.contains("recompute_and_resume"),
            "докачка обязана заканчиваться подъёмом припаркованных звонков"
        );
        assert!(
            !body.contains("snapshot.ready"),
            "ранний выход по готовому снимку обходил подъём припаркованных: \
             пустой список к докачке — не повод оставлять звонок сломанным"
        );
        assert!(
            !body.contains("try_lock"),
            "второй вызов обязан дождаться первого, а не возвращать Ok молча: \
             за время прошлой очереди мог смениться размер движка"
        );
    }

    #[tokio::test]
    async fn reclaimable_is_zero_without_a_preset() {
        let db = crate::db::test_support::fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(reclaimable_bytes(&db.pool, tmp.path()).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn reclaimable_counts_only_files_actually_on_disk() {
        let db = crate::db::test_support::fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        crate::db::set_setting(
            &db.pool,
            crate::local_engine::preset::SETTING_ACTIVE_PRESET,
            "light",
        )
        .await
        .unwrap();
        // Ничего не скачано — освобождать нечего.
        assert_eq!(reclaimable_bytes(&db.pool, tmp.path()).await.unwrap(), 0);

        // Кладём файл ровно каталожной длины: быстрый статус сверяет размер,
        // поэтому такой файл считается установленным. Разреженный (`set_len`),
        // иначе тест писал бы на диск полгигабайта.
        let entry = lookup("whisper-medium").unwrap();
        let path = models::model_path(tmp.path(), "whisper-medium");
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::File::create(&path)
            .await
            .unwrap()
            .set_len(entry.size_bytes)
            .await
            .unwrap();

        assert_eq!(
            reclaimable_bytes(&db.pool, tmp.path()).await.unwrap(),
            entry.size_bytes
        );
    }
}
