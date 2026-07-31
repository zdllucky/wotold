//! [T2] Задача-обёртка вокруг решающего ядра [`super::SilenceWatch`].
//!
//! Вынесена из `mod.rs` не по вкусу, а по гейту 800 строк (правило 8): ядро с
//! его таблицей случаев и плумбинг каналов — две разные когезии, и растут они
//! независимо. Здесь нет ни одного решения о тишине, только маршрутизация
//! сигналов и побочные эффекты через инжектируемый колбэк.

use std::future::Future;

use tokio::sync::{mpsc, oneshot};

use super::{SilenceEvent, SilenceWatch, SilenceWatchConfig};

/// Боксированное будущее колбэка `on_event` — та же форма, что `RotateFut` /
/// `EnqueueFut` у оркестратора: сам [`run`] дженерик, но callsite'у нужен один
/// конкретный тип, чтобы замыкание не расползалось по сигнатурам.
pub type SilenceEventFut = std::pin::Pin<Box<dyn Future<Output = ()> + Send>>;

/// Управляющие сигналы задачи-обёртки. Отдельным enum'ом вместо трёх каналов:
/// порядок между pause и snooze имеет значение, а один канал его сохраняет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilenceControl {
    Pause,
    Resume,
    /// «Продолжить» из подсказки.
    Snooze,
}

/// [T2] Задача-обёртка вокруг [`SilenceWatch`]. Форма — как у
/// `chunk_orchestrator::run`: `select!` по каналам, ни одного `sleep`
/// (инженерное правило 6), побочные эффекты через инжектируемый колбэк.
///
/// Живёт независимо от chunked-режима: `prepare_chunked_setup` возвращает
/// `None` при отсутствии preset'а, а тишину надо ловить всегда.
///
/// Возвращается когда пришёл `stop_rx`, закрылся `rms_rx` (запись кончилась)
/// либо после отданного `AutoStop` — дальше следить не за чем.
pub async fn run<F, Fut>(
    cfg: SilenceWatchConfig,
    mut rms_rx: mpsc::Receiver<(u64, f32)>,
    mut control_rx: mpsc::Receiver<SilenceControl>,
    mut stop_rx: oneshot::Receiver<()>,
    on_event: F,
) -> SilenceWatchSummary
where
    F: Fn(SilenceEvent) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let mut watch = SilenceWatch::new(cfg);
    let mut summary = SilenceWatchSummary::default();

    loop {
        tokio::select! {
            _ = &mut stop_rx => {
                log::debug!("silence_watch stop signal received");
                break;
            }

            maybe_control = control_rx.recv() => {
                match maybe_control {
                    Some(SilenceControl::Pause) => watch.on_pause(),
                    Some(SilenceControl::Resume) => watch.on_resume(),
                    Some(SilenceControl::Snooze) => {
                        summary.snoozes += 1;
                        watch.snooze();
                    }
                    // Канал закрыт — caller дропнул tx без stop-сигнала
                    // (обычно cleanup в stop_recording). Продолжаем до
                    // закрытия rms_rx / stop_rx, как и оркестратор.
                    None => {}
                }
            }

            maybe_sample = rms_rx.recv() => {
                let Some((ts_ms, rms)) = maybe_sample else {
                    log::debug!("silence_watch rms channel closed");
                    break;
                };
                match watch.on_sample(ts_ms, rms) {
                    SilenceEvent::None => {}
                    event @ SilenceEvent::SuggestStop { .. } => {
                        summary.suggestions += 1;
                        on_event(event).await;
                    }
                    event @ SilenceEvent::AutoStop { trim_at_ms, .. } => {
                        summary.auto_stopped_at_ms = Some(trim_at_ms);
                        on_event(event).await;
                        // Запись останавливается — второго решения не будет.
                        break;
                    }
                }
            }
        }
    }

    summary
}

/// Сводка отработавшей задачи — для логов и тестов клея.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SilenceWatchSummary {
    pub suggestions: u32,
    pub snoozes: u32,
    /// `Some(trim_at_ms)` если задача выдала авто-стоп.
    pub auto_stopped_at_ms: Option<u64>,
}

/// [T2] Ручки активного наблюдателя — живут в `AppState` рядом с
/// `orchestrator_*`. `JoinHandle` намеренно не храним: задача самозавершается,
/// когда диспатчер дропает `rms_tx` на терминальном событии сайдкара, а
/// `stop_tx` нужен лишь чтобы не ждать этого момента на ручном стопе.
pub struct SilenceWatchHandles {
    control_tx: mpsc::Sender<SilenceControl>,
    stop_tx: Option<oneshot::Sender<()>>,
}

impl SilenceWatchHandles {
    pub fn new(control_tx: mpsc::Sender<SilenceControl>, stop_tx: oneshot::Sender<()>) -> Self {
        Self {
            control_tx,
            stop_tx: Some(stop_tx),
        }
    }

    /// Fire-and-forget управляющий сигнал. Буфер полон или задача умерла —
    /// дропаем: следующий pause/resume починит состояние (тот же выбор, что у
    /// `orchestrator_pause_tx`).
    pub fn signal(&self, control: SilenceControl) {
        let _ = self.control_tx.try_send(control);
    }

    /// Погасить наблюдателя. Идемпотентно.
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    //    все три прод-бага M13 были в клее при параноидально покрытых листьях.

    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    /// Порог 1с/2с и гистерезис 200мс — читаемые числа вместо минут.
    fn cfg() -> SilenceWatchConfig {
        SilenceWatchConfig {
            floor_rms: 0.01,
            voice_hysteresis_ms: 200,
            suggest_after_ms: Some(1_000),
            auto_stop_after_ms: Some(2_000),
            tail_pad_ms: 100,
        }
    }

    const QUIET: f32 = 0.001;

    /// Колбэк, складывающий решения в общий вектор — заменяет собой всё, что в
    /// проде делает Tauri (эмит события и стоп записи).
    fn recorder() -> (
        Arc<Mutex<Vec<SilenceEvent>>>,
        impl Fn(SilenceEvent) -> SilenceEventFut,
    ) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let f = move |ev: SilenceEvent| {
            let sink = Arc::clone(&sink);
            Box::pin(async move {
                sink.lock().expect("poisoned").push(ev);
            }) as SilenceEventFut
        };
        (seen, f)
    }

    #[tokio::test]
    async fn glue_happy_path_suggests_then_auto_stops() {
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(64);
        let (_ctl_tx, ctl_rx) = mpsc::channel::<SilenceControl>(4);
        let (_stop_tx, stop_rx) = oneshot::channel::<()>();
        let (seen, on_event) = recorder();

        let task = tokio::spawn(run(cfg(), rms_rx, ctl_rx, stop_rx, on_event));

        let mut ts = 0;
        while ts <= 3_000 {
            // Буфер 64 против 31 сэмпла — ни одного дропа, ждать нечего.
            rms_tx.send((ts, QUIET)).await.expect("send");
            ts += 100;
        }
        drop(rms_tx);

        let summary = task.await.expect("task");
        assert_eq!(summary.suggestions, 1);
        assert_eq!(summary.auto_stopped_at_ms, Some(100));
        let seen = seen.lock().expect("poisoned").clone();
        assert_eq!(
            seen,
            vec![
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: Some(1_000)
                },
                SilenceEvent::AutoStop {
                    trim_at_ms: 100,
                    silent_for_ms: 2_000
                },
            ]
        );
    }

    #[tokio::test]
    async fn glue_exits_when_rms_channel_closes_mid_run() {
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(8);
        let (_ctl_tx, ctl_rx) = mpsc::channel::<SilenceControl>(4);
        let (_stop_tx, stop_rx) = oneshot::channel::<()>();
        let (seen, on_event) = recorder();

        let task = tokio::spawn(run(cfg(), rms_rx, ctl_rx, stop_rx, on_event));
        rms_tx.send((0, QUIET)).await.expect("send");
        rms_tx.send((500, QUIET)).await.expect("send");
        drop(rms_tx);

        let summary = task.await.expect("задача обязана выйти, а не висеть");
        assert_eq!(summary, SilenceWatchSummary::default());
        assert!(seen.lock().expect("poisoned").is_empty());
    }

    #[tokio::test]
    async fn glue_exits_on_stop_signal() {
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(8);
        let (_ctl_tx, ctl_rx) = mpsc::channel::<SilenceControl>(4);
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let (_seen, on_event) = recorder();

        let task = tokio::spawn(run(cfg(), rms_rx, ctl_rx, stop_rx, on_event));
        stop_tx.send(()).expect("stop signal");
        let summary = task.await.expect("task");
        assert_eq!(summary.auto_stopped_at_ms, None);
        // rms_tx жив — выход именно по stop-сигналу, не по закрытию канала.
        drop(rms_tx);
    }

    #[tokio::test]
    async fn glue_snooze_postpones_auto_stop() {
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(64);
        let (ctl_tx, ctl_rx) = mpsc::channel::<SilenceControl>(4);
        let (_stop_tx, stop_rx) = oneshot::channel::<()>();
        let (seen, on_event) = recorder();

        let task = tokio::spawn(run(cfg(), rms_rx, ctl_rx, stop_rx, on_event));

        // До подсказки включительно.
        let mut ts = 0;
        while ts <= 1_000 {
            rms_tx.send((ts, QUIET)).await.expect("send");
            ts += 100;
        }
        // Snooze должен быть обработан ДО следующих сэмплов. Гарантия — не
        // sleep, а порядок в одном канале: сначала дожидаемся, что задача
        // забрала все rms-сэмплы (capacity вернулась), потом шлём control.
        while rms_tx.capacity() < 64 {
            tokio::task::yield_now().await;
        }
        ctl_tx.send(SilenceControl::Snooze).await.expect("snooze");
        while ctl_tx.capacity() < 4 {
            tokio::task::yield_now().await;
        }

        // Исходный дедлайн 2000 — после snooze он не должен сработать.
        while ts <= 2_500 {
            rms_tx.send((ts, QUIET)).await.expect("send");
            ts += 100;
        }
        drop(rms_tx);

        let summary = task.await.expect("task");
        assert_eq!(summary.snoozes, 1);
        assert_eq!(summary.auto_stopped_at_ms, None, "snooze не отложил стоп");
        // Тишина после snooze — новый run: подсказка приходит заново через
        // полный интервал (в прод — ещё 15 минут), дедлайн стопа уехал с
        // 2000 на 3100 и до конца прогона не наступает.
        assert_eq!(
            seen.lock().expect("poisoned").clone(),
            vec![
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: Some(1_000)
                },
                SilenceEvent::SuggestStop {
                    silent_for_ms: 1_000,
                    auto_stop_in_ms: Some(1_000)
                },
            ]
        );
        assert_eq!(summary.suggestions, 2);
    }

    #[tokio::test]
    async fn glue_pause_resume_does_not_trip_stop() {
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(64);
        let (ctl_tx, ctl_rx) = mpsc::channel::<SilenceControl>(4);
        let (_stop_tx, stop_rx) = oneshot::channel::<()>();
        let (seen, on_event) = recorder();

        let task = tokio::spawn(run(cfg(), rms_rx, ctl_rx, stop_rx, on_event));
        ctl_tx.send(SilenceControl::Pause).await.expect("pause");
        while ctl_tx.capacity() < 4 {
            tokio::task::yield_now().await;
        }

        let mut ts = 0;
        while ts <= 10_000 {
            rms_tx.send((ts, QUIET)).await.expect("send");
            ts += 100;
        }
        drop(rms_tx);

        let summary = task.await.expect("task");
        assert_eq!(summary, SilenceWatchSummary::default());
        assert!(
            seen.lock().expect("poisoned").is_empty(),
            "пауза длиной 10с не должна давать ни подсказки, ни стопа"
        );
    }
}
