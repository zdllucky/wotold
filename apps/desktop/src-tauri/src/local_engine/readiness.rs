//! Готовность локального движка: что обязано лежать на диске, чтобы звонок
//! вообще можно было обработать.
//!
//! # Зачем отдельный модуль
//!
//! Список обязательных моделей раньше существовал в трёх местах:
//! `preset::required_model_ids` (помечен dead_code и никем не вызывался),
//! `PRESET_TO_MODELS` в настройках и `PRESET_MODELS` в онбординге. Три копии
//! расходились молча, а расхождение выглядит для пользователя как «скачал, а
//! оно пишет что модели нет». Здесь единственный источник истины, обе UI-копии
//! теперь спрашивают его через команды.
//!
//! # Строгость
//!
//! Список один, тиров нет: не хватает любого модуля — обработка стоит целиком.
//! Раньше половина модулей молча деградировала (система в один голос,
//! поиск ассистента без семантики), и пользователь узнавал об этом по
//! результату, а не по честному «не хватает софта, скачать?».
//!
//! # R13
//!
//! Готовность считается **только** по наличию файлов и метке подмены. Железо
//! не участвует: слабая машина обязана работать (на ней просто Light), и
//! hard-gate по железу паспорт запрещает прямым текстом.

use std::path::Path;

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::{db, AppError};

use super::model_catalog::lookup;
use super::models::{self, ModelId, ModelStatus};
use super::preset::{LocalEnginePreset, SETTING_ACTIVE_PRESET};

/// Маркер ошибки «модулей не хватает». Фронтенд матчит префикс и показывает
/// кнопку скачивания вместо голого текста ошибки.
pub const NOT_READY_MARKER: &str = "local_engine_not_ready";

/// Модули, обязательные при любом пресете.
///
/// - `pyannote-segmentation` + `voice-embedder` — диаризация и кластеры
///   спикеров; без них дорожка склеивается в одного говорящего.
/// - `silero-vad-v5` — обрезка тишины до энкодера whisper (заметно быстрее на
///   паузах). До этой ревизии не скачивался вообще ничем, хотя код его ждал.
/// - `e5-small-qint8` + `e5-small-tokenizer` — семантический поиск ассистента.
/// - `qwen25-0_5b` — draft-модель спекулятивного декодирования саммари.
pub fn base_model_ids() -> [ModelId; 6] {
    [
        ModelId::PYANNOTE_SEGMENTATION,
        ModelId::VOICE_EMBEDDER,
        ModelId::SILERO_VAD,
        ModelId::E5_SMALL_QINT8,
        ModelId::E5_TOKENIZER,
        ModelId::QWEN25_0_5B,
    ]
}

/// Полный обязательный список: модели пресета + базовые модули.
pub fn required_ids(preset: LocalEnginePreset) -> Vec<ModelId> {
    let mut ids = preset.required_model_ids().to_vec();
    ids.extend_from_slice(&base_model_ids());
    ids
}

/// Почему модуль не годен к работе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingState {
    /// Файла нет.
    Absent,
    /// Файл есть, но размер не совпадает с каталогом (обрубок докачки).
    Corrupted,
    /// Файл не прошёл проверку SHA256 на старте — в нативный парсер не отдаём.
    Tampered,
}

/// Недостающий модуль. Без display_name намеренно: человекочитаемые названия
/// живут в `utils/modelLabel.ts`, чтобы бренды моделей не протекали в UI.
#[derive(Debug, Clone, Serialize)]
pub struct MissingModel {
    pub id: &'static str,
    pub bytes_total: u64,
    pub state: MissingState,
}

/// Снимок готовности движка. Контракт —
/// `packages/contracts/src/local-engine.ts::LocalEngineReadiness`.
#[derive(Debug, Clone, Serialize)]
pub struct LocalEngineReadiness {
    pub ready: bool,
    /// `None` — пресет ещё не выбран; тогда качать нечего до выбора размера.
    pub preset: Option<LocalEnginePreset>,
    pub missing: Vec<MissingModel>,
    pub missing_bytes_total: u64,
}

/// Чистая часть решения: по статусам моделей собрать снимок готовности.
/// Тестируется без диска и без базы.
pub(crate) fn readiness_from_statuses(
    preset: Option<LocalEnginePreset>,
    statuses: &[(ModelId, ModelStatus, bool)],
) -> LocalEngineReadiness {
    let Some(preset) = preset else {
        return LocalEngineReadiness {
            ready: false,
            preset: None,
            missing: Vec::new(),
            missing_bytes_total: 0,
        };
    };
    let missing: Vec<MissingModel> = statuses
        .iter()
        .filter_map(|(id, status, tampered)| {
            let state = if *tampered {
                MissingState::Tampered
            } else {
                match status {
                    ModelStatus::Present { .. } => return None,
                    ModelStatus::Absent { .. } => MissingState::Absent,
                    ModelStatus::Corrupted { .. } => MissingState::Corrupted,
                }
            };
            let bytes_total = match status {
                ModelStatus::Absent { bytes_total, .. }
                | ModelStatus::Present { bytes_total, .. }
                | ModelStatus::Corrupted { bytes_total, .. } => *bytes_total,
            };
            Some(MissingModel {
                id: id.as_str(),
                bytes_total,
                state,
            })
        })
        .collect();
    let missing_bytes_total = missing.iter().map(|m| m.bytes_total).sum();
    LocalEngineReadiness {
        ready: missing.is_empty(),
        preset: Some(preset),
        missing,
        missing_bytes_total,
    }
}

/// Прочитать пресет из настроек.
pub async fn active_preset(pool: &SqlitePool) -> Result<Option<LocalEnginePreset>, AppError> {
    Ok(db::get_setting(pool, SETTING_ACTIVE_PRESET)
        .await?
        .as_deref()
        .and_then(LocalEnginePreset::from_str))
}

/// Снимок готовности: пресет из настроек + быстрый статус каждого модуля.
///
/// Быстрый путь (сверка размера) намеренно: полный SHA по 6 ГБ перед каждым
/// звонком и был причиной появления быстрого пути. Подмену того же размера
/// ловит стартовая проверка целостности, её вердикт учитывается здесь через
/// `is_known_tampered`.
pub async fn compute(
    pool: &SqlitePool,
    app_data_dir: &Path,
) -> Result<LocalEngineReadiness, AppError> {
    let preset = active_preset(pool).await?;
    let Some(p) = preset else {
        return Ok(readiness_from_statuses(None, &[]));
    };
    let mut statuses = Vec::with_capacity(8);
    for id in required_ids(p) {
        let status = models::check_status_fast(app_data_dir, id.as_str()).await?;
        let tampered = super::model_integrity::is_known_tampered(pool, id.as_str()).await?;
        statuses.push((id, status, tampered));
    }
    Ok(readiness_from_statuses(Some(p), &statuses))
}

/// Гейт пайплайна. Единственная проверка моделей перед обработкой — до этого
/// её копии жили в `prepare_local_run` и `build_local_llm_provider` с
/// одинаковыми сообщениями и разной зрелостью.
pub async fn assert_ready(pool: &SqlitePool, app_data_dir: &Path) -> Result<(), AppError> {
    let r = compute(pool, app_data_dir).await?;
    if r.ready {
        return Ok(());
    }
    if r.preset.is_none() {
        return Err(AppError::Other(
            "local_engine_preset_not_set: выберите Light/Balanced/Quality в Настройках → Обработка"
                .into(),
        ));
    }
    let tampered: Vec<&str> = r
        .missing
        .iter()
        .filter(|m| m.state == MissingState::Tampered)
        .map(|m| m.id)
        .collect();
    let ids: Vec<&str> = r.missing.iter().map(|m| m.id).collect();
    if !tampered.is_empty() {
        return Err(AppError::Other(format!(
            "{NOT_READY_MARKER}: файлы модулей не прошли проверку SHA256 ({}) — перекачайте их",
            tampered.join(", ")
        )));
    }
    Err(AppError::Other(format!(
        "{NOT_READY_MARKER}: не хватает модулей: {}",
        ids.join(", ")
    )))
}

/// Пересчитать готовность и разослать снимок в UI. Fire-and-forget: баннер
/// дополнительно тянет снимок командой при монтировании, поэтому пропущенное
/// событие означает «устарело до следующего действия», а не «врёт навсегда».
pub fn recompute_and_emit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = recompute_inner(&app, /* resume_parked */ false).await;
    });
}

/// То же плюс подъём припаркованных звонков, если движок стал готов.
///
/// Отдельная функция, а не флаг внутри `recompute_and_emit`: подъём — это
/// действие, а не уведомление, и звать его надо ровно там, где готовность
/// могла смениться на «готов» (докачка, возврат прежнего размера, старт).
/// Раньше подъём висел на одном-единственном пути — успешном `ensure_required`,
/// — и звонок, дождавшийся моделей другим путём, оставался failed до
/// перезапуска приложения.
pub async fn recompute_and_resume(app: &AppHandle) {
    let _ = recompute_inner(app, /* resume_parked */ true).await;
}

async fn recompute_inner(app: &AppHandle, resume_parked: bool) -> Option<LocalEngineReadiness> {
    let (pool, app_data_dir) = {
        let state = tauri::Manager::state::<crate::state::AppState>(app);
        (state.db.clone(), state.app_data_dir.clone())
    };
    let snapshot = match compute(&pool, &app_data_dir).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("readiness: пересчёт не удался: {e}");
            return None;
        }
    };
    crate::events::EventBus::new(Some(app)).readiness_changed(&snapshot);
    if resume_parked && snapshot.ready {
        crate::commands::resume_parked_calls(app.clone()).await;
    }
    Some(snapshot)
}

/// Суммарный размер модулей, которые ещё не скачаны. Для подписи кнопки.
#[allow(dead_code)]
pub fn missing_bytes(r: &LocalEngineReadiness) -> u64 {
    r.missing_bytes_total
}

/// Есть ли запись в каталоге для каждого базового модуля. Защита от опечатки
/// в `base_model_ids` — без неё `compute` падал бы на «unknown model id».
#[allow(dead_code)]
pub fn base_ids_are_known() -> bool {
    base_model_ids()
        .iter()
        .all(|id| lookup(id.as_str()).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn present(id: ModelId, bytes: u64) -> (ModelId, ModelStatus, bool) {
        (
            id,
            ModelStatus::Present {
                id: id.as_str().to_string(),
                bytes_total: bytes,
            },
            false,
        )
    }

    fn absent(id: ModelId, bytes: u64) -> (ModelId, ModelStatus, bool) {
        (
            id,
            ModelStatus::Absent {
                id: id.as_str().to_string(),
                bytes_total: bytes,
            },
            false,
        )
    }

    fn corrupted(id: ModelId, bytes: u64) -> (ModelId, ModelStatus, bool) {
        (
            id,
            ModelStatus::Corrupted {
                id: id.as_str().to_string(),
                bytes_done: 1,
                bytes_total: bytes,
                expected: "a".into(),
                got: "b".into(),
            },
            false,
        )
    }

    fn all_present(preset: LocalEnginePreset) -> Vec<(ModelId, ModelStatus, bool)> {
        required_ids(preset)
            .into_iter()
            .map(|id| present(id, 100))
            .collect()
    }

    #[test]
    fn required_list_is_preset_models_plus_base() {
        let ids = required_ids(LocalEnginePreset::Balanced);
        assert_eq!(ids.len(), 8, "2 модели пресета + 6 базовых");
        assert!(ids.contains(&ModelId::WHISPER_MEDIUM));
        assert!(ids.contains(&ModelId::QWEN25_3B));
        for base in base_model_ids() {
            assert!(ids.contains(&base), "{} обязателен", base.as_str());
        }
        let unique: std::collections::HashSet<&str> = ids.iter().map(|i| i.as_str()).collect();
        assert_eq!(unique.len(), ids.len(), "дубли в обязательном списке");
    }

    #[test]
    fn base_modules_all_exist_in_catalog() {
        assert!(base_ids_are_known());
    }

    #[test]
    fn all_present_is_ready() {
        let r = readiness_from_statuses(
            Some(LocalEnginePreset::Light),
            &all_present(LocalEnginePreset::Light),
        );
        assert!(r.ready);
        assert!(r.missing.is_empty());
        assert_eq!(r.missing_bytes_total, 0);
    }

    #[test]
    fn missing_base_module_blocks_even_when_preset_models_are_present() {
        // Строгость без тиров: draft-модель саммари весит 400 МБ и раньше
        // считалась необязательной — теперь её отсутствие тоже стоп.
        let mut statuses = all_present(LocalEnginePreset::Light);
        let idx = statuses
            .iter()
            .position(|(id, _, _)| *id == ModelId::QWEN25_0_5B)
            .unwrap();
        statuses[idx] = absent(ModelId::QWEN25_0_5B, 397_808_192);

        let r = readiness_from_statuses(Some(LocalEnginePreset::Light), &statuses);
        assert!(!r.ready);
        assert_eq!(r.missing.len(), 1);
        assert_eq!(r.missing[0].id, "qwen25-0_5b");
        assert_eq!(r.missing[0].state, MissingState::Absent);
        assert_eq!(r.missing_bytes_total, 397_808_192);
    }

    #[test]
    fn corrupted_counts_as_missing_and_keeps_full_size() {
        // Обрубок докачки: качать надо целиком, поэтому в сумму идёт полный
        // размер файла, а не остаток.
        let statuses = vec![corrupted(ModelId::WHISPER_SMALL, 190_085_487)];
        let r = readiness_from_statuses(Some(LocalEnginePreset::Light), &statuses);
        assert!(!r.ready);
        assert_eq!(r.missing[0].state, MissingState::Corrupted);
        assert_eq!(r.missing_bytes_total, 190_085_487);
    }

    #[test]
    fn tampered_wins_over_present_status() {
        // Файл на месте и нужного размера, но стартовая проверка SHA его
        // забраковала — в нативный парсер такой файл не отдаём.
        let mut statuses = all_present(LocalEnginePreset::Quality);
        statuses[0].2 = true;
        let r = readiness_from_statuses(Some(LocalEnginePreset::Quality), &statuses);
        assert!(!r.ready);
        assert_eq!(r.missing.len(), 1);
        assert_eq!(r.missing[0].state, MissingState::Tampered);
    }

    #[test]
    fn no_preset_is_not_ready_and_lists_nothing_to_download() {
        let r = readiness_from_statuses(None, &all_present(LocalEnginePreset::Light));
        assert!(!r.ready);
        assert!(r.preset.is_none());
        assert!(
            r.missing.is_empty(),
            "до выбора размера качать нечего — UI ведёт в настройки"
        );
        assert_eq!(r.missing_bytes_total, 0);
    }

    #[test]
    fn missing_bytes_sums_every_gap() {
        let statuses = vec![
            absent(ModelId::WHISPER_SMALL, 10),
            absent(ModelId::SILERO_VAD, 5),
            present(ModelId::E5_TOKENIZER, 999),
        ];
        let r = readiness_from_statuses(Some(LocalEnginePreset::Light), &statuses);
        assert_eq!(r.missing_bytes_total, 15);
        assert_eq!(missing_bytes(&r), 15);
    }

    #[test]
    fn readiness_serializes_to_the_frontend_shape() {
        let r = readiness_from_statuses(
            Some(LocalEnginePreset::Light),
            &[absent(ModelId::SILERO_VAD, 885_098)],
        );
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["ready"], false);
        assert_eq!(json["preset"], "light");
        assert_eq!(json["missing_bytes_total"], 885_098);
        assert_eq!(json["missing"][0]["id"], "silero-vad-v5");
        assert_eq!(json["missing"][0]["state"], "absent");
    }

    #[tokio::test]
    async fn assert_ready_reports_preset_first() {
        let db = crate::db::test_support::fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        let err = assert_ready(&db.pool, tmp.path()).await.unwrap_err();
        assert!(
            err.to_string().contains("local_engine_preset_not_set"),
            "без пресета сообщаем про выбор размера, а не про модули: {err}"
        );
    }

    #[tokio::test]
    async fn assert_ready_lists_missing_modules_when_preset_is_set() {
        let db = crate::db::test_support::fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        db::set_setting(&db.pool, SETTING_ACTIVE_PRESET, "light")
            .await
            .unwrap();

        let err = assert_ready(&db.pool, tmp.path()).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(NOT_READY_MARKER), "{msg}");
        assert!(msg.contains("whisper-small"), "{msg}");
        assert!(msg.contains("voice-embedder"), "{msg}");
    }

    #[tokio::test]
    async fn assert_ready_calls_out_tampered_files_separately() {
        let db = crate::db::test_support::fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        db::set_setting(&db.pool, SETTING_ACTIVE_PRESET, "light")
            .await
            .unwrap();
        db::set_setting(
            &db.pool,
            "local_engine.model_verified.whisper-small",
            "FAILED",
        )
        .await
        .unwrap();

        let err = assert_ready(&db.pool, tmp.path()).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("SHA256"),
            "подмена требует другого текста: {msg}"
        );
        assert!(msg.contains("whisper-small"), "{msg}");
    }

    #[tokio::test]
    async fn compute_ignores_hardware_entirely() {
        // R13: слабое железо не блокирует движок. Готовность считается без
        // единого обращения к hw_probe — здесь это видно по тому, что
        // снимок собирается на пустом каталоге без отчёта о железе в базе.
        let db = crate::db::test_support::fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        db::set_setting(&db.pool, SETTING_ACTIVE_PRESET, "quality")
            .await
            .unwrap();
        let r = compute(&db.pool, tmp.path()).await.unwrap();
        assert_eq!(r.preset, Some(LocalEnginePreset::Quality));
        assert_eq!(r.missing.len(), 8, "все обязательные модули отсутствуют");
    }
}
