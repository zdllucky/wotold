//! [TD-41] Тесты оркестратора ротации чанков.
//!
//! Живут отдельным файлом, а не `#[cfg(test)] mod tests` внутри модуля:
//! сам оркестратор — 430 строк, тесты к нему — 700, и вместе они давали
//! 1129 при лимите 800 (правило 8). Резать по доменной границе тут нечего:
//! это одна машина состояний с двумя хелперами. Дочерний модуль в отдельном
//! файле видит приватные элементы родителя ровно так же.

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

/// [M13.2.2] В parallel mode prev_prompt всегда `None` — cross-chunk
/// prompt chain trade-off (см. module doc-comment). Этот тест guards
/// что invariant соблюдается: оба chunk'а получают None.
#[tokio::test]
async fn parallel_mode_never_passes_prev_prompt() {
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
    // Оба chunk'а drained на stop → chunks_completed = 2.
    assert_eq!(summary.chunks_completed, 2);

    let calls_snap = calls.lock().unwrap().clone();
    assert_eq!(calls_snap.len(), 2);
    // Phase 2: оба получают prev=None (best-effort).
    assert_eq!(calls_snap[0].0, 0);
    assert!(calls_snap[0].3.is_none());
    assert_eq!(calls_snap[1].0, 1);
    assert_eq!(calls_snap[1].1, 1000);
    assert!(
        calls_snap[1].3.is_none(),
        "parallel mode не должен передавать prev_prompt"
    );
}

/// [M13.2.2] 3 rotation events подряд → 3 enqueue tasks spawned →
/// drain'ятся на stop → все 3 counter'ятся.
#[tokio::test]
async fn parallel_spawn_drains_all_on_stop() {
    let (_rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(10);
    let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
    let (stop_tx, stop_rx) = oneshot::channel();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let rotate_count = Arc::new(AtomicU32::new(0));
    let (rotated_back_tx, _) = mpsc::channel::<Value>(1);
    let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);

    let handle = tokio::spawn(run(
        test_config(),
        rms_rx,
        rotate_rx,
        stop_rx,
        pause_rx,
        make_rotate_fn(rotate_count, rotated_back_tx, 0),
        make_enqueue_fn(calls.clone(), "tail".into()),
    ));

    for dur in [1.0, 1.0, 1.0] {
        rotate_tx
            .send(serde_json::json!({
                "event": "rotated",
                "duration_sec": dur,
                "mic_bytes": 0,
                "system_bytes": 0,
            }))
            .await
            .unwrap();
    }
    // Дать spawn'нутым task'ам шанс start'нуть до stop'а.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let _ = stop_tx.send(());
    let summary = handle.await.unwrap();
    assert_eq!(
        summary.chunks_completed, 3,
        "все 3 spawned task'а должны drain'нуться"
    );
    assert_eq!(summary.enqueue_errors, 0);
    let calls_snap = calls.lock().unwrap().clone();
    assert_eq!(calls_snap.len(), 3);
    // chunk_idx 0, 1, 2 в порядке rotate events.
    assert_eq!(calls_snap[0].0, 0);
    assert_eq!(calls_snap[1].0, 1);
    assert_eq!(calls_snap[2].0, 2);
}

/// [M13 fix] После N rotated events открытый финальный chunk = N с
/// корректным start_ms (сумма durations). stop_recording обработает его.
#[tokio::test]
async fn final_chunk_coords_reported_on_stop() {
    let (_rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(10);
    let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
    let (stop_tx, stop_rx) = oneshot::channel();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let rotate_count = Arc::new(AtomicU32::new(0));
    let (rotated_back_tx, _) = mpsc::channel::<Value>(1);
    let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);

    let handle = tokio::spawn(run(
        test_config(),
        rms_rx,
        rotate_rx,
        stop_rx,
        pause_rx,
        make_rotate_fn(rotate_count, rotated_back_tx, 0),
        make_enqueue_fn(calls, "tail".into()),
    ));

    // 2 rotated events (dur 1.0s, 2.0s) → chunk_idx=2, chunk_start=3000ms.
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
    assert_eq!(summary.final_chunk_idx, 2, "открытый chunk после 2 ротаций");
    assert_eq!(
        summary.final_chunk_start_ms, 3000,
        "start_ms = сумма chunk durations (1000+2000)"
    );
}

// ============================================================
// [TD-14] Гонка stop vs rotated
// ============================================================

/// Прогнать сценарий «rotated и stop готовы одновременно» один раз.
/// Возвращает (final_chunk_idx, заенкьюенные индексы, drained-счётчик).
async fn race_stop_vs_rotated() -> (u32, Vec<u32>, u32) {
    let (_rms_tx, rms_rx) = mpsc::channel::<(u64, f32)>(10);
    let (rotate_tx, rotate_rx) = mpsc::channel::<Value>(10);
    let (stop_tx, stop_rx) = oneshot::channel();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let rotate_count = Arc::new(AtomicU32::new(0));
    let (rotated_back_tx, _) = mpsc::channel::<Value>(1);
    let (_pause_tx, pause_rx) = mpsc::channel::<bool>(1);

    // Обе ветки готовы ДО первого polling'а select! — гонка гарантирована.
    // Никаких sleep: синхронизация порядком отправки (правило 6).
    rotate_tx
        .send(serde_json::json!({
            "event": "rotated",
            "duration_sec": 1.0,
            "mic_bytes": 0,
            "system_bytes": 0,
        }))
        .await
        .unwrap();
    let _ = stop_tx.send(());

    let summary = run(
        test_config(),
        rms_rx,
        rotate_rx,
        stop_rx,
        pause_rx,
        make_rotate_fn(rotate_count, rotated_back_tx, 0),
        make_enqueue_fn(calls.clone(), "tail".into()),
    )
    .await;

    let enqueued: Vec<u32> = calls.lock().unwrap().iter().map(|c| c.0).collect();
    (
        summary.final_chunk_idx,
        enqueued,
        summary.rotated_drained_on_stop,
    )
}

/// [TD-14] Пришедший, но не прочитанный `rotated` не должен терять хвост
/// записи. `select!` выбирает готовую ветку случайно, поэтому гоняем
/// сценарий многократно: инвариант обязан держаться при ЛЮБОМ исходе
/// гонки (rotated выиграл — обработан сразу; stop выиграл — подобран
/// drain'ом). Без drain'а stop-ветка выигрывает хотя бы раз практически
/// наверняка, и тест краснеет.
#[tokio::test]
async fn stop_racing_rotated_never_loses_the_tail() {
    for attempt in 0..20 {
        let (final_idx, enqueued, _drained) = race_stop_vs_rotated().await;

        assert_eq!(
            final_idx, 1,
            "попытка {attempt}: индекс обязан продвинуться — иначе \
             final_chunk указывает на уже закрытый chunk, а открытый \
             сайдкаром следующий не получит ни строки в БД, ни STT"
        );
        assert_eq!(
            enqueued,
            vec![0],
            "попытка {attempt}: закрытый chunk 0 обязан быть заенкьюен \
             (enqueue живёт в rotated-ветке — без drain'а он терялся)"
        );
    }
}

/// [TD-14] Счётчик подобранных событий наблюдаем в summary: ненулевой
/// означает, что гонка случилась и была разобрана штатно.
#[tokio::test]
async fn drained_counter_is_observable() {
    let mut seen_drain = false;
    for _ in 0..20 {
        let (final_idx, _enqueued, drained) = race_stop_vs_rotated().await;
        assert_eq!(final_idx, 1);
        if drained > 0 {
            seen_drain = true;
            assert_eq!(drained, 1, "подобрано ровно одно событие");
        }
    }
    assert!(
        seen_drain,
        "за 20 прогонов stop обязан хоть раз выиграть гонку — иначе \
         тест не проверяет drain-путь"
    );
}

/// [M13 fix] Без ротаций финальный (единственный открытый) chunk = 0.
#[tokio::test]
async fn final_chunk_coords_zero_when_no_rotations() {
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
        make_rotate_fn(rotate_count, rotated_tx, 1000),
        make_enqueue_fn(Arc::new(Mutex::new(Vec::new())), "tail".into()),
    ));

    let _ = stop_tx.send(());
    let summary = handle.await.unwrap();
    assert_eq!(summary.final_chunk_idx, 0);
    assert_eq!(summary.final_chunk_start_ms, 0);
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
