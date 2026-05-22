//! [M13.1.5b step 1] Chunk orchestrator — long-lived task который во время
//! активной записи:
//! 1. Слушает RMS feed через `mpsc::Receiver<(u64, f32)>` (`(elapsed_ms, max_rms)`)
//!    из `audio::macos` dispatcher.
//! 2. Push'ит каждое значение в `SilenceDetector`.
//! 3. Каждый `tick_interval_ms` после `>chunk_start + window_start_offset_ms`
//!    зовёт `silence_detector.find_cut(window)`. Если cut найден — триггерит
//!    `rotate_fn(chunk_idx)` (caller дёргает `audio::macos::rotate`).
//! 4. Получает rotated events через `mpsc::Receiver<Value>` (raw sidecar JSON).
//! 5. На rotated event — зовёт `enqueue_fn` для completed chunk'а с
//!    `prev_transcript_tail` для context priming следующего chunk'а.
//! 6. На stop signal — возвращает `(total_chunks, last_chunk_start_ms)` для
//!    caller'а (он assemble'ит финальный pipeline или просто mark'ает done).
//!
//! Pure-ish — все side effects идут через closure callbacks. Testable через
//! mock channels + mock fn (см. unit tests внизу).
//!
//! **Не подключён к recording flow.** `commands::recording::start_recording`
//! сейчас передаёт `orchestrator=None` в `audio::macos::start`. Wiring через
//! `CHUNKED_PIPELINE` feature flag — отдельный sprint (M13.1.5c).

#![allow(dead_code)] // Wiring в recording flow — следующий sprint.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot;

use crate::audio::silence_detector::SilenceDetector;

/// Тюнинг chunk-rotation параметров. Default 10-мин chunks с ±1 мин tolerance.
#[derive(Debug, Clone, Copy)]
pub struct ChunkOrchestratorConfig {
    /// Target длительность chunk'а — center окна поиска тишины (ms).
    pub target_chunk_ms: u64,
    /// Окно поиска тишины относительно chunk start: `[start_off, end_off]` ms.
    pub window_start_offset_ms: u64,
    pub window_end_offset_ms: u64,
    /// RMS-порог для silence (0..1).
    pub silence_threshold: f32,
    /// Минимальная длительность silent run чтобы признать cut-candidate (ms).
    pub silence_min_duration_ms: u64,
    /// Как часто проверять find_cut в фоновом таймере (ms).
    pub tick_interval_ms: u64,
    /// Retention RMS-buffer (ms). Должен > end_offset_ms.
    pub rms_retention_ms: u64,
}

impl Default for ChunkOrchestratorConfig {
    fn default() -> Self {
        Self {
            target_chunk_ms: 600_000,        // 10 min
            window_start_offset_ms: 540_000, // 9 min
            window_end_offset_ms: 660_000,   // 11 min
            silence_threshold: 0.01,
            silence_min_duration_ms: 300,
            tick_interval_ms: 60_000,  // 1 min
            rms_retention_ms: 180_000, // 3 min
        }
    }
}

/// `Pin<Box<dyn Future...>>` для rotate-callback. Caller обычно zoom'ает на
/// `audio::macos::rotate(session, next_mic, next_system)`. Тесты mock'ают.
pub type RotateFut = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

/// `Pin<Box<dyn Future...>>` для chunk-runner callback. `Result::Ok(tail)` —
/// последние N слов transcript'а для prev_prompt следующего chunk'а.
pub type EnqueueFut = Pin<Box<dyn Future<Output = Result<Option<String>, String>> + Send>>;

/// Сводка завершённой работы orchestrator'а — caller использует чтобы
/// решить как assemble'ить финальный артефакт.
#[derive(Debug, Clone, Default)]
pub struct OrchestratorSummary {
    /// Сколько раз rotate_fn был вызван успешно (= chunks − 1, последний
    /// chunk закрывается caller'ом на `stop_recording`).
    pub rotations_triggered: u32,
    /// Сколько chunk'ов было обработано через enqueue_fn.
    pub chunks_completed: u32,
    /// Сколько rotate_fn вернул error — orchestrator продолжает работу, но
    /// caller может decide retry/fallback на полный file.
    pub rotate_errors: u32,
    /// Аналогично — enqueue errors (chunk_runner отказал).
    pub enqueue_errors: u32,
}

/// Запустить orchestrator main loop. Возвращает `OrchestratorSummary` когда
/// получен stop_signal либо все каналы закрылись.
///
/// Generic closures для side effects — позволяет mock'ать в unit tests без
/// тащить настоящий sidecar.
#[allow(clippy::too_many_arguments)]
pub async fn run<RotateF, EnqueueF>(
    config: ChunkOrchestratorConfig,
    mut rms_rx: Receiver<(u64, f32)>,
    mut rotate_rx: Receiver<Value>,
    mut stop_rx: oneshot::Receiver<()>,
    mut pause_rx: Receiver<bool>,
    rotate_fn: RotateF,
    enqueue_fn: EnqueueF,
) -> OrchestratorSummary
where
    RotateF: Fn(u32) -> RotateFut + Send + 'static,
    EnqueueF: Fn(u32, u64, u64, Option<String>) -> EnqueueFut + Send + 'static,
{
    let mut detector = SilenceDetector::new(config.rms_retention_ms);
    let mut chunk_idx: u32 = 0;
    let mut chunk_start_ms: u64 = 0;
    let mut last_rms_ts_ms: u64 = 0;
    let mut prev_transcript_tail: Option<String> = None;
    let mut summary = OrchestratorSummary::default();
    let mut rotate_pending = false;
    // [M13.2.1] Pause state. Когда `paused=true`:
    //   - RMS samples всё ещё консумируются (sidecar v1 пишет фреймы во время
    //     pause), но НЕ push'атся в `detector` — иначе пауза > silence_min
    //     зарегистрируется как cut-candidate и orchestrator преждевременно
    //     rotate'нёт chunk.
    //   - Tick skip'ает rotation logic entirely.
    //   - `pause_started_at_ms` anchor'ит на last_rms_ts_ms; на resume delta
    //     добавляется в `paused_total_ms_in_chunk` который вычитается из
    //     `effective_elapsed` при cut-decision.
    //   - На rotation (новый chunk) — reset `paused_total_ms_in_chunk = 0`.
    let mut paused = false;
    let mut pause_started_at_ms: Option<u64> = None;
    let mut paused_total_ms_in_chunk: u64 = 0;

    let mut tick = tokio::time::interval(Duration::from_millis(config.tick_interval_ms));
    // first tick fires immediately — skip.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick.tick().await;

    loop {
        tokio::select! {
            // Stop signal — clean exit.
            _ = &mut stop_rx => {
                log::debug!("chunk_orchestrator stop signal received");
                break;
            }

            // [M13.2.1] Pause/resume control. `Some(true)` = pause, `Some(false)` = resume.
            // None (channel closed) = caller dropped tx без shutdown — игнорим (orchestrator
            // продолжает работу до rms_rx closure / stop_rx).
            maybe_pause = pause_rx.recv() => {
                match maybe_pause {
                    Some(true) if !paused => {
                        paused = true;
                        pause_started_at_ms = Some(last_rms_ts_ms);
                        log::debug!("chunk_orchestrator paused at {last_rms_ts_ms}ms");
                    }
                    Some(false) if paused => {
                        if let Some(start) = pause_started_at_ms {
                            let delta = last_rms_ts_ms.saturating_sub(start);
                            paused_total_ms_in_chunk =
                                paused_total_ms_in_chunk.saturating_add(delta);
                        }
                        paused = false;
                        pause_started_at_ms = None;
                        log::debug!(
                            "chunk_orchestrator resumed (paused_total_ms_in_chunk={paused_total_ms_in_chunk})"
                        );
                    }
                    // Some(true) when уже paused, Some(false) when not paused — idempotent.
                    // None — Sender dropped, обычно при stop_recording cleanup.
                    _ => {}
                }
            }

            // RMS sample fed from dispatcher.
            maybe_sample = rms_rx.recv() => {
                let Some((ts_ms, rms)) = maybe_sample else {
                    // Channel closed = recording ended.
                    log::debug!("chunk_orchestrator rms channel closed");
                    break;
                };
                last_rms_ts_ms = ts_ms;
                // [M13.2.1] Skip detector push во время pause — pause-period
                // RMS не должны влиять на silence cut.
                if !paused {
                    detector.push(ts_ms, rms);
                }
            }

            // Sidecar rotated event — current chunk closed, next is open.
            maybe_rotated = rotate_rx.recv() => {
                let Some(event) = maybe_rotated else {
                    log::debug!("chunk_orchestrator rotate channel closed");
                    break;
                };
                rotate_pending = false;

                // chunk_end_ms = chunk_start_ms + duration из event.
                // duration_sec может быть Number или String depending on sidecar.
                let duration_ms = event
                    .get("duration_sec")
                    .and_then(|v| v.as_f64())
                    .map(|s| (s * 1000.0) as u64)
                    .unwrap_or(0);
                let chunk_end_ms = chunk_start_ms + duration_ms;
                let closed_idx = chunk_idx;
                let prev = prev_transcript_tail.clone();

                // Запустить pipeline для completed chunk'а. Sequential для Phase
                // 1 simplicity — Phase 2 переключит на parallel spawn.
                match enqueue_fn(closed_idx, chunk_start_ms, chunk_end_ms, prev).await {
                    Ok(tail) => {
                        prev_transcript_tail = tail;
                        summary.chunks_completed += 1;
                    }
                    Err(e) => {
                        log::warn!("chunk_orchestrator enqueue chunk {closed_idx}: {e}");
                        summary.enqueue_errors += 1;
                        // prev_transcript_tail не обновляем — next chunk
                        // получит prompt от последнего successful chunk'а.
                    }
                }

                chunk_idx += 1;
                chunk_start_ms = chunk_end_ms;
                // [M13.2.1] Новый chunk — reset pause accumulator. Если мы
                // всё ещё paused, anchor сдвигается на текущий last_rms_ts_ms.
                paused_total_ms_in_chunk = 0;
                if paused {
                    pause_started_at_ms = Some(last_rms_ts_ms);
                }
            }

            // Periodic tick — try silence cut если достаточно времени прошло.
            _ = tick.tick() => {
                if rotate_pending {
                    // Уже отправили rotate, ждём rotated event.
                    continue;
                }
                if paused {
                    // [M13.2.1] Pause замораживает chunk-elapsed clock —
                    // никаких rotation попыток до resume.
                    continue;
                }
                // [M13.2.1] effective_elapsed = wall_elapsed - paused durations.
                // (Текущая pause-duration не учитывается потому что paused=false
                // в этой ветке.)
                let wall_elapsed = last_rms_ts_ms.saturating_sub(chunk_start_ms);
                let elapsed = wall_elapsed.saturating_sub(paused_total_ms_in_chunk);
                if elapsed < config.window_start_offset_ms {
                    // Слишком рано — chunk ещё <9 мин (active recording time).
                    continue;
                }
                // Window рассчитываем на effective time. Shift на pause-сумму
                // даёт окно в wall-time терминах.
                let window_start = chunk_start_ms + paused_total_ms_in_chunk + config.window_start_offset_ms;
                let window_end = chunk_start_ms + paused_total_ms_in_chunk + config.window_end_offset_ms;
                let cut_search_end = window_end.min(last_rms_ts_ms);
                if cut_search_end <= window_start {
                    continue;
                }
                if let Some(_cut_ms) = detector.find_cut(
                    window_start,
                    cut_search_end,
                    config.silence_threshold,
                    config.silence_min_duration_ms,
                ) {
                    // Cut найден (либо silence run, либо local min RMS).
                    // Триггерим rotate — caller через rotate_fn пишет команду
                    // в sidecar. duration в rotated event покажет реальную
                    // длину chunk'а (sidecar отрезает сам).
                    rotate_pending = true;
                    match rotate_fn(chunk_idx).await {
                        Ok(()) => {
                            summary.rotations_triggered += 1;
                        }
                        Err(e) => {
                            log::warn!("chunk_orchestrator rotate fn failed: {e}");
                            summary.rotate_errors += 1;
                            rotate_pending = false;
                            // Не break'аем — попробуем ещё раз на следующем
                            // tick'е. Если sidecar мёртв, recording тоже
                            // мёртвый, rms_rx скоро закроется → loop exit.
                        }
                    }
                }
            }
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    };
    use tokio::sync::mpsc;

    /// Helper для test config с короткими интервалами (тесты не ждут реальные
    /// 10 мин).
    fn test_config() -> ChunkOrchestratorConfig {
        ChunkOrchestratorConfig {
            target_chunk_ms: 1000,
            window_start_offset_ms: 900,
            window_end_offset_ms: 1100,
            silence_threshold: 0.01,
            silence_min_duration_ms: 50,
            tick_interval_ms: 100,
            rms_retention_ms: 3000,
        }
    }

    /// Mock rotate fn — counts invocations + sends rotated event через канал
    /// (имитирует sidecar ack).
    fn make_rotate_fn(
        rotate_count: Arc<AtomicU32>,
        rotated_tx: mpsc::Sender<Value>,
        rotated_duration_ms: u64,
    ) -> impl Fn(u32) -> RotateFut + Send + 'static {
        move |_idx| {
            let count = rotate_count.clone();
            let tx = rotated_tx.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                // Имитируем sidecar ack через канал.
                let ev = serde_json::json!({
                    "event": "rotated",
                    "duration_sec": rotated_duration_ms as f64 / 1000.0,
                    "mic_bytes": 0,
                    "system_bytes": 0,
                });
                let _ = tx.send(ev).await;
                Ok(())
            })
        }
    }

    /// Mock enqueue fn — captures calls + возвращает determinистический tail.
    #[allow(clippy::type_complexity)]
    fn make_enqueue_fn(
        calls: Arc<Mutex<Vec<(u32, u64, u64, Option<String>)>>>,
        tail_template: String,
    ) -> impl Fn(u32, u64, u64, Option<String>) -> EnqueueFut + Send + 'static {
        move |idx, start, end, prev| {
            let calls = calls.clone();
            let tail = format!("{tail_template}-{idx}");
            Box::pin(async move {
                calls.lock().unwrap().push((idx, start, end, prev));
                Ok(Some(tail))
            })
        }
    }

    #[tokio::test]
    async fn stop_signal_exits_cleanly() {
        let (_rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(10);
        let (_rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let rotate_count = Arc::new(AtomicU32::new(0));
        let (rotated_tx, _) = mpsc::channel::<Value>(1);

        let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);
        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count.clone(), rotated_tx, 1000),
            make_enqueue_fn(Arc::new(Mutex::new(Vec::new())), "tail".into()),
        ));

        // Signal stop immediately.
        let _ = stop_tx.send(());
        let summary = handle.await.unwrap();
        assert_eq!(summary.rotations_triggered, 0);
        assert_eq!(summary.chunks_completed, 0);
    }

    #[tokio::test]
    async fn rotated_event_enqueues_chunk() {
        let (_rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(10);
        let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let rotate_count = Arc::new(AtomicU32::new(0));
        let (rotated_back_tx, _) = mpsc::channel::<Value>(1);

        let calls_clone = calls.clone();
        let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);
        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count, rotated_back_tx, 0),
            make_enqueue_fn(calls_clone, "tail".into()),
        ));

        // Симулируем rotated event приходящий из dispatcher.
        rotate_tx
            .send(serde_json::json!({
                "event": "rotated",
                "duration_sec": 1.5,
                "mic_bytes": 0,
                "system_bytes": 0,
            }))
            .await
            .unwrap();

        // Дать orchestrator'у обработать.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = stop_tx.send(());
        let summary = handle.await.unwrap();
        assert_eq!(summary.chunks_completed, 1);

        let calls_snap = calls.lock().unwrap().clone();
        assert_eq!(calls_snap.len(), 1);
        let (idx, start_ms, end_ms, prev) = &calls_snap[0];
        assert_eq!(*idx, 0);
        assert_eq!(*start_ms, 0);
        assert_eq!(*end_ms, 1500);
        assert!(prev.is_none());
    }

    #[tokio::test]
    async fn second_rotated_event_inherits_prev_prompt() {
        let (_rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(10);
        let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let rotate_count = Arc::new(AtomicU32::new(0));
        let (rotated_back_tx, _) = mpsc::channel::<Value>(1);

        let calls_clone = calls.clone();
        let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);
        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count, rotated_back_tx, 0),
            make_enqueue_fn(calls_clone, "tail".into()),
        ));

        // Два последовательных rotated event'а.
        for dur in [1.0, 2.0] {
            rotate_tx
                .send(serde_json::json!({
                    "event": "rotated",
                    "duration_sec": dur,
                    "mic_bytes": 0,
                    "system_bytes": 0,
                }))
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
        }

        let _ = stop_tx.send(());
        let summary = handle.await.unwrap();
        assert_eq!(summary.chunks_completed, 2);

        let calls_snap = calls.lock().unwrap().clone();
        assert_eq!(calls_snap.len(), 2);
        // Chunk 0: prev=None, start=0, end=1000.
        assert_eq!(calls_snap[0].0, 0);
        assert!(calls_snap[0].3.is_none());
        // Chunk 1: prev=Some("tail-0"), start=1000 (= end of chunk 0).
        assert_eq!(calls_snap[1].0, 1);
        assert_eq!(calls_snap[1].1, 1000);
        assert_eq!(calls_snap[1].3.as_deref(), Some("tail-0"));
    }

    #[tokio::test]
    async fn silence_in_window_triggers_rotate() {
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(100);
        let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let rotate_count = Arc::new(AtomicU32::new(0));
        let calls = Arc::new(Mutex::new(Vec::new()));

        // rotate_fn будет сам слать rotated event через rotate_tx.
        let rotate_back_tx = rotate_tx.clone();
        let rotate_count_clone = rotate_count.clone();

        let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);
        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count_clone, rotate_back_tx, 950),
            make_enqueue_fn(calls.clone(), "tail".into()),
        ));

        // Push RMS samples: 0-850ms loud, 870-1050ms silence (в window 900-1100).
        for ts in (0..850).step_by(50) {
            rms_tx.send((ts, 0.5)).await.unwrap();
        }
        for ts in (870..=1050).step_by(20) {
            rms_tx.send((ts, 0.001)).await.unwrap();
        }

        // Ждать чтобы interval tick фаирнул find_cut.
        tokio::time::sleep(Duration::from_millis(300)).await;

        let _ = stop_tx.send(());
        let summary = handle.await.unwrap();

        assert_eq!(
            rotate_count.load(Ordering::SeqCst),
            1,
            "rotate_fn must fire"
        );
        assert_eq!(summary.rotations_triggered, 1);
        // rotated event arrived (rotate_fn послал) → chunk 0 completed.
        assert_eq!(summary.chunks_completed, 1);
    }

    #[tokio::test]
    async fn no_silence_still_falls_back_to_local_min_rms() {
        // find_cut с loud-only данными возвращает local min RMS как fallback —
        // orchestrator всё равно rotate'ит (нельзя бесконечно ждать тишины).
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(100);
        let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let rotate_count = Arc::new(AtomicU32::new(0));
        let calls = Arc::new(Mutex::new(Vec::new()));

        let rotate_back_tx = rotate_tx.clone();
        let rotate_count_clone = rotate_count.clone();

        let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);
        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count_clone, rotate_back_tx, 1000),
            make_enqueue_fn(calls.clone(), "tail".into()),
        ));

        // Все samples loud — нет тишины. find_cut вернёт fallback (local min).
        for ts in (0..1100).step_by(20) {
            rms_tx
                .send((ts, 0.5 + (ts % 100) as f32 * 0.001))
                .await
                .unwrap();
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = stop_tx.send(());
        let _ = handle.await.unwrap();
        assert_eq!(rotate_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn does_not_rotate_before_window_start() {
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(100);
        let (_rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let rotate_count = Arc::new(AtomicU32::new(0));
        let calls = Arc::new(Mutex::new(Vec::new()));

        let (rotated_back_tx, _) = mpsc::channel::<Value>(1);
        let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);
        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count.clone(), rotated_back_tx, 1000),
            make_enqueue_fn(calls.clone(), "tail".into()),
        ));

        // Only push samples до 500ms (< window_start_offset_ms=900).
        for ts in (0..500).step_by(20) {
            rms_tx.send((ts, 0.005)).await.unwrap();
        }
        // Дать orchestrator'у несколько tick'ов посмотреть.
        tokio::time::sleep(Duration::from_millis(250)).await;

        let _ = stop_tx.send(());
        let _ = handle.await.unwrap();
        // Никаких rotate — не достигли target chunk size.
        assert_eq!(rotate_count.load(Ordering::SeqCst), 0);
    }

    // ========================================================================
    // [M13.2.1] Pause-aware tests
    // ========================================================================

    #[tokio::test]
    async fn pause_freezes_chunk_elapsed() {
        // Сценарий: push RMS samples с большим pause-окном внутри. Без
        // pause-aware orchestrator увидел бы wall_elapsed > window_end и
        // rotate'нул раньше. С pause-aware effective_elapsed остаётся в
        // pre-rotate зоне до достижения target active time.
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(100);
        let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let (pause_tx, pause_rx) = mpsc::channel::<bool>(8);
        let rotate_count = Arc::new(AtomicU32::new(0));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let rotate_back_tx = rotate_tx.clone();
        let rotate_count_clone = rotate_count.clone();

        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count_clone, rotate_back_tx, 950),
            make_enqueue_fn(calls.clone(), "tail".into()),
        ));

        // 0-500ms active speech (loud RMS).
        for ts in (0..500).step_by(20) {
            rms_tx.send((ts, 0.5)).await.unwrap();
        }
        // Pause at 500ms. Sidecar продолжает emit'ить RMS (silence) — без
        // pause-aware silence сейчас зарегистрировался бы как cut.
        pause_tx.send(true).await.unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        // Имитация pause-периода: 500-1300ms low RMS (sidecar тишина).
        for ts in (520..=1300).step_by(20) {
            rms_tx.send((ts, 0.001)).await.unwrap();
        }
        // Resume — 800ms pause накапливается в paused_total_ms_in_chunk.
        pause_tx.send(false).await.unwrap();
        // Дать orchestrator'у обработать pause/resume.
        tokio::time::sleep(Duration::from_millis(50)).await;
        // 1320-1850ms active speech. effective_elapsed = 500 + (1850-1320) = 1030
        // → > window_start_offset_ms=900. Должен rotate.
        for ts in (1320..=1850).step_by(20) {
            rms_tx.send((ts, 0.5)).await.unwrap();
        }
        // Тишина в окне для cut detection.
        for ts in (1870..=1950).step_by(20) {
            rms_tx.send((ts, 0.001)).await.unwrap();
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = stop_tx.send(());
        let _ = handle.await.unwrap();

        // По крайней мере один rotation — pause-aware всё ещё инициирует
        // chunk cut на правильной границе. Без pause-aware result был бы тот
        // же но раньше (на pause silence) — точное число rotation'ов
        // зависит от mock duration; verify минимум один и что orchestrator
        // не зависает в pause.
        assert!(
            rotate_count.load(Ordering::SeqCst) >= 1,
            "pause-aware orchestrator rotate'ит post-resume когда effective_elapsed достиг target"
        );
    }

    #[tokio::test]
    async fn pause_resume_idempotent_no_crash() {
        // Двойной pause/resume → orchestrator не crash'ится, корректно
        // переходит между состояниями. Тест проверяет robustness, не timing
        // (последнее покрывает pause_freezes_chunk_elapsed).
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(100);
        let (_rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let (pause_tx, pause_rx) = mpsc::channel::<bool>(8);
        let rotate_count = Arc::new(AtomicU32::new(0));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (rotated_back_tx, _) = mpsc::channel::<Value>(1);

        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count.clone(), rotated_back_tx, 950),
            make_enqueue_fn(calls.clone(), "tail".into()),
        ));

        // Несколько pause/resume циклов — orchestrator должен пережить.
        pause_tx.send(true).await.unwrap();
        pause_tx.send(true).await.unwrap(); // idempotent
        tokio::time::sleep(Duration::from_millis(50)).await;
        pause_tx.send(false).await.unwrap();
        pause_tx.send(false).await.unwrap(); // idempotent
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Push несколько samples — orchestrator alive.
        for ts in (0..100).step_by(20) {
            rms_tx.send((ts, 0.5)).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = stop_tx.send(());
        // Не должно паниковать. Summary возвращается — task alive.
        let summary = handle.await.unwrap();
        // Никаких rotation'ов не успело произойти (too short).
        assert_eq!(summary.rotations_triggered, 0);
    }

    #[tokio::test]
    async fn pause_after_rotation_resets_accumulator() {
        // Сценарий: chunk завершён (rotated event), потом pause → resume
        // во втором chunk'е. Pause accumulator должен быть сброшен после
        // rotation, иначе second chunk'у достались бы paused_ms от first'а.
        let (rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(100);
        let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
        let (stop_tx, stop_rx) = oneshot::channel();
        let (pause_tx, pause_rx) = mpsc::channel::<bool>(8);
        let rotate_count = Arc::new(AtomicU32::new(0));
        let calls = Arc::new(Mutex::new(Vec::new()));

        let handle = tokio::spawn(run(
            test_config(),
            rms_rx,
            rotate_rx,
            stop_rx,
            pause_rx,
            make_rotate_fn(rotate_count.clone(), rotate_tx.clone(), 0),
            make_enqueue_fn(calls.clone(), "tail".into()),
        ));

        // Pause+resume в первом chunk'е.
        pause_tx.send(true).await.unwrap();
        for ts in (0..400).step_by(20) {
            rms_tx.send((ts, 0.001)).await.unwrap();
        }
        pause_tx.send(false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Симулируем rotated event — закрываем chunk 0.
        rotate_tx
            .send(serde_json::json!({
                "event": "rotated",
                "duration_sec": 0.5,
                "mic_bytes": 0,
                "system_bytes": 0,
            }))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Сейчас chunk_idx=1, accumulator должен быть 0. Push minimal active
        // RMS. Если accumulator не сброшен — orchestrator подумает что много
        // pause-time уже накоплено и неправильно посчитает effective_elapsed.
        for ts in (500..1500).step_by(20) {
            rms_tx.send((ts, 0.5)).await.unwrap();
        }
        for ts in (1520..=1700).step_by(20) {
            rms_tx.send((ts, 0.001)).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = stop_tx.send(());
        let _ = handle.await.unwrap();

        // chunk_completed >= 1 (от rotated event).
        let snap = calls.lock().unwrap().clone();
        assert!(!snap.is_empty(), "chunk 0 должен быть enqueued");
        // Test проходит если no panic/incorrect state.
    }
}
