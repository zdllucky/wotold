//! [M13.1.1] Silence-aware cut detector — pure модуль для определения
//! момента chunk-rotation в активной записи.
//!
//! # Зачем
//!
//! Резать аудио ровно в 10:00.000 порвёт слово на границе chunk'а. Whisper
//! может неверно интерпретировать обрывки. Решение — резать в естественной
//! паузе ±1 минута от target.
//!
//! # Алгоритм
//!
//! Буфер хранит `(timestamp_ms, rms)` точки. На каждой минуте >9 от старта
//! chunk'а:
//! 1. Искать contiguous run точек с `rms < threshold` длительностью ≥
//!    `min_duration_ms` в окне `[target_ms - tolerance, target_ms + tolerance]`.
//! 2. Если найдено — cut в середине самого длинного run'а.
//! 3. Иначе fallback — cut в точке с локальным минимумом RMS в окне.
//! 4. Если окно пустое — `None` (caller должен подождать ещё RMS точек).
//!
//! # Producer / consumer
//!
//! Producer — sidecar `audio:level` events ~10Hz. `push()` добавляет точку.
//! Consumer — orchestrator в recording flow каждую минуту вызывает `find_cut()`
//! как только текущая длительность chunk'а ≥ 9 минут.
//!
//! Note: `#[allow(dead_code)]` на pub API — orchestration wiring добавится
//! в следующем M13.1 sprint'е. Тесты гарантируют что код корректен.

#![allow(dead_code)]

use std::collections::VecDeque;

/// Pure-state container для RMS-точек. НЕ thread-safe — caller обёртывает в
/// Mutex если нужно (recording loop держит на одной задаче).
#[derive(Debug, Default)]
pub struct SilenceDetector {
    /// `(timestamp_ms, rms)` points, append-only ordered by timestamp.
    samples: VecDeque<(u64, f32)>,
    /// Сколько последних мс хранить (drop older). Для 10-мин chunks с
    /// 2-мин tolerance окном нужно ~3 мин буфера = 180_000 ms.
    retention_ms: u64,
}

impl SilenceDetector {
    pub fn new(retention_ms: u64) -> Self {
        Self {
            samples: VecDeque::new(),
            retention_ms,
        }
    }

    /// Добавить RMS точку. Дроп старых самплов за пределами retention.
    pub fn push(&mut self, timestamp_ms: u64, rms: f32) {
        // Drop old samples beyond retention. Заодно обеспечивает append-only
        // ordering: если timestamp < последний, добавляем в конец, потеряв
        // монотонность (caller должен гарантировать ordered push).
        if timestamp_ms > self.retention_ms {
            let cutoff = timestamp_ms - self.retention_ms;
            while let Some(&(ts, _)) = self.samples.front() {
                if ts < cutoff {
                    self.samples.pop_front();
                } else {
                    break;
                }
            }
        }
        self.samples.push_back((timestamp_ms, rms));
    }

    /// Количество хранимых точек (для тестов и диагностики).
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Найти cut-point в окне `[start_ms, end_ms]`.
    ///
    /// **Sentinel logic:**
    /// 1. Тихий run (`rms < threshold` ≥ `min_duration_ms`) → cut в его середине.
    /// 2. Тихих run'ов нет → cut в точке с локальным минимумом RMS.
    /// 3. Окно пустое (нет точек в [start, end]) → `None`.
    pub fn find_cut(
        &self,
        start_ms: u64,
        end_ms: u64,
        threshold: f32,
        min_duration_ms: u64,
    ) -> Option<u64> {
        debug_assert!(start_ms < end_ms, "find_cut: empty window");

        // Filter samples внутри окна.
        let in_window: Vec<(u64, f32)> = self
            .samples
            .iter()
            .copied()
            .filter(|(ts, _)| *ts >= start_ms && *ts <= end_ms)
            .collect();
        if in_window.is_empty() {
            return None;
        }

        // Поиск тихих run'ов длительностью >= min_duration_ms.
        let mut best_run: Option<(u64, u64)> = None;
        let mut run_start: Option<u64> = None;
        let mut last_silent_ts: Option<u64> = None;

        for &(ts, rms) in in_window.iter() {
            if rms < threshold {
                if run_start.is_none() {
                    run_start = Some(ts);
                }
                last_silent_ts = Some(ts);
            } else if let (Some(s), Some(e)) = (run_start, last_silent_ts) {
                if e - s >= min_duration_ms {
                    let cur_len = e - s;
                    let best_len = best_run.map(|(bs, be)| be - bs).unwrap_or(0);
                    if cur_len > best_len {
                        best_run = Some((s, e));
                    }
                }
                run_start = None;
                last_silent_ts = None;
            }
        }
        // Tail run (если окно заканчивается тишиной).
        if let (Some(s), Some(e)) = (run_start, last_silent_ts) {
            if e - s >= min_duration_ms {
                let cur_len = e - s;
                let best_len = best_run.map(|(bs, be)| be - bs).unwrap_or(0);
                if cur_len > best_len {
                    best_run = Some((s, e));
                }
            }
        }

        if let Some((s, e)) = best_run {
            return Some((s + e) / 2);
        }

        // Fallback — точка с минимальным RMS в окне.
        in_window
            .iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(ts, _)| *ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_silence_run_in_middle() {
        let mut sd = SilenceDetector::new(60_000);
        // 0-1000ms: loud (0.5), 1000-1500ms: silent (0.005), 1500-2000ms: loud.
        for ts in (0..1000).step_by(100) {
            sd.push(ts, 0.5);
        }
        for ts in (1000..=1500).step_by(100) {
            sd.push(ts, 0.005);
        }
        for ts in (1600..2000).step_by(100) {
            sd.push(ts, 0.5);
        }
        let cut = sd.find_cut(0, 2000, 0.01, 300).unwrap();
        // Silence from 1000 to 1500 → cut в середине = 1250.
        assert_eq!(cut, 1250);
    }

    #[test]
    fn ignores_short_silence_below_min_duration() {
        let mut sd = SilenceDetector::new(60_000);
        sd.push(0, 0.5);
        sd.push(100, 0.5);
        // 200ms silence — короче min_duration=300ms.
        sd.push(200, 0.005);
        sd.push(300, 0.5);
        sd.push(400, 0.5);
        let cut = sd.find_cut(0, 500, 0.01, 300);
        // Не тихий run → fallback на min RMS = 200ms.
        assert_eq!(cut, Some(200));
    }

    #[test]
    fn fallback_to_local_min_rms_when_no_silence() {
        let mut sd = SilenceDetector::new(60_000);
        sd.push(0, 0.5);
        sd.push(100, 0.4);
        sd.push(200, 0.3);
        sd.push(300, 0.6);
        sd.push(400, 0.5);
        // Все > threshold=0.01 → нет тихого run. Min RMS на ts=200 (rms=0.3).
        let cut = sd.find_cut(0, 500, 0.01, 300).unwrap();
        assert_eq!(cut, 200);
    }

    #[test]
    fn returns_none_when_window_empty() {
        let mut sd = SilenceDetector::new(60_000);
        sd.push(0, 0.5);
        sd.push(100, 0.5);
        // Окно [500, 1000] не содержит самплов.
        assert_eq!(sd.find_cut(500, 1000, 0.01, 300), None);
    }

    #[test]
    fn picks_longest_silence_run_when_multiple() {
        let mut sd = SilenceDetector::new(60_000);
        // Short silence 0-300.
        for ts in (0..=300).step_by(100) {
            sd.push(ts, 0.005);
        }
        sd.push(400, 0.5);
        // Long silence 500-1100 (longer than short one).
        for ts in (500..=1100).step_by(100) {
            sd.push(ts, 0.005);
        }
        let cut = sd.find_cut(0, 1500, 0.01, 200).unwrap();
        // Cut в середине ЛУЧШЕГО (длиннее) run'а: (500+1100)/2 = 800.
        assert_eq!(cut, 800);
    }

    #[test]
    fn retention_drops_old_samples() {
        let mut sd = SilenceDetector::new(1_000);
        sd.push(0, 0.5);
        sd.push(500, 0.5);
        sd.push(2_000, 0.5);
        // Retention 1000 от ts=2000 → отбросит ts=0 и ts=500.
        assert_eq!(sd.len(), 1);
    }

    #[test]
    fn silence_at_window_boundary_still_detected() {
        let mut sd = SilenceDetector::new(60_000);
        // Тишина 1700-2000 в окне [1500, 2000].
        for ts in 1500..1700 {
            if ts % 100 == 0 {
                sd.push(ts, 0.5);
            }
        }
        for ts in (1700..=2000).step_by(100) {
            sd.push(ts, 0.005);
        }
        let cut = sd.find_cut(1500, 2000, 0.01, 200).unwrap();
        // Tail run 1700-2000 → cut = 1850.
        assert_eq!(cut, 1850);
    }

    #[test]
    fn empty_detector_returns_none() {
        let sd = SilenceDetector::new(60_000);
        assert!(sd.is_empty());
        assert_eq!(sd.find_cut(0, 1000, 0.01, 300), None);
    }
}
