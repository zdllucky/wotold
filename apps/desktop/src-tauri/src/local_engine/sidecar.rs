//! [M12.6.4] Shared utilities для sidecar spawn — RAII guard убивающий
//! OS-процесс на любом неожиданном выходе (cancel via `JoinHandle::abort`,
//! panic, timeout — всё ведёт в `Drop`).
//!
//! Проблема: `tauri-plugin-shell::CommandChild` НЕ имеет `Drop` impl. Если
//! `tokio::task::JoinHandle::abort()` обрубит pipeline task, async stack
//! раскрутится, `CommandChild` будет dropped — но процесс llama-cli /
//! whisper-cli продолжит работать ещё 5-20 минут до timeout (или часами на
//! Quality preset). Это PRD §M12.6.4 cancellation requirement.
//!
//! Решение — `SidecarGuard<T>` владеет `CommandChild` через `Option<T>`,
//! `Drop` вызывает `kill()` если child ещё не явно released. Pattern usage:
//!
//! ```ignore
//! let (rx, child) = sidecar.spawn()?;
//! let mut guard = SidecarGuard::new(child);
//! // ... process events ...
//! match event {
//!     Terminated(_) => {
//!         guard.release(); // child уже мёртв — не убивать
//!         break;
//!     }
//!     _ => continue,
//! }
//! ```
//!
//! Если функция возвращает Err / cancel'ится / panic'ует ДО `release()`,
//! `Drop` шлёт SIGKILL процессу.

use std::time::Duration;

use tauri_plugin_shell::process::CommandEvent;

use crate::local_engine::llm_prompt::truncate_at_char_boundary;
use crate::providers::llm::LlmError;

use tauri_plugin_shell::process::CommandChild;

/// RAII guard над CommandChild. Drop = `kill()` если ещё не released.
pub struct SidecarGuard {
    child: Option<CommandChild>,
}

impl SidecarGuard {
    pub fn new(child: CommandChild) -> Self {
        Self { child: Some(child) }
    }

    /// Освободить guard: следующий drop НЕ убьёт процесс. Вызывать после
    /// `Terminated` event (процесс уже мёртв) или после явного `kill()`.
    pub fn release(mut self) {
        self.child.take();
    }

    /// Явный SIGKILL + release. Используется на timeout / sidecar error
    /// чтобы не дожидаться drop.
    pub fn kill(mut self) {
        if let Some(child) = self.child.take() {
            let _ = child.kill();
        }
    }
}

impl Drop for SidecarGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            // Best-effort kill — ошибки игнорируем, процесс мог уже умереть.
            let _ = child.kill();
        }
    }
}

/// [TD-11] RAII guard над временными файлами/директориями с sensitive
/// содержимым (промпты, whisper-JSON — оба несут транскрипт звонка).
///
/// Проблема: cleanup через `tokio::fs::remove_file` после `await` не
/// выполняется, если задачу оборвали `JoinHandle::abort()` (отмена звонка,
/// выход) — async-стек раскручивается, до удаления управление не доходит, и
/// файл с транскриптом остаётся в /tmp до очистки ОС. Плюс между созданием
/// файла и ручным cleanup'ом стоят ранние `?`-возвраты, на которых утечка
/// происходит даже без отмены.
///
/// `Drop` удаляет через **синхронный** `std::fs::remove_file`/`remove_dir_all`:
/// в `Drop` нельзя await, а на оборванной задаче tokio-runtime может быть
/// недоступен. Зарегистрированное чистится при любом выходе — happy-path,
/// ранний `?`, panic, abort. Best-effort: ошибки удаления игнорируются.
#[derive(Default)]
pub struct TempFileGuard {
    files: Vec<std::path::PathBuf>,
    dirs: Vec<std::path::PathBuf>,
    released: bool,
}

impl TempFileGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Зарегистрировать файл к удалению. Вызывать сразу после создания, ДО
    /// любых `?`.
    pub fn push_file(&mut self, path: impl Into<std::path::PathBuf>) {
        self.files.push(path.into());
    }

    /// Зарегистрировать директорию (удаляется рекурсивно). Для приватных
    /// поддиректорий, куда сайдкар пишет вывод.
    pub fn push_dir(&mut self, path: impl Into<std::path::PathBuf>) {
        self.dirs.push(path.into());
    }

    /// Отменить удаление — следующий drop ничего не тронет. Симметрично
    /// `SidecarGuard::release`.
    #[allow(dead_code)]
    pub fn release(mut self) {
        self.released = true;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        for f in &self.files {
            let _ = std::fs::remove_file(f);
        }
        for d in &self.dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

// [TD-41] Общий запуск сайдкара с таймаутом переехал сюда из `llm.rs`
// (1063 строки при лимите 800, правило 8): здесь уже живёт RAII-guard,
// которым этот код и пользуется.
/// Запустить sidecar, ждать `Terminated`, агрегировать stdout. Таймаут
/// `tokio::time::timeout`. На таймауте — `drop(child)` шлёт kill сигнал.
pub(crate) async fn run_sidecar_with_timeout(
    sidecar: tauri_plugin_shell::process::Command,
    timeout: Duration,
) -> Result<String, LlmError> {
    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| LlmError::Provider(format!("sidecar spawn: {e}")))?;
    // [M12.3.6 + M12.6.4] llama-cli не читает stdin (prompt через `-f`).
    // `SidecarGuard` гарантирует SIGKILL процессу при cancel / panic /
    // unhandled Err — без него process переживает abort task'а на 5+ минут.
    let mut guard = Some(SidecarGuard::new(child));

    let start = std::time::Instant::now();
    log::info!("llama sidecar spawn: timeout={}s", timeout.as_secs());

    let mut stdout = Vec::<u8>::new();
    let mut stderr = Vec::<u8>::new();
    let mut exit_code: Option<i32> = None;
    let drained = tokio::time::timeout(timeout, async {
        loop {
            // Heartbeat: если 30s нет событий — log warning что процесс
            // молчит (model load или stuck). Outer timeout всё равно
            // прикончит через `timeout` seconds — heartbeat lapping в
            // паузах между ним.
            let event = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await;
            let event = match event {
                Ok(Some(ev)) => ev,
                Ok(None) => return Ok::<(), LlmError>(()), // channel closed
                Err(_) => {
                    log::warn!(
                        "llama silent for 30s+ (elapsed={}s, stdout={}b, stderr={}b)",
                        start.elapsed().as_secs(),
                        stdout.len(),
                        stderr.len()
                    );
                    continue;
                }
            };
            match event {
                CommandEvent::Stdout(b) => {
                    stdout.extend_from_slice(&b);
                    // [security-scan W5] Содержимое НЕ логируем. Вывод модели
                    // на этом пути — это саммари звонка: имена, договорённости,
                    // всё сказанное. Логи ротируются на диске и переживают
                    // удаление звонка, то есть «удалил звонок» переставало
                    // означать «следов не осталось». В логе остаётся только
                    // прогресс: сколько байт и за сколько миллисекунд.
                    log::debug!(
                        "llama stdout +{}b ({}ms, total={}b)",
                        b.len(),
                        start.elapsed().as_millis(),
                        stdout.len()
                    );
                }
                CommandEvent::Stderr(b) => {
                    stderr.extend_from_slice(&b);
                    let chunk = String::from_utf8_lossy(&b);
                    log::debug!(
                        "llama stderr +{}b ({}ms): {}",
                        b.len(),
                        start.elapsed().as_millis(),
                        chunk.trim_end()
                    );
                }
                CommandEvent::Terminated(p) => {
                    exit_code = p.code;
                    log::info!(
                        "llama Terminated: code={:?}, elapsed={}ms, stdout={}b, stderr={}b",
                        p.code,
                        start.elapsed().as_millis(),
                        stdout.len(),
                        stderr.len()
                    );
                    return Ok::<(), LlmError>(());
                }
                CommandEvent::Error(e) => {
                    log::warn!(
                        "llama Error event: {e} (elapsed={}ms)",
                        start.elapsed().as_millis()
                    );
                    return Err(LlmError::Provider(format!("sidecar error: {e}")));
                }
                _ => {}
            }
        }
    })
    .await;

    let stderr_snippet = || {
        let s = String::from_utf8_lossy(&stderr);
        if s.is_empty() {
            String::new()
        } else {
            format!("; stderr: {}", truncate_at_char_boundary(&s, 512))
        }
    };

    match drained {
        Ok(Ok(())) => {
            // Terminated event received — процесс мёртв. Release guard
            // чтобы Drop не пытался killить уже-завершившийся pid.
            if let Some(g) = guard.take() {
                g.release();
            }
            if let Some(code) = exit_code {
                if code != 0 {
                    return Err(LlmError::Provider(format!(
                        "sidecar exit code {code}; output {} bytes{}",
                        stdout.len(),
                        stderr_snippet()
                    )));
                }
            }
            Ok(String::from_utf8_lossy(&stdout).into_owned())
        }
        Ok(Err(e)) => {
            // Sidecar Error event — child может быть ещё жив, явный kill.
            if let Some(g) = guard.take() {
                g.kill();
            }
            Err(e)
        }
        Err(_) => {
            // [PRD §M12.3.6 + M12.6.4] Timeout / cancel — child работает,
            // нужен SIGKILL. `tauri_plugin_shell::CommandChild::kill()` →
            // `SharedChild::kill()` → POSIX kill(SIGKILL). При abort task'а
            // `guard` всё равно drop'нется и убьёт процесс (defense-in-depth).
            if let Some(g) = guard.take() {
                g.kill();
            }
            Err(LlmError::Provider(format!(
                "local_llm_timeout{}",
                stderr_snippet()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn drop_removes_registered_files() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("transcript.txt");
        fs::write(&f, b"sensitive").unwrap();
        {
            let mut g = TempFileGuard::new();
            g.push_file(&f);
            assert!(f.exists());
        }
        assert!(!f.exists(), "файл должен исчезнуть при drop");
    }

    #[test]
    fn drop_removes_registered_dir_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("wotold-stt-xyz");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("out.json"), b"{}").unwrap();
        {
            let mut g = TempFileGuard::new();
            g.push_dir(&sub);
        }
        assert!(!sub.exists(), "директория должна исчезнуть рекурсивно");
    }

    #[test]
    fn drop_is_best_effort_on_missing_file() {
        // Файл уже удалён (happy-path успел почистить) — drop не паникует.
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("gone.txt");
        let mut g = TempFileGuard::new();
        g.push_file(&f); // никогда не создавали
        drop(g); // не должно паниковать
    }

    #[test]
    fn release_cancels_removal() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("keep.txt");
        fs::write(&f, b"x").unwrap();
        let mut g = TempFileGuard::new();
        g.push_file(&f);
        g.release();
        assert!(f.exists(), "release отменяет удаление");
    }

    #[test]
    fn drop_after_early_return_still_cleans() {
        // Регрессия на abort/ранний-?: файл создан, guard уронён РАНЬШЕ, чем
        // выполнился бы ручной cleanup после await. Именно этот путь и терял
        // транскрипт в /tmp.
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("wotold-llama-abc.txt");
        fs::write(&f, b"prompt with transcript").unwrap();
        let mut g = TempFileGuard::new();
        g.push_file(&f);
        // симулируем ранний return: просто роняем guard, не дойдя до "cleanup"
        drop(g);
        assert!(!f.exists());
    }
}
