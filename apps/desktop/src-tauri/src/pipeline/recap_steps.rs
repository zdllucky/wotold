//! [F3] Sink для пошаговых событий генерации рекапа (`recap:step`).
//!
//! Оркестратор и refine-чейн не знают про Tauri: они эмитят шаги через
//! `RecapStepSink`. Production — `BusStepSink` (обёртка над `EventBus`),
//! тесты — свой `VecSink` c захватом, headless — `NoopStepSink`.

use crate::events::{EventBus, RecapStepEvent, RecapStepPreview};
use tauri::AppHandle;

/// Приёмник step-событий. `emit` — fire-and-forget, ошибки глотает
/// сам транспорт (EventBus логирует warn).
pub(crate) trait RecapStepSink: Send + Sync {
    fn emit(&self, ev: RecapStepEvent);
}

/// Production sink: шлёт `recap:step` через EventBus. `app=None` → no-op
/// (headless), тот же контракт что у остальных событий.
pub(crate) struct BusStepSink {
    pub app: Option<AppHandle>,
    pub call_id: String,
}

impl RecapStepSink for BusStepSink {
    fn emit(&self, mut ev: RecapStepEvent) {
        ev.call_id = self.call_id.clone();
        EventBus::new(self.app.as_ref()).recap_step(&ev);
    }
}

/// Тихий sink — для путей, где шаги никому не нужны (headless-вызовы
/// оркестратора, тесты). В production-коде пока не конструируется —
/// оставлен как контрактная заглушка для будущих callers.
#[allow(dead_code)]
pub(crate) struct NoopStepSink;

impl RecapStepSink for NoopStepSink {
    fn emit(&self, _ev: RecapStepEvent) {}
}

/// Хелпер сборки события: заполняет call_id пустым — `BusStepSink` подставит
/// реальный. `chunk_no`/`chunk_total`/`preview` — только для kind=refine.
pub(crate) fn step_event(
    step_idx: u32,
    total_steps: u32,
    kind: &'static str,
    status: &'static str,
) -> RecapStepEvent {
    RecapStepEvent {
        call_id: String::new(),
        step_idx,
        total_steps,
        kind,
        status,
        chunk_no: None,
        chunk_total: None,
        preview: None,
    }
}

/// [F3] Превью текущего состояния рекапа для развёртки шага в UI:
/// title (≤120 chars) + первые ≤3 key_points (по ≤120 chars).
pub(crate) fn preview_from_summary(summary: &serde_json::Value) -> Option<RecapStepPreview> {
    const MAX_CHARS: usize = 120;
    const MAX_POINTS: usize = 3;

    fn truncate_chars(s: &str, max: usize) -> String {
        if s.chars().count() <= max {
            return s.to_string();
        }
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }

    let title = summary.get("title")?.as_str()?.trim();
    if title.is_empty() {
        return None;
    }
    let key_points = summary
        .get("key_points")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .take(MAX_POINTS)
                .map(|s| truncate_chars(s, MAX_CHARS))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(RecapStepPreview {
        title: truncate_chars(title, MAX_CHARS),
        key_points,
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::Mutex;

    /// Тестовый sink: копит события для assert'ов порядка/содержимого.
    pub(crate) struct VecSink(pub Mutex<Vec<RecapStepEvent>>);

    impl VecSink {
        pub(crate) fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }
        pub(crate) fn events(&self) -> Vec<RecapStepEvent> {
            self.0.lock().unwrap().clone()
        }
    }

    impl RecapStepSink for VecSink {
        fn emit(&self, ev: RecapStepEvent) {
            self.0.lock().unwrap().push(ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_truncates_title_and_caps_key_points() {
        let long_title = "т".repeat(200);
        let summary = serde_json::json!({
            "title": long_title,
            "key_points": ["a", "b", "c", "d", "e"],
        });
        let p = preview_from_summary(&summary).unwrap();
        assert!(p.title.chars().count() <= 120);
        assert!(p.title.ends_with('…'));
        assert_eq!(p.key_points.len(), 3);
    }

    #[test]
    fn preview_none_when_title_missing_or_empty() {
        assert!(preview_from_summary(&serde_json::json!({})).is_none());
        assert!(preview_from_summary(&serde_json::json!({ "title": "  " })).is_none());
    }

    #[test]
    fn bus_sink_without_app_is_noop_and_sets_call_id() {
        // Без AppHandle emit не паникует (EventBus None = no-op).
        let sink = BusStepSink {
            app: None,
            call_id: "c1".into(),
        };
        sink.emit(step_event(0, 5, "classify", "started"));
    }
}
