//! [Q] Per-resource очереди тяжёлых local-engine ресурсов (concurrency = 1).
//!
//! ## Зачем
//!
//! 3 параллельных reprocess = до 6 whisper-сайдкаров по 8 потоков + диаризация
//! без лимита → CPU-thrashing. LLM был сериализован локальным
//! `LLM_SEMAPHORE`; этот модуль обобщает паттерн на все три ресурса
//! (STT / Diarization / LLM) и добавляет наблюдаемость: на каждый transition
//! (встал в очередь / захватил / освободил) эмитится `queue:state` — полный
//! снапшот для QueueMonitor-попапа и «в очереди» на странице звонка.
//!
//! ## Гарантии
//!
//! - FIFO: `tokio::sync::Semaphore` выдаёт permits в порядке очереди.
//! - Drop-safety: отмена ждущего (`JoinHandle::abort` на `.await`) убирает
//!   его из waiting (`WaitGuard`); drop `QueuePermit` (включая panic/cancel
//!   держателя) освобождает ресурс. Сайдкары убиваются своим `SidecarGuard`.
//! - `OwnedSemaphorePermit` — permit можно переносить в `spawn_blocking`
//!   (диаризация: abort не прерывает blocking, ресурс честно busy до конца).
//! - Снапшот строится под lock'ом, emit — ВНЕ lock'а (иначе deadlock через
//!   sink, если тот когда-нибудь синхронно дернёт snapshot()).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock};

use serde::Serialize;
use tauri::AppHandle;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::events::EventBus;

/// Тяжёлый ресурс. id — стабильный контракт с фронтом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Stt,
    Diarization,
    Llm,
}

impl Resource {
    pub const ALL: [Resource; 3] = [Resource::Stt, Resource::Diarization, Resource::Llm];

    pub fn id(self) -> &'static str {
        match self {
            Resource::Stt => "stt",
            Resource::Diarization => "diarization",
            Resource::Llm => "llm",
        }
    }

    fn index(self) -> usize {
        match self {
            Resource::Stt => 0,
            Resource::Diarization => 1,
            Resource::Llm => 2,
        }
    }
}

/// Одна запись в очереди/работе. `call_id=None` — служебная задача (warm-up).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QueueTicket {
    pub call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceStateDto {
    pub id: &'static str,
    pub busy: Option<QueueTicket>,
    /// FIFO; позиция в очереди = index + 1.
    pub waiting: Vec<QueueTicket>,
}

/// Полный снапшот всех ресурсов — payload `queue:state` и ответ
/// `get_queue_state`.
#[derive(Debug, Clone, Serialize)]
pub struct QueueStateEvent {
    pub resources: Vec<ResourceStateDto>,
}

/// Приёмник снапшотов. Production — `BusQueueSink`; до `set_app` — Noop.
pub(crate) trait QueueSink: Send + Sync {
    fn emit(&self, ev: QueueStateEvent);
}

struct NoopSink;
impl QueueSink for NoopSink {
    fn emit(&self, _ev: QueueStateEvent) {}
}

struct BusQueueSink {
    app: AppHandle,
}
impl QueueSink for BusQueueSink {
    fn emit(&self, ev: QueueStateEvent) {
        EventBus::new(Some(&self.app)).queue_state(&ev);
    }
}

struct ResState {
    busy: Option<(u64, QueueTicket)>,
    waiting: Vec<(u64, QueueTicket)>,
}

struct ResourceEntry {
    sem: Arc<Semaphore>,
    state: Mutex<ResState>,
}

impl ResourceEntry {
    fn new() -> Self {
        Self {
            sem: Arc::new(Semaphore::new(1)),
            state: Mutex::new(ResState {
                busy: None,
                waiting: Vec::new(),
            }),
        }
    }

    /// Lock с восстановлением после poisoning: состояние очереди — простые
    /// Vec/Option, паника держателя не оставляет их в невалидном виде.
    fn state(&self) -> MutexGuard<'_, ResState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct QueueRegistry {
    entries: [ResourceEntry; 3],
    sink: RwLock<Arc<dyn QueueSink>>,
    next_ticket: AtomicU64,
}

impl QueueRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: [
                ResourceEntry::new(),
                ResourceEntry::new(),
                ResourceEntry::new(),
            ],
            sink: RwLock::new(Arc::new(NoopSink)),
            next_ticket: AtomicU64::new(1),
        })
    }

    pub fn global() -> &'static Arc<QueueRegistry> {
        // OnceLock вместо LazyLock — MSRV 1.77 (LazyLock стабилен с 1.80).
        static GLOBAL: OnceLock<Arc<QueueRegistry>> = OnceLock::new();
        GLOBAL.get_or_init(QueueRegistry::new)
    }

    fn set_sink(&self, sink: Arc<dyn QueueSink>) {
        *self.sink.write().unwrap_or_else(PoisonError::into_inner) = sink;
    }

    /// Полный снапшот (под lock'ами, по одному ресурсу за раз).
    pub fn snapshot(&self) -> QueueStateEvent {
        let resources = Resource::ALL
            .iter()
            .map(|r| {
                let st = self.entries[r.index()].state();
                ResourceStateDto {
                    id: r.id(),
                    busy: st.busy.as_ref().map(|(_, t)| t.clone()),
                    waiting: st.waiting.iter().map(|(_, t)| t.clone()).collect(),
                }
            })
            .collect();
        QueueStateEvent { resources }
    }

    fn emit_snapshot(&self) {
        let snap = self.snapshot();
        let sink = self
            .sink
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        sink.emit(snap);
    }

    /// Встать в очередь ресурса и дождаться permit'а. FIFO. Отмена во время
    /// ожидания (drop future) корректно убирает запись из waiting.
    pub async fn acquire(self: &Arc<Self>, res: Resource, call_id: Option<&str>) -> QueuePermit {
        let ticket_id = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        let ticket = QueueTicket {
            call_id: call_id.map(str::to_string),
        };

        {
            let mut st = self.entries[res.index()].state();
            st.waiting.push((ticket_id, ticket.clone()));
        }
        self.emit_snapshot();

        // Guard на случай отмены прямо на `.await` семафора.
        let mut wait_guard = WaitGuard {
            reg: Some(Arc::clone(self)),
            res,
            ticket_id,
        };

        let sem = Arc::clone(&self.entries[res.index()].sem);
        // Семафор никогда не close'ится; на невозможной ошибке — деградация
        // без сериализации (permit=None) вместо паники, с громким логом.
        let permit = match sem.acquire_owned().await {
            Ok(p) => Some(p),
            Err(e) => {
                log::error!("resource_queue: semaphore closed ({e}) — деградация без очереди");
                None
            }
        };

        {
            let mut st = self.entries[res.index()].state();
            st.waiting.retain(|(id, _)| *id != ticket_id);
            st.busy = Some((ticket_id, ticket));
        }
        wait_guard.reg = None; // disarm — мы больше не «ждущие»
        self.emit_snapshot();

        QueuePermit {
            reg: Arc::clone(self),
            res,
            ticket_id,
            _permit: permit,
        }
    }
}

/// Убирает запись из waiting, если acquire-future отменили до получения
/// permit'а (abort пайплайна пока звонок стоял в очереди).
struct WaitGuard {
    reg: Option<Arc<QueueRegistry>>,
    res: Resource,
    ticket_id: u64,
}

impl Drop for WaitGuard {
    fn drop(&mut self) {
        let Some(reg) = self.reg.take() else { return };
        {
            let mut st = reg.entries[self.res.index()].state();
            st.waiting.retain(|(id, _)| *id != self.ticket_id);
        }
        reg.emit_snapshot();
    }
}

/// RAII-permit: пока жив — ресурс busy этим тикетом; drop (включая panic /
/// cancel держателя) освобождает ресурс и эмитит снапшот. `'static` — можно
/// переносить в `spawn_blocking`.
pub struct QueuePermit {
    reg: Arc<QueueRegistry>,
    res: Resource,
    ticket_id: u64,
    _permit: Option<OwnedSemaphorePermit>,
}

impl Drop for QueuePermit {
    fn drop(&mut self) {
        {
            let mut st = self.reg.entries[self.res.index()].state();
            if st
                .busy
                .as_ref()
                .is_some_and(|(id, _)| *id == self.ticket_id)
            {
                st.busy = None;
            }
        }
        self.reg.emit_snapshot();
    }
}

// ── Удобные free-fn поверх global() — для провайдеров ──────────────────────

/// Встать в очередь глобального реестра.
pub async fn acquire(res: Resource, call_id: Option<&str>) -> QueuePermit {
    QueueRegistry::global().acquire(res, call_id).await
}

/// Подключить эмиссию `queue:state` (вызывается один раз из lib.rs setup).
pub fn set_app(app: AppHandle) {
    QueueRegistry::global().set_sink(Arc::new(BusQueueSink { app }));
}

/// Снапшот глобального реестра (команда `get_queue_state`).
pub fn snapshot() -> QueueStateEvent {
    QueueRegistry::global().snapshot()
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Тестовый sink: копит снапшоты для assert'ов.
    pub(crate) struct VecSink(pub Mutex<Vec<QueueStateEvent>>);

    impl VecSink {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(Vec::new())))
        }
        pub(crate) fn count(&self) -> usize {
            self.0.lock().unwrap().len()
        }
    }

    impl QueueSink for VecSink {
        fn emit(&self, ev: QueueStateEvent) {
            self.0.lock().unwrap().push(ev);
        }
    }

    pub(crate) fn registry_with_sink() -> (Arc<QueueRegistry>, Arc<VecSink>) {
        let reg = QueueRegistry::new();
        let sink = VecSink::new();
        reg.set_sink(sink.clone());
        (reg, sink)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::registry_with_sink;
    use super::*;
    use std::time::Duration;

    fn res_state<'a>(ev: &'a QueueStateEvent, id: &str) -> &'a ResourceStateDto {
        ev.resources.iter().find(|r| r.id == id).unwrap()
    }

    #[tokio::test]
    async fn acquire_is_fifo() {
        let (reg, _sink) = registry_with_sink();
        let order = Arc::new(Mutex::new(Vec::new()));

        let first = reg.acquire(Resource::Stt, Some("c1")).await;

        let mut handles = Vec::new();
        for name in ["c2", "c3", "c4"] {
            let reg = Arc::clone(&reg);
            let order = Arc::clone(&order);
            handles.push(tokio::spawn(async move {
                let _p = reg.acquire(Resource::Stt, Some(name)).await;
                order.lock().unwrap().push(name.to_string());
            }));
            // Даём каждому встать в очередь по порядку.
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        drop(first);
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec!["c2", "c3", "c4"]);
    }

    #[tokio::test]
    async fn snapshot_shows_busy_and_waiting() {
        let (reg, _sink) = registry_with_sink();
        let _p1 = reg.acquire(Resource::Llm, Some("c1")).await;

        let reg2 = Arc::clone(&reg);
        let waiter = tokio::spawn(async move {
            let _p = reg2.acquire(Resource::Llm, Some("c2")).await;
        });
        tokio::time::sleep(Duration::from_millis(30)).await;

        let snap = reg.snapshot();
        let llm = res_state(&snap, "llm");
        assert_eq!(llm.busy.as_ref().unwrap().call_id.as_deref(), Some("c1"));
        assert_eq!(llm.waiting.len(), 1);
        assert_eq!(llm.waiting[0].call_id.as_deref(), Some("c2"));
        // Другие ресурсы свободны.
        assert!(res_state(&snap, "stt").busy.is_none());

        drop(_p1);
        waiter.await.unwrap();
    }

    #[tokio::test]
    async fn permit_drop_promotes_next_waiter_and_emits() {
        let (reg, sink) = registry_with_sink();
        let p1 = reg.acquire(Resource::Stt, Some("c1")).await;

        let reg2 = Arc::clone(&reg);
        let waiter = tokio::spawn(async move {
            let _p = reg2.acquire(Resource::Stt, Some("c2")).await;
            // Держим чуть-чуть чтобы снапшот успел показать busy=c2.
            tokio::time::sleep(Duration::from_millis(30)).await;
        });
        tokio::time::sleep(Duration::from_millis(30)).await;

        let before = sink.count();
        drop(p1);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(sink.count() > before, "release + acquire должны эмитить");
        let snap = reg.snapshot();
        assert_eq!(
            res_state(&snap, "stt")
                .busy
                .as_ref()
                .unwrap()
                .call_id
                .as_deref(),
            Some("c2")
        );
        waiter.await.unwrap();
    }

    #[tokio::test]
    async fn aborted_waiter_disappears_from_waiting() {
        let (reg, sink) = registry_with_sink();
        let _p1 = reg.acquire(Resource::Diarization, Some("c1")).await;

        let reg2 = Arc::clone(&reg);
        let waiter = tokio::spawn(async move {
            let _p = reg2.acquire(Resource::Diarization, Some("c2")).await;
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(res_state(&reg.snapshot(), "diarization").waiting.len(), 1);

        let before = sink.count();
        waiter.abort();
        tokio::time::sleep(Duration::from_millis(30)).await;

        assert_eq!(
            res_state(&reg.snapshot(), "diarization").waiting.len(),
            0,
            "abort ждущего должен убрать его из очереди"
        );
        assert!(sink.count() > before, "WaitGuard должен эмитить снапшот");
    }

    #[tokio::test]
    async fn permit_moved_into_spawn_blocking_holds_resource() {
        let (reg, _sink) = registry_with_sink();
        let permit = reg.acquire(Resource::Diarization, Some("c1")).await;

        let blocking = tokio::task::spawn_blocking(move || {
            let _q = permit; // permit живёт внутри blocking-задачи
            std::thread::sleep(Duration::from_millis(80));
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            res_state(&reg.snapshot(), "diarization").busy.is_some(),
            "ресурс busy пока blocking-задача жива"
        );
        blocking.await.unwrap();
        assert!(res_state(&reg.snapshot(), "diarization").busy.is_none());
    }

    #[tokio::test]
    async fn panic_while_holding_permit_releases() {
        let (reg, _sink) = registry_with_sink();
        let reg2 = Arc::clone(&reg);
        let handle = tokio::spawn(async move {
            let _p = reg2.acquire(Resource::Llm, Some("c1")).await;
            panic!("boom");
        });
        let _ = handle.await; // JoinError(panic)
        assert!(
            res_state(&reg.snapshot(), "llm").busy.is_none(),
            "panic держателя должен освободить ресурс"
        );
        // Ресурс снова доступен.
        let _p = reg.acquire(Resource::Llm, Some("c2")).await;
    }

    #[test]
    fn event_payload_serde_shape() {
        let ev = QueueStateEvent {
            resources: vec![ResourceStateDto {
                id: "stt",
                busy: Some(QueueTicket {
                    call_id: Some("abc".into()),
                }),
                waiting: vec![QueueTicket { call_id: None }],
            }],
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["resources"][0]["id"], "stt");
        assert_eq!(json["resources"][0]["busy"]["call_id"], "abc");
        assert!(json["resources"][0]["waiting"][0]["call_id"].is_null());
    }
}
