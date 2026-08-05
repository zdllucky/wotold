use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::AppError;

const SIDECAR_NAME: &str = "wotold-audio";
const CHECK_TIMEOUT_SECS: u64 = 5;
const REQUEST_TIMEOUT_SECS: u64 = 120;

/// [perm-usage] Спрашивали ли у пользователя Screen Capture хотя бы раз.
///
/// `CGPreflightScreenCaptureAccess` возвращает `false` и когда отказали, и
/// когда ни разу не спрашивали — различить их без приватных API нельзя. Флаг
/// закрывает эту дыру со стороны приложения: без него свежая установка
/// встречала пользователя красным «отказано» на разрешении, которого у него
/// никто не просил.
pub const SCREEN_CAPTURE_ASKED_KEY: &str = "permissions.screen_capture_asked";

#[derive(Debug, Clone, Serialize)]
pub struct PermissionsStatus {
    pub microphone: String,
    pub screen_recording: String,
}

/// Запрашивает текущий статус разрешений без триггера диалогов.
pub async fn check(app: &AppHandle, pool: &SqlitePool) -> Result<PermissionsStatus, AppError> {
    let raw = one_shot(
        app,
        serde_json::json!({"cmd": "check_permissions"}),
        CHECK_TIMEOUT_SECS,
    )
    .await?;
    refine(raw, pool).await
}

/// Триггерит macOS-диалоги запроса разрешений. target = "microphone" |
/// "screen_recording" | "all". После реквеста возвращает обновлённый статус.
/// Note: для Screen Recording новый статус вступает в силу только в новом
/// процессе sidecar'а (TCC quirk) — поэтому при denied → granted нужен
/// перезапуск приложения чтобы запись заработала.
pub async fn request(
    app: &AppHandle,
    pool: &SqlitePool,
    target: &str,
) -> Result<PermissionsStatus, AppError> {
    let raw = one_shot(
        app,
        serde_json::json!({"cmd": "request_permissions", "target": target}),
        REQUEST_TIMEOUT_SECS,
    )
    .await?;

    // Флаг ставим только после того, как сайдкар отчитался: диалог показан.
    // Если бы он выставлялся до запроса, упавший сайдкар оставлял бы за собой
    // «отказано» на разрешении, которого у пользователя никто не спросил, —
    // вместе с подсказкой про протухший грант, которого тоже не было.
    if target == "screen_recording" || target == "all" {
        crate::db::set_setting(pool, SCREEN_CAPTURE_ASKED_KEY, "1").await?;
    }

    refine(raw, pool).await
}

async fn refine(raw: PermissionsStatus, pool: &SqlitePool) -> Result<PermissionsStatus, AppError> {
    let asked = crate::db::get_setting(pool, SCREEN_CAPTURE_ASKED_KEY)
        .await?
        .is_some();
    Ok(PermissionsStatus {
        screen_recording: refine_screen_recording(&raw.screen_recording, asked),
        microphone: raw.microphone,
    })
}

/// Отделяет «ещё не спрашивали» от «отказано» для Screen Capture.
///
/// Сайдкар физически не может их различить (см. [`SCREEN_CAPTURE_ASKED_KEY`]),
/// поэтому решение принимается здесь — по флагу, который ставит [`request`].
fn refine_screen_recording(raw: &str, asked: bool) -> String {
    if raw == "denied" && !asked {
        return "not_determined".to_string();
    }
    raw.to_string()
}

/// Чем закончилась попытка получить событие из сайдкара.
#[derive(Debug, PartialEq, Eq)]
enum ProbeOutcome {
    /// Пришло `{"event":"permissions", …}`.
    Event(Value),
    /// Процесс завершился сам, события так и не прислав.
    Terminated {
        code: Option<i32>,
        signal: Option<i32>,
    },
    /// Поток событий закрылся без `Terminated` — сайдкар унесло целиком.
    Closed,
}

async fn one_shot(
    app: &AppHandle,
    cmd: Value,
    timeout_secs: u64,
) -> Result<PermissionsStatus, AppError> {
    let sidecar = app
        .shell()
        .sidecar(SIDECAR_NAME)
        .map_err(|e| AppError::Other(format!("sidecar lookup: {e}")))?;

    let (mut rx, mut child) = sidecar
        .spawn()
        .map_err(|e| AppError::Other(format!("sidecar spawn: {e}")))?;

    let mut line = cmd.to_string();
    line.push('\n');
    child
        .write(line.as_bytes())
        .map_err(|e| AppError::Other(format!("sidecar write: {e}")))?;

    // [perm-usage] stderr копим, чтобы объяснить смерть процесса. Раньше
    // наружу шло глухое «sidecar exited without permissions event», за которым
    // на самом деле стоял SIGABRT от TCC — из такого текста причину не достать.
    let mut stderr = String::new();

    let outcome = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    if let Some(json) = parse_permissions_line(&line) {
                        return ProbeOutcome::Event(json);
                    }
                }
                CommandEvent::Stderr(bytes) => {
                    stderr.push_str(&String::from_utf8_lossy(&bytes));
                }
                // Рантайм сообщил причину явным текстом — терять его в
                // функции, вся задача которой объяснить смерть процесса,
                // означает подменить готовый диагноз на «stream closed».
                CommandEvent::Error(message) => {
                    stderr.push_str(&message);
                }
                CommandEvent::Terminated(payload) => {
                    return ProbeOutcome::Terminated {
                        code: payload.code,
                        signal: payload.signal,
                    };
                }
                _ => {}
            }
        }
        ProbeOutcome::Closed
    })
    .await;

    let _ = child.kill();

    let outcome = outcome
        .map_err(|_| AppError::Other(format!("permissions probe timed out ({timeout_secs}s)")))?;

    let event = match outcome {
        ProbeOutcome::Event(json) => json,
        other => return Err(AppError::Other(probe_failure_message(&other, &stderr))),
    };

    Ok(PermissionsStatus {
        microphone: field(&event, "microphone"),
        screen_recording: field(&event, "screen_recording"),
    })
}

fn field(event: &Value, key: &str) -> String {
    event
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

/// Строка `{"event":"permissions", …}` из stdout сайдкара, если это она.
fn parse_permissions_line(line: &str) -> Option<Value> {
    let json: Value = serde_json::from_str(line.trim()).ok()?;
    if json.get("event").and_then(Value::as_str) == Some("permissions") {
        Some(json)
    } else {
        None
    }
}

/// Хвост stderr в сообщении об ошибке: последнее, что успел сказать процесс.
const STDERR_TAIL_CHARS: usize = 200;

/// Человеконечитаемый, но диагностируемый код ошибки для UI-слоя.
///
/// Текст матчится паттернами `api/errors.ts` и переводится там — сюда кладём
/// только то, чего на фронте не узнать: сигнал, код выхода, хвост stderr.
fn probe_failure_message(outcome: &ProbeOutcome, stderr: &str) -> String {
    let trimmed = stderr.trim();
    let skip = trimmed.chars().count().saturating_sub(STDERR_TAIL_CHARS);
    let tail: String = trimmed.chars().skip(skip).collect();

    let cause = match outcome {
        ProbeOutcome::Terminated {
            signal: Some(signal),
            ..
        } => format!("signal {signal}"),
        ProbeOutcome::Terminated {
            code: Some(code), ..
        } => format!("exit {code}"),
        ProbeOutcome::Terminated { .. } => "unknown status".to_string(),
        ProbeOutcome::Closed => "stream closed".to_string(),
        ProbeOutcome::Event(_) => "no failure".to_string(),
    };

    if tail.is_empty() {
        format!("permissions sidecar terminated: {cause}")
    } else {
        format!("permissions sidecar terminated: {cause}; {tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;

    fn status(microphone: &str, screen_recording: &str) -> PermissionsStatus {
        PermissionsStatus {
            microphone: microphone.to_string(),
            screen_recording: screen_recording.to_string(),
        }
    }

    /// Клей между сайдкаром и флагом в БД (правило 1). Покрытия листа
    /// `refine_screen_recording` не хватало: перестановка полей местами —
    /// эвристика уезжает на микрофон — проходила весь набор тестов зелёной,
    /// притом что фича при этом инвертирована целиком.
    #[tokio::test]
    async fn refine_touches_only_screen_recording() {
        let db = fresh_db().await;
        let refined = refine(status("denied", "denied"), &db.pool)
            .await
            .expect("refine");

        assert_eq!(
            refined.microphone, "denied",
            "микрофон отличает отказ от «не спрашивали» сам, трогать его нельзя"
        );
        assert_eq!(
            refined.screen_recording, "not_determined",
            "Screen Capture до первого запроса — «не запрошено»"
        );
    }

    /// Миграция 0028 не должна врать новой установке: пустая база звонков
    /// значит, что разрешение действительно ещё не спрашивали.
    #[tokio::test]
    async fn fresh_install_starts_without_asked_flag() {
        let db = fresh_db().await;
        assert_eq!(
            crate::db::get_setting(&db.pool, SCREEN_CAPTURE_ASKED_KEY)
                .await
                .expect("get"),
            None,
        );
    }

    /// После запроса «отказано» обязано остаться «отказано»: иначе кнопка
    /// «Запросить» вечно предлагает диалог, которого macOS больше не покажет.
    #[tokio::test]
    async fn refine_keeps_denied_once_asked() {
        let db = fresh_db().await;
        crate::db::set_setting(&db.pool, SCREEN_CAPTURE_ASKED_KEY, "1")
            .await
            .expect("set");

        let refined = refine(status("granted", "denied"), &db.pool)
            .await
            .expect("refine");
        assert_eq!(refined.screen_recording, "denied");
        assert_eq!(refined.microphone, "granted");
    }

    #[test]
    fn parses_permissions_event() {
        let json = parse_permissions_line(
            r#"{"event":"permissions","microphone":"granted","screen_recording":"denied"}"#,
        )
        .expect("это событие permissions");
        assert_eq!(field(&json, "microphone"), "granted");
        assert_eq!(field(&json, "screen_recording"), "denied");
    }

    #[test]
    fn ignores_other_events_and_garbage() {
        assert!(parse_permissions_line(r#"{"event":"error","message":"boom"}"#).is_none());
        assert!(parse_permissions_line("не json").is_none());
        assert!(parse_permissions_line("").is_none());
    }

    #[test]
    fn missing_fields_become_unknown() {
        let json = parse_permissions_line(r#"{"event":"permissions"}"#).expect("событие");
        assert_eq!(field(&json, "microphone"), "unknown");
    }

    /// Fail-path клея: сайдкар умер, не прислав события. Именно так выглядел
    /// TCC-SIGABRT из-за отсутствующего NSMicrophoneUsageDescription — и
    /// пользователь видел строку, по которой причину узнать невозможно.
    #[test]
    fn terminated_by_signal_reports_signal_and_stderr() {
        let outcome = ProbeOutcome::Terminated {
            code: None,
            signal: Some(6),
        };
        let message = probe_failure_message(&outcome, "  dyld: symbol not found  ");
        assert!(message.starts_with("permissions sidecar terminated:"));
        assert!(message.contains("signal 6"));
        assert!(message.contains("dyld: symbol not found"));
    }

    #[test]
    fn terminated_by_exit_code_without_stderr() {
        let outcome = ProbeOutcome::Terminated {
            code: Some(1),
            signal: None,
        };
        assert_eq!(
            probe_failure_message(&outcome, ""),
            "permissions sidecar terminated: exit 1"
        );
    }

    #[test]
    fn closed_stream_is_distinguishable_from_termination() {
        assert!(probe_failure_message(&ProbeOutcome::Closed, "").contains("stream closed"));
    }

    /// Длинный stderr не должен утаскивать в UI простыню: `humanError`
    /// обрезает на 160 символах, и обрезать хвост осмысленно лучше здесь.
    #[test]
    fn stderr_tail_is_bounded() {
        let noise = "x".repeat(1000);
        let message = probe_failure_message(&ProbeOutcome::Closed, &noise);
        assert!(message.len() < 260, "слишком длинно: {}", message.len());
    }

    #[test]
    fn screen_capture_denied_is_not_determined_until_asked() {
        assert_eq!(refine_screen_recording("denied", false), "not_determined");
        assert_eq!(refine_screen_recording("denied", true), "denied");
    }

    #[test]
    fn screen_capture_granted_survives_refinement() {
        assert_eq!(refine_screen_recording("granted", false), "granted");
        assert_eq!(refine_screen_recording("granted", true), "granted");
    }
}
