//! [M13.1.5b step 1] Chunk orchestrator — long-lived task который во время
//! активной записи:
//! 1. Слушает RMS feed через `mpsc::Receiver<(u64, f32)>` (`(elapsed_ms, max_rms)`)
//!    из `audio::macos` dispatcher.
//! 2. Push'ит каждое значение в `SilenceDetector`.
//! 3. Каждый `tick_interval_ms` после `>chunk_start + window_start_offset_ms`
//!    зовёт `silence_detector.find_cut(window)`. Если cut найден — триггерит
//!    `rotate_fn(chunk_idx)` (caller дёргает `audio::macos::rotate`).
//! 4. Получает rotated events через `mpsc::Receiver<Value>` (raw sidecar JSON).
//! 5. На rotated event — **спавнит** `enqueue_fn` task через `tokio::spawn`
//!    для completed chunk'а. Phase 2 = parallel pipelining: chunk N STT идёт
//!    параллельно с записью chunk N+1.
//! 6. На stop signal / rms_rx closure — drain'ит все pending enqueue handles
//!    с timeout'ом, обновляет `summary` поштучно по результатам, возвращает.
//!
//! Pure-ish — все side effects идут через closure callbacks. Testable через
//! mock channels + mock fn (см. unit tests внизу).
//!
//! Wired в recording flow через `CHUNKED_PIPELINE` feature flag (M13.1.5c).
//! Pause-aware (M13.2.1) — `pause_rx` arm замораживает rotation timer.
//!
//! **Phase 2 trade-off (M13.2.2)** — параллельный spawn ломает cross-chunk
//! prompt chain: `prev_transcript_tail` всегда `None` в parallel mode, потому
//! что chunk N+1 стартует до того, как chunk N закончит STT. Whisper всё равно
//! сбрасывает context на каждом cut'е, так что потеря качества ~1% на стыках —
//! приемлемая цена за обещанный 6-10× speed-up. True chain потребовал бы
//! re-serialization Phase 1, что съело бы выигрыш pipelining.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::audio::silence_detector::SilenceDetector;

/// [M13.2.2] Max wait для drain pending enqueue tasks на stop. Phase 2 STT
/// для одного chunk'а на 10-мин ауд занимает 30s-3min (Balanced preset),
/// 5 мин — generous safety net.
const DRAIN_TIMEOUT_PER_TASK: Duration = Duration::from_secs(300);

/// Тюнинг chunk-rotation параметров. Default 10-мин chunks с ±1 мин tolerance.
#[derive(Debug, Clone, Copy)]
pub struct ChunkOrchestratorConfig {
    /// Target длительность chunk'а — center окна поиска тишины (ms).
    /// Документационный — фактические границы окна задаются через
    /// window_start_offset_ms/window_end_offset_ms. Оставлен для clarity.
    #[allow(dead_code)]
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
    /// [TD-14] Сколько `rotated`-событий подобрал drain ПОСЛЕ выхода из loop'а.
    /// Не ошибка: `select!` опрашивает готовые ветки в случайном порядке, и
    /// Stop мог выиграть у уже пришедшего `rotated`. Ненулевое значение
    /// означает, что гонка случилась и была корректно разобрана.
    pub rotated_drained_on_stop: u32,
    /// [M13 fix] Индекс chunk'а, ещё **открытого** на момент выхода из loop'а —
    /// его rotated event так и не пришёл, значит он никогда не был enqueue'нут.
    /// `stop_recording` обязан обработать его после `audio_macos::stop`
    /// (финальный ≤10-мин сегмент). Для zero-rotation записи = 0.
    pub final_chunk_idx: u32,
    /// [M13 fix] `start_ms` открытого финального chunk'а (offset от начала
    /// записи). Для chunk 0 = 0.
    pub final_chunk_start_ms: u64,
    /// [M13 fix] Последний известный RMS-timestamp = provisional `end_ms`
    /// финального chunk'а. `stop_recording` предпочтёт authoritative
    /// wall-clock total, но это разумный fallback.
    pub final_chunk_last_ts_ms: u64,
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
    let mut summary = OrchestratorSummary::default();
    let mut rotate_pending = false;
    // [M13.2.2] Pending enqueue tasks от tokio::spawn. Drain на stop / EOF.
    let mut pending_handles: Vec<JoinHandle<Result<Option<String>, String>>> = Vec::new();
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
                apply_rotated(
                    &event,
                    &enqueue_fn,
                    &mut chunk_idx,
                    &mut chunk_start_ms,
                    &mut paused_total_ms_in_chunk,
                    &mut pause_started_at_ms,
                    paused,
                    last_rms_ts_ms,
                    &mut pending_handles,
                );
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
                // [M13 review fix] Cap paused_total_ms_in_chunk на
                // window_end_offset_ms чтобы экстремально длинная пауза
                // (> target_chunk_ms) не shift'ила window так далеко в
                // future что cut_search_end <= window_start навсегда —
                // тогда chunk never rotate'ит. Capped pause всё ещё
                // freezes timer effectively (orchestrator continues
                // active accounting), но не блокирует rotation forever.
                let capped_paused = paused_total_ms_in_chunk.min(config.window_end_offset_ms);
                // [M13.2.1] effective_elapsed = wall_elapsed - paused durations.
                // (Текущая pause-duration не учитывается потому что paused=false
                // в этой ветке.)
                let wall_elapsed = last_rms_ts_ms.saturating_sub(chunk_start_ms);
                let elapsed = wall_elapsed.saturating_sub(capped_paused);
                if elapsed < config.window_start_offset_ms {
                    // Слишком рано — chunk ещё <9 мин (active recording time).
                    continue;
                }
                // Window рассчитываем на effective time. Shift на pause-сумму
                // даёт окно в wall-time терминах.
                let window_start = chunk_start_ms + capped_paused + config.window_start_offset_ms;
                let window_end = chunk_start_ms + capped_paused + config.window_end_offset_ms;
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

    // [TD-14] Разобрать `rotated`, пришедшие но не прочитанные к моменту
    // выхода из loop'а. `select!` опрашивает готовые ветки в случайном
    // порядке: если Stop пришёл через секунды после ротации, событие могло
    // остаться в канале. Оно несёт ДВЕ вещи, и обе терялись:
    //   1. enqueue закрытого chunk'а (он делается только в rotated-ветке);
    //   2. инкремент `chunk_idx` — без него `final_chunk_idx` ниже указывал
    //      на УЖЕ ЗАКРЫТЫЙ chunk, а открытый сайдкаром следующий не получал
    //      ни строки в БД, ни STT. Аудио на диске цело (merger сканирует ФС),
    //      но последние секунды разговора пропадали из транскрипта молча.
    // Обязано идти ДО подсчёта final_chunk_* — те читают `chunk_idx`.
    while let Ok(event) = rotate_rx.try_recv() {
        apply_rotated(
            &event,
            &enqueue_fn,
            &mut chunk_idx,
            &mut chunk_start_ms,
            &mut paused_total_ms_in_chunk,
            &mut pause_started_at_ms,
            paused,
            last_rms_ts_ms,
            &mut pending_handles,
        );
        summary.rotated_drained_on_stop += 1;
    }
    if summary.rotated_drained_on_stop > 0 {
        log::info!(
            "chunk_orchestrator: подобрано {} rotated-событий после stop (гонка select!)",
            summary.rotated_drained_on_stop
        );
    }

    // [M13 fix] Запомнить координаты открытого (не-rotated) финального chunk'а.
    // Эти локалы уже отслеживаются: `chunk_idx` = текущий открытый chunk,
    // `chunk_start_ms` = его начало, `last_rms_ts_ms` = последний RMS.
    // `stop_recording` обработает его после финализации WAV в sidecar.
    summary.final_chunk_idx = chunk_idx;
    summary.final_chunk_start_ms = chunk_start_ms;
    summary.final_chunk_last_ts_ms = last_rms_ts_ms;

    // [M13.2.2] Drain pending parallel enqueue tasks. Каждый — c timeout'ом
    // на случай зависшего whisper-cli. Counters обновляются поштучно.
    let drained = drain_pending(pending_handles, &mut summary).await;
    if drained > 0 {
        log::info!("chunk_orchestrator drained {drained} pending enqueue tasks");
    }

    summary
}

/// [TD-14] Обработка одного `rotated`-события: заенкьюить закрытый chunk и
/// продвинуть координаты открытого.
///
/// Вынесено из ветки `select!`, потому что drain после выхода из loop'а обязан
/// делать РОВНО ТО ЖЕ САМОЕ. Пока логика жила в одном месте, drain'а не было
/// вовсе и хвост записи терялся; дублировать её двумя копиями — верный способ
/// получить «одинаковый контракт, разная зрелость».
#[allow(clippy::too_many_arguments)]
fn apply_rotated<EnqueueF>(
    event: &Value,
    enqueue_fn: &EnqueueF,
    chunk_idx: &mut u32,
    chunk_start_ms: &mut u64,
    paused_total_ms_in_chunk: &mut u64,
    pause_started_at_ms: &mut Option<u64>,
    paused: bool,
    last_rms_ts_ms: u64,
    pending_handles: &mut Vec<JoinHandle<Result<Option<String>, String>>>,
) where
    EnqueueF: Fn(u32, u64, u64, Option<String>) -> EnqueueFut,
{
    // chunk_end_ms = chunk_start_ms + duration из event.
    // duration_sec может быть Number или String depending on sidecar.
    let duration_ms = event
        .get("duration_sec")
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64)
        .unwrap_or(0);
    let chunk_end_ms = *chunk_start_ms + duration_ms;
    let closed_idx = *chunk_idx;

    // [M13.2.2] Spawn enqueue_fn в отдельный task — chunk N STT идёт
    // параллельно с записью chunk N+1. prev_prompt всегда None в parallel
    // mode (cross-chunk prompt chain trade-off, см. module doc-comment).
    let fut = enqueue_fn(closed_idx, *chunk_start_ms, chunk_end_ms, None);
    pending_handles.push(tokio::spawn(fut));

    *chunk_idx += 1;
    *chunk_start_ms = chunk_end_ms;
    // [M13.2.1] Новый chunk — reset pause accumulator. Если мы всё ещё
    // paused, anchor сдвигается на текущий last_rms_ts_ms.
    *paused_total_ms_in_chunk = 0;
    if paused {
        *pause_started_at_ms = Some(last_rms_ts_ms);
    }
}

/// [M13.2.2] Await каждого pending JoinHandle с per-task timeout'ом.
/// Возвращает число drained handles (для логирования).
async fn drain_pending(
    handles: Vec<JoinHandle<Result<Option<String>, String>>>,
    summary: &mut OrchestratorSummary,
) -> usize {
    let count = handles.len();
    for handle in handles {
        match tokio::time::timeout(DRAIN_TIMEOUT_PER_TASK, handle).await {
            Ok(Ok(Ok(_tail))) => {
                summary.chunks_completed += 1;
            }
            Ok(Ok(Err(e))) => {
                log::warn!("chunk_orchestrator drain: enqueue task err: {e}");
                summary.enqueue_errors += 1;
            }
            Ok(Err(join_err)) => {
                log::warn!("chunk_orchestrator drain: task panicked / cancelled: {join_err}");
                summary.enqueue_errors += 1;
            }
            Err(_) => {
                log::warn!(
                    "chunk_orchestrator drain: enqueue task timeout ({:?})",
                    DRAIN_TIMEOUT_PER_TASK
                );
                summary.enqueue_errors += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod probes;
#[cfg(test)]
mod tests;
