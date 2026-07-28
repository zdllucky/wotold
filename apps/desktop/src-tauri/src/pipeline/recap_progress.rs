//! [P1.3] Periodic `recap:progress` event emitter wrapper.
//!
//! Local LLM recap (regen или full pipeline run) занимает от секунд до минут;
//! без UI signals юзер не знает идёт ли работа. Этот helper оборачивает
//! arbitrary `Future` и каждые `EMIT_INTERVAL` эмитит `RecapProgressEvent`
//! с elapsed_sec. На completion future (success / failure / timeout) задача
//! ticker'а аборт'ится — UI получает финальный elapsed implicitly через
//! flip `regenerating: false`.
//!
//! Headless / unit-test path (`app: None`) — ticker всё равно крутится но
//! emit no-op'ится через `EventBus::new(None)`. Это упрощает кастомизацию
//! callsite'ам (нет ветвлений Some/None).
//!
//! # Cancellation
//!
//! `tokio::task::JoinHandle::abort()` шлёт cancellation signal в spawned
//! task; sleep'ы внутри ticker'а — cancellation point'ы у tokio runtime'а,
//! поэтому abort срабатывает мгновенно (без race на «1 лишний emit»).
//!
//! # Granularity
//!
//! 15s interval — баланс между «юзер видит что что-то идёт» и noise.
//! Reasonable lower bound: 5s; upper: 30s. На Quality preset (worst case
//! 8-12 мин) даёт ~30-50 updates — без spam'а.
//!
//! # Reuse
//!
//! Используется в `pipeline::regenerate_recap_local` и `pipeline::run_local_inner`
//! — двух entry-points для local LLM. См. `events::RECAP_PROGRESS` для wire-up.

use std::future::Future;
use std::time::Duration;

use tauri::AppHandle;

use crate::events::{EventBus, RecapProgressEvent};

/// Период между emit'ами. См. модульный комментарий — обоснование 15s.
const EMIT_INTERVAL: Duration = Duration::from_secs(15);

/// Запустить `fut`, эмитя `recap:progress` event с `elapsed_sec` каждые
/// [`EMIT_INTERVAL`] секунд. На completion future (любой исход) ticker
/// аборт'ится. Возвращает результат `fut` неизменным.
///
/// `app` — `None` → emit no-op (headless / tests). Ticker всё равно
/// крутится но silently — это упрощает callsite'ы (не нужно условно
/// оборачивать).
pub async fn with_recap_progress_emitter<F, T>(app: Option<AppHandle>, call_id: String, fut: F) -> T
where
    F: Future<Output = T>,
{
    with_elapsed_emitter(app, call_id, Phase::Recap, fut).await
}

/// Фаза, за которой тикает счётчик. Различаются только именем события: UI
/// показывает их в разных местах (рекап — в шапке, STT — в панели обработки).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Recap,
    Stt,
}

/// То же самое для распознавания речи. Full-file STT на длинной записи —
/// единственный шаг, где прогресс-бар стоит на одном проценте по несколько
/// минут: whisper-cli отдаёт результат целиком, промежуточных сигналов нет.
/// Тикающий счётчик отличает «работает» от «повисло».
pub async fn with_stt_progress_emitter<F, T>(app: Option<AppHandle>, call_id: String, fut: F) -> T
where
    F: Future<Output = T>,
{
    with_elapsed_emitter(app, call_id, Phase::Stt, fut).await
}

async fn with_elapsed_emitter<F, T>(
    app: Option<AppHandle>,
    call_id: String,
    phase: Phase,
    fut: F,
) -> T
where
    F: Future<Output = T>,
{
    // Spawn ticker. `move` забирает clone'ы для long-running task.
    let ticker = tokio::spawn(async move {
        let mut interval = tokio::time::interval(EMIT_INTERVAL);
        // Первый tick `interval.tick()` immediate — skip'аем, чтобы первый
        // emit был после полного EMIT_INTERVAL (UI видит 15s, не 0s).
        interval.tick().await;
        let mut elapsed_sec: u64 = 0;
        loop {
            interval.tick().await;
            elapsed_sec += EMIT_INTERVAL.as_secs();
            let bus = EventBus::new(app.as_ref());
            let e = RecapProgressEvent {
                call_id: call_id.clone(),
                elapsed_sec,
            };
            match phase {
                Phase::Recap => bus.recap_progress(&e),
                Phase::Stt => bus.stt_progress(&e),
            }
        }
    });

    let result = fut.await;

    // Abort ticker — кооперативная cancellation в tokio. Sleep'ы внутри
    // ticker'а — instant cancellation point'ы, race на лишний emit
    // отсутствует.
    ticker.abort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity check — emitter не блокирует основной future и result
    /// пропадает наружу неизменным. Future завершается до первого
    /// EMIT_INTERVAL, поэтому ticker аборт'ится без emit'ов.
    #[tokio::test]
    async fn passes_through_result_immediately() {
        let result = with_recap_progress_emitter(None, "c1".into(), async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            42_u32
        })
        .await;
        assert_eq!(result, 42);
    }

    /// `Result<T, E>` propagation работает — future с Err передаётся неизменным.
    #[tokio::test]
    async fn result_propagation_for_err_variant() {
        let res: Result<i32, &str> =
            with_recap_progress_emitter(None, "c1".into(), async { Err("oops") }).await;
        assert_eq!(res, Err("oops"));
    }

    /// String result variant — Future<Output = String> compiles + value
    /// проходит сквозь обёртку.
    #[tokio::test]
    async fn passes_through_string_result() {
        let result =
            with_recap_progress_emitter(None, "c1".into(), async { "ok".to_string() }).await;
        assert_eq!(result, "ok");
    }
}
