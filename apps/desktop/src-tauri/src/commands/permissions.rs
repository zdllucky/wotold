//! Tauri-команды разрешений macOS.
//!
//! Жили в `recording.rs` рядом со стартом записи и уехали сюда, когда к ним
//! добавился сброс TCC: у файла записи своя когезия, и общий счётчик строк
//! упёрся в лимит модуля (правило 8).
//!
//! [perm-usage] Статус читает и запрашивает Swift-сайдкар — он же пишет звук.
//! TCC при этом атрибутирует запрос **ответственному процессу**, то есть
//! приложению, поэтому usage-описания обязаны лежать в `Info.plist` бандла
//! (см. `src-tauri/Info.plist` и `app_env.rs`), а не только у сайдкара.

use tauri::{AppHandle, State};

use crate::{
    audio::permissions::{self, PermissionsStatus},
    state::AppState,
    AppError,
};

#[tauri::command]
pub async fn get_audio_permissions(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PermissionsStatus, AppError> {
    permissions::check(&app, &state.db).await
}

#[tauri::command]
pub async fn request_audio_permissions(
    app: AppHandle,
    state: State<'_, AppState>,
    target: String,
) -> Result<PermissionsStatus, AppError> {
    permissions::request(&app, &state.db, &target).await
}

/// [perm-usage] Сбрасывает TCC-запись приложения для одного разрешения.
///
/// macOS привязывает выданный доступ к подписи бинаря, а сборки подписаны
/// ad-hoc (R6, без Developer ID) — cdhash меняется на каждой сборке. После
/// обновления тумблер в Системных настройках выглядит включённым, а
/// приложение получает «отказано», и никакой «Запросить» это не чинит:
/// диалог для уже принятого решения macOS второй раз не показывает.
/// Сброс возвращает разрешение в состояние «не спрашивали».
///
/// Идентификатор берём из профиля сборки, чтобы dev не сбрасывал релизу и
/// наоборот. `pane` в аргументы процесса сырым не попадает — только через
/// whitelist [`tcc_service`].
///
/// Ожидание `tccutil` уходит в `spawn_blocking` (правило 5): синхронная
/// Tauri-команда выполняется на главном потоке, и блокирующий `status()`
/// подвесил бы окно на всё время работы утилиты.
///
/// Ограничение dev-сборки: `tauri dev` запускает голый бинарь без бандла,
/// у него нет `CFBundleIdentifier`, и TCC ведёт его по пути. Сброс по
/// идентификатору `app.wotold.desktop.dev` в такой сборке ничего не находит,
/// а `tccutil` всё равно выходит с нулём — отличить нечем. Проверять кнопку
/// нужно на собранном `.app`.
///
/// Абсолютный путь к `tccutil` намеренный: команда мутирует базу приватности,
/// и подменённый через PATH бинарь унаследовал бы TCC-ответственность
/// приложения — тот самый механизм, который чинит весь этот дифф.
#[tauri::command]
pub async fn reset_permission(pane: String) -> Result<(), AppError> {
    let service = tcc_service(&pane)?;

    let status = tokio::task::spawn_blocking(move || {
        std::process::Command::new(TCCUTIL_PATH)
            .arg("reset")
            .arg(service)
            .arg(crate::app_env::identifier())
            .status()
    })
    .await
    .map_err(|e| AppError::Other(reset_failed(service, &e.to_string())))?
    .map_err(|e| AppError::Other(reset_failed(service, &e.to_string())))?;

    if !status.success() {
        return Err(AppError::Other(reset_failed(service, &status.to_string())));
    }
    Ok(())
}

const TCCUTIL_PATH: &str = "/usr/bin/tccutil";

/// Код ошибки сброса для `api/errors.ts`.
///
/// Отдельный от permission-группы намеренно: строка `tccutil reset Microphone
/// failed` попадала в паттерн `/(microphone)/i` и превращалась в «нет доступа
/// к микрофону, открой Настройки» — совет, который к неудавшемуся сбросу
/// отношения не имеет и уводит пользователя не туда.
fn reset_failed(service: &str, cause: &str) -> String {
    format!("permission reset failed: {service} ({cause})")
}

#[tauri::command]
pub fn open_system_privacy_pane(pane: String) -> Result<(), AppError> {
    let url = privacy_pane_url(&pane)?;

    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| AppError::Other(format!("open failed: {e}")))?;
    Ok(())
}

/// Имя TCC-сервиса по идентификатору панели (правило 7: граница доверия).
fn tcc_service(pane: &str) -> Result<&'static str, AppError> {
    match pane {
        "microphone" => Ok("Microphone"),
        "screen_recording" => Ok("ScreenCapture"),
        _ => Err(AppError::Other(format!("unknown pane: {pane}"))),
    }
}

/// URL панели «Конфиденциальность и безопасность» по идентификатору панели.
fn privacy_pane_url(pane: &str) -> Result<&'static str, AppError> {
    match pane {
        "microphone" => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        }
        "screen_recording" => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        }
        _ => Err(AppError::Other(format!("unknown pane: {pane}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Панели, известные фронтенду. Разъедется с `SystemPane` в
    /// `api/permissions.ts` — иконка «Открыть настройки» начнёт отдавать
    /// ошибку вместо панели.
    #[test]
    fn known_panes_resolve_everywhere() {
        for pane in ["microphone", "screen_recording"] {
            assert!(tcc_service(pane).is_ok(), "нет TCC-сервиса для {pane}");
            assert!(privacy_pane_url(pane).is_ok(), "нет URL панели для {pane}");
        }
    }

    /// Строка приходит из webview. Ни в аргументы `tccutil`, ни в `open` она
    /// не должна попадать неизвестной — иначе это подстановка чужого сервиса
    /// или чужого URL-схемного адреса.
    #[test]
    fn unknown_pane_is_rejected_not_passed_through() {
        for pane in ["accessibility", "Camera", "All", "", "microphone; rm -rf /"] {
            assert!(tcc_service(pane).is_err(), "принял сервис {pane:?}");
            assert!(privacy_pane_url(pane).is_err(), "принял URL {pane:?}");
        }
    }

    /// `tccutil` мутирует базу приватности, а подменённый через PATH бинарь
    /// унаследовал бы TCC-ответственность приложения. Путь обязан быть
    /// абсолютным и существовать.
    #[test]
    fn tccutil_is_resolved_by_absolute_path() {
        assert!(TCCUTIL_PATH.starts_with('/'), "PATH-резолвинг запрещён");
        assert!(
            std::path::Path::new(TCCUTIL_PATH).exists(),
            "{TCCUTIL_PATH} не найден — macOS переехал, поправь путь"
        );
    }

    /// Код ошибки не должен попадать в permission-паттерны `api/errors.ts`:
    /// иначе неудавшийся сброс объясняется пользователю как «нет доступа к
    /// микрофону, открой Настройки» — совет не по адресу.
    #[test]
    fn reset_error_code_is_distinct_from_permission_denial() {
        let message = reset_failed("Microphone", "exit status: 1");
        assert!(message.starts_with("permission reset failed:"));
        assert!(!message.contains("permission denied"));
    }
}
