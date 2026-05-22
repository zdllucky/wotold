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
