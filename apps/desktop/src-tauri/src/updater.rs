use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::AppError;

/// Аварийный downgrade-режим (M11.8 паспорта). Активируется env-переменной
/// в момент запуска, в UI не выставляется. Используется только при выкатке
/// исправленного `latest.json` с меньшим semver для отката плохого релиза.
const ALLOW_DOWNGRADE_ENV: &str = "WOTOLD_UPDATER_ALLOW_DOWNGRADE";

/// Маркер принудительного обновления в теле релиза. Ставится релизным
/// workflow первой строкой при `mandatory: true`.
///
/// Нужен потому, что политика по semver умеет форсировать только мажор, а
/// критический фикс безопасности приезжает патчем. Своих полей в `latest.json`
/// не завести — его целиком генерирует `tauri-action`, — поэтому признак едет
/// в единственном свободном поле манифеста.
const MANDATORY_MARKER: &str = "!mandatory";

/// Чистое ядро сравнения версий — то, что реально проверяется тестами.
/// `compare_versions` ниже только достаёт режим из окружения: сигнатуру
/// диктует плагин, а `RemoteRelease` в тесте не собрать.
fn should_update(
    current: &semver::Version,
    release: &semver::Version,
    allow_downgrade: bool,
) -> bool {
    if allow_downgrade {
        release != current
    } else {
        release > current
    }
}

/// Сравнение версий: по умолчанию обновляемся только вверх. При выставленной
/// env `WOTOLD_UPDATER_ALLOW_DOWNGRADE` — берём любую версию, не равную текущей
/// (аварийный откат).
pub fn compare_versions(
    current: semver::Version,
    release: tauri_plugin_updater::RemoteRelease,
) -> bool {
    let allow_downgrade = std::env::var(ALLOW_DOWNGRADE_ENV).is_ok();
    should_update(&current, &release.version, allow_downgrade)
}

/// Насколько обновление обязательно. Определяет, спрашивают ли пользователя.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateUrgency {
    /// Тост с кнопкой. Момент выбирает пользователь.
    Optional,
    /// Ставится само — но только когда приложение не занято
    /// (см. `is_safe_to_restart`): запись прерывать нельзя ничем.
    Mandatory,
}

/// Мажорный бамп ломает совместимость, поэтому обновляет принудительно.
/// Маркер в первой строке release notes форсирует любую версию.
///
/// Даунгрейд принудительным не бывает никогда: откат — операция владельца
/// через `WOTOLD_UPDATER_ALLOW_DOWNGRADE`, а не то, что клиент решает сам.
pub fn classify(
    current: &semver::Version,
    next: &semver::Version,
    notes: Option<&str>,
) -> UpdateUrgency {
    if next <= current {
        return UpdateUrgency::Optional;
    }

    // Только первая строка: иначе цитата правила в changelog делала бы
    // обновление принудительным.
    let marked = notes
        .and_then(|n| n.lines().next())
        .map(str::trim)
        .is_some_and(|first| {
            let rest = first
                .get(..MANDATORY_MARKER.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(MANDATORY_MARKER))
                .then(|| &first[MANDATORY_MARKER.len()..]);
            // `!mandatorily` — не маркер: после него обязан идти конец строки
            // или пробел.
            matches!(rest, Some(tail) if tail.is_empty() || tail.starts_with(char::is_whitespace))
        });

    if marked || next.major > current.major {
        UpdateUrgency::Mandatory
    } else {
        UpdateUrgency::Optional
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct AvailableUpdate {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub urgency: UpdateUrgency,
}

/// Неблокирующая проверка (M11.4). Дёргается из фронта при старте.
pub async fn check(app: &AppHandle) -> Result<Option<AvailableUpdate>, AppError> {
    let updater = app
        .updater()
        .map_err(|e| AppError::Other(format!("updater not configured: {e}")))?;

    let update = updater
        .check()
        .await
        .map_err(|e| AppError::Other(format!("update check failed: {e}")))?;

    Ok(update.map(|u| {
        // Версии в манифесте — строки. Непарсимую версию не считаем поводом
        // уронить проверку: обновление всё равно предложим, просто не
        // принудительно.
        let urgency = match (
            semver::Version::parse(&u.current_version),
            semver::Version::parse(&u.version),
        ) {
            (Ok(current), Ok(next)) => classify(&current, &next, u.body.as_deref()),
            _ => {
                log::warn!(
                    "updater: не разобрал версии {} -> {}, считаю обновление необязательным",
                    u.current_version,
                    u.version
                );
                UpdateUrgency::Optional
            }
        };

        AvailableUpdate {
            version: u.version.clone(),
            current_version: u.current_version.clone(),
            notes: u.body.clone(),
            pub_date: u.date.map(|d| d.to_string()),
            urgency,
        }
    }))
}

/// Занятость приложения глазами апдейтера.
///
/// Запись прерывать нельзя ничем: перезапуск посреди звонка теряет то, что
/// пользователь уже не сможет переснять. Обработка тоже считается занятостью —
/// пайплайн переживает краш, но заставлять его восстанавливаться ради
/// обновления бессмысленно.
pub fn is_safe_to_restart(recording_active: bool, active_pipeline_calls: usize) -> bool {
    !recording_active && active_pipeline_calls == 0
}

/// Окружение принудительной установки. Существует ради тестируемости клея:
/// сам цикл ожидания проверяется без `AppHandle`, реального времени и сети.
#[allow(async_fn_in_trait)]
pub trait UpdateHost {
    /// Идёт запись или обработка.
    async fn is_busy(&self) -> bool;
    /// Скачать, установить и перезапустить. При успехе не возвращается.
    async fn install(&self) -> Result<(), AppError>;
}

/// Принудительное обновление не отменяется при занятости, а откладывается:
/// ждёт ближайшего простоя и ставится тогда.
///
/// Верхней границы ожидания нет намеренно. «Обновление не поставилось, потому
/// что пользователь три часа писал звонок» — правильный исход; «запись
/// оборвалась, потому что вышла новая версия» — нет.
pub async fn install_when_idle<H: UpdateHost>(
    host: &H,
    retry_every: std::time::Duration,
) -> Result<(), AppError> {
    let mut waited = false;
    loop {
        if !host.is_busy().await {
            if waited {
                log::info!("updater: приложение освободилось, ставлю отложенное обновление");
            }
            return host.install().await;
        }
        if !waited {
            log::info!("updater: приложение занято, обновление отложено до простоя");
            waited = true;
        }
        tokio::time::sleep(retry_every).await;
    }
}

/// Боевая реализация `UpdateHost` поверх состояния приложения.
pub struct AppUpdateHost<'a> {
    pub app: &'a AppHandle,
}

impl UpdateHost for AppUpdateHost<'_> {
    async fn is_busy(&self) -> bool {
        let state = tauri::Manager::state::<crate::state::AppState>(self.app);
        let recording_active = state.recording.lock().await.is_some();
        let active_calls = state.pipeline_tasks.lock().await.len();
        !is_safe_to_restart(recording_active, active_calls)
    }

    async fn install(&self) -> Result<(), AppError> {
        apply(self.app).await
    }
}

/// Скачать и поставить апдейт, затем перезапуск. Никогда не возвращается при успехе
/// (`app.restart()` завершает процесс).
pub async fn apply(app: &AppHandle) -> Result<(), AppError> {
    let updater = app
        .updater()
        .map_err(|e| AppError::Other(format!("updater not configured: {e}")))?;

    let update = updater
        .check()
        .await
        .map_err(|e| AppError::Other(format!("update check failed: {e}")))?
        .ok_or_else(|| AppError::Other("no update available".into()))?;

    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| AppError::Other(format!("update install failed: {e}")))?;

    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).expect("test version literal must parse")
    }

    // ── should_update: направление обновления ──────────────────────────

    #[test]
    fn updates_forward_only_by_default() {
        assert!(should_update(&v("1.0.0"), &v("1.0.1"), false));
        assert!(should_update(&v("1.0.0"), &v("2.0.0"), false));
        assert!(!should_update(&v("1.0.0"), &v("1.0.0"), false));
        assert!(!should_update(&v("1.2.0"), &v("1.1.9"), false));
    }

    #[test]
    fn downgrade_mode_accepts_any_different_version() {
        assert!(should_update(&v("1.2.0"), &v("1.1.9"), true));
        assert!(should_update(&v("2.0.0"), &v("1.0.0"), true));
        // Равная версия не обновление ни в каком режиме — иначе аварийный
        // откат зациклится на самом себе.
        assert!(!should_update(&v("1.0.0"), &v("1.0.0"), true));
    }

    // ── classify: обязательность ───────────────────────────────────────

    #[test]
    fn patch_and_minor_are_optional() {
        assert_eq!(
            classify(&v("1.0.0"), &v("1.0.1"), None),
            UpdateUrgency::Optional
        );
        assert_eq!(
            classify(&v("1.0.0"), &v("1.1.0"), None),
            UpdateUrgency::Optional
        );
        assert_eq!(
            classify(&v("1.4.2"), &v("1.9.9"), None),
            UpdateUrgency::Optional
        );
    }

    #[test]
    fn major_bump_is_mandatory() {
        assert_eq!(
            classify(&v("1.0.0"), &v("2.0.0"), None),
            UpdateUrgency::Mandatory
        );
        assert_eq!(
            classify(&v("1.9.9"), &v("3.0.1"), None),
            UpdateUrgency::Mandatory
        );
    }

    /// Граница, на которой политика включается впервые: проект уезжает с 0.x
    /// на 1.0.0, и этот переход обязан быть принудительным.
    #[test]
    fn first_stable_release_is_mandatory() {
        assert_eq!(
            classify(&v("0.9.0"), &v("1.0.0"), None),
            UpdateUrgency::Mandatory
        );
        assert_eq!(
            classify(&v("0.0.1"), &v("1.0.0"), None),
            UpdateUrgency::Mandatory
        );
    }

    /// Аварийный выход: критический фикс в patch-версии тоже можно
    /// форсировать — по semver это иначе недостижимо.
    #[test]
    fn marker_forces_any_version() {
        assert_eq!(
            classify(
                &v("1.0.0"),
                &v("1.0.1"),
                Some("!mandatory\n\nCritical fix.")
            ),
            UpdateUrgency::Mandatory
        );
        assert_eq!(
            classify(&v("1.0.0"), &v("1.1.0"), Some("  !MANDATORY  \nrest")),
            UpdateUrgency::Mandatory
        );
    }

    #[test]
    fn marker_is_only_honoured_on_the_first_line() {
        // Иначе любое упоминание слова в теле релиза делало бы обновление
        // принудительным — включая цитату этого же правила в changelog.
        assert_eq!(
            classify(&v("1.0.0"), &v("1.0.1"), Some("Fixes\n!mandatory")),
            UpdateUrgency::Optional
        );
    }

    #[test]
    fn empty_and_junk_notes_do_not_force() {
        assert_eq!(
            classify(&v("1.0.0"), &v("1.0.1"), None),
            UpdateUrgency::Optional
        );
        assert_eq!(
            classify(&v("1.0.0"), &v("1.0.1"), Some("")),
            UpdateUrgency::Optional
        );
        assert_eq!(
            classify(&v("1.0.0"), &v("1.0.1"), Some("   \n\n  ")),
            UpdateUrgency::Optional
        );
        assert_eq!(
            classify(
                &v("1.0.0"),
                &v("1.0.1"),
                Some("mandatory rewrite of the parser")
            ),
            UpdateUrgency::Optional
        );
    }

    /// Граница слова. Кейсы подобраны так, чтобы префикс `!mandatory`
    /// совпадал ПОЛНОСТЬЮ и решение принимала именно проверка следующего
    /// символа.
    ///
    /// Первая версия этого теста использовала `!mandatorily` и была
    /// бесполезна: она расходится с маркером уже на десятом символе (`i`
    /// вместо `y`), то есть отсекается сравнением префикса и до проверки
    /// границы не доходит. Мутация проверки на наивный `starts_with`
    /// оставляла тесты зелёными — отсюда эти кейсы.
    #[test]
    fn marker_requires_a_word_boundary() {
        for junk in [
            "!mandatoryX",
            "!mandatory-ish",
            "!mandatory:",
            "!mandatory2",
        ] {
            assert_eq!(
                classify(&v("1.0.0"), &v("1.0.1"), Some(junk)),
                UpdateUrgency::Optional,
                "{junk} не должен считаться маркером"
            );
        }

        for real in ["!mandatory", "!mandatory ", "!mandatory security fix"] {
            assert_eq!(
                classify(&v("1.0.0"), &v("1.0.1"), Some(real)),
                UpdateUrgency::Mandatory,
                "{real} обязан считаться маркером"
            );
        }
    }

    /// Даунгрейд мажора принудительным быть не может: откат — операция
    /// владельца, а не то, что клиент делает сам.
    #[test]
    fn downgrade_is_never_mandatory() {
        assert_eq!(
            classify(&v("2.0.0"), &v("1.0.0"), None),
            UpdateUrgency::Optional
        );
    }

    // ── is_safe_to_restart ─────────────────────────────────────────────

    #[test]
    fn restart_allowed_only_when_fully_idle() {
        assert!(is_safe_to_restart(false, 0));
        assert!(!is_safe_to_restart(true, 0), "идёт запись");
        assert!(!is_safe_to_restart(false, 1), "идёт обработка");
        assert!(!is_safe_to_restart(true, 3), "и то и другое");
    }

    // ── install_when_idle: клей ────────────────────────────────────────

    /// Хост, у которого занятость задана расписанием: `busy[i]` — ответ на
    /// i-й опрос. Считает установки, чтобы отличить «поставилось» от
    /// «поставилось дважды».
    struct ScriptedHost {
        busy: std::sync::Mutex<std::collections::VecDeque<bool>>,
        installs: std::sync::atomic::AtomicUsize,
        install_result: Result<(), &'static str>,
    }

    impl ScriptedHost {
        fn new(busy: impl IntoIterator<Item = bool>) -> Self {
            Self {
                busy: std::sync::Mutex::new(busy.into_iter().collect()),
                installs: std::sync::atomic::AtomicUsize::new(0),
                install_result: Ok(()),
            }
        }

        fn failing(busy: impl IntoIterator<Item = bool>) -> Self {
            Self {
                install_result: Err("disk full"),
                ..Self::new(busy)
            }
        }

        fn installs(&self) -> usize {
            self.installs.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn polls_left(&self) -> usize {
            self.busy.lock().expect("test mutex").len()
        }
    }

    impl UpdateHost for ScriptedHost {
        async fn is_busy(&self) -> bool {
            // Расписание кончилось — считаем свободным, чтобы кривой тест
            // висел не вечно, а падал на ассерте.
            self.busy
                .lock()
                .expect("test mutex")
                .pop_front()
                .unwrap_or(false)
        }

        async fn install(&self) -> Result<(), AppError> {
            self.installs
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.install_result
                .map_err(|e| AppError::Other(e.to_string()))
        }
    }

    const RETRY: std::time::Duration = std::time::Duration::from_secs(30);

    #[tokio::test(start_paused = true)]
    async fn installs_immediately_when_idle() {
        let host = ScriptedHost::new([false]);
        install_when_idle(&host, RETRY).await.expect("install");
        assert_eq!(host.installs(), 1);
    }

    /// Fail-path, ради которого весь этот трейт и существует: пока идёт
    /// запись, установки не происходит вовсе.
    #[tokio::test(start_paused = true)]
    async fn waits_out_a_recording_before_installing() {
        let host = ScriptedHost::new([true, true, true, false]);
        install_when_idle(&host, RETRY).await.expect("install");

        assert_eq!(host.installs(), 1, "поставить обязано ровно один раз");
        assert_eq!(host.polls_left(), 0, "должен был опросить всё расписание");
    }

    /// Ожидание не имеет верхней границы: длинная запись откладывает
    /// обновление настолько, насколько нужно.
    #[tokio::test(start_paused = true)]
    async fn long_recording_does_not_time_out() {
        let host = ScriptedHost::new(std::iter::repeat_n(true, 500).chain([false]));
        install_when_idle(&host, RETRY).await.expect("install");
        assert_eq!(host.installs(), 1);
        // Без этой проверки тест зеленел бы и при полностью убранном гейте.
        assert_eq!(host.polls_left(), 0, "ожидание обязано пережить всю запись");
    }

    #[tokio::test(start_paused = true)]
    async fn install_failure_propagates_and_does_not_retry() {
        let host = ScriptedHost::failing([false]);
        let err = install_when_idle(&host, RETRY)
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("disk full"), "got: {err}");
        assert_eq!(host.installs(), 1, "повторять установку сами не должны");
    }
}
