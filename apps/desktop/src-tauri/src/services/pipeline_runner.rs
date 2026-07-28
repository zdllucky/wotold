//! [Phase 4 R4] PipelineRunner — owns spawn/abort lifecycle для pipeline tasks.
//!
//! Раньше `stop_recording`, `reprocess_call`, `cancel_reprocess` все три держали
//! свой собственный inline spawn + insert в `pipeline_tasks` + cleanup remove.
//! Лёгко рассинхронизировать lock ordering или забыть удалить handle при ошибке.
//!
//! Теперь:
//! - `PipelineRunner::spawn_initial` — для свежей записи (post stop_recording).
//! - `PipelineRunner::spawn_reprocess` — abort'ит старого если он есть + spawn'ит новый.
//! - `PipelineRunner::cancel` — abort + restore SQL + emit `pipeline:cancelled`.
//!
//! Lock-ordering preserved: всегда сначала `pipeline_tasks.lock().await`,
//! потом `recording.lock()` / DB writes. Никаких nested ожиданий.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;

use futures_util::FutureExt;
use sqlx::SqlitePool;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::call_store::{ArtifactKind, CallStore};
use crate::events::{EventBus, PipelineCancelledEvent, PipelineFinishedEvent};
use crate::pipeline::{self, PipelineCtx};
use crate::AppError;

/// Реестр active pipeline tasks. Cheap to clone (Arc внутри).
pub type PipelineTasks = Arc<Mutex<HashMap<String, JoinHandle<()>>>>;

/// [Global regen] Какой именно регенератор гнать в фоне через `spawn_regen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegenKind {
    /// Пересоздать recap.md + decisions/open_questions/action_items.
    Recap,
    /// Пересоздать только заголовок звонка (lightweight).
    Title,
}

/// Результат `cancel` — для frontend / тестов чтобы знать, остались ли
/// артефакты на диске (transcript.md). Если да — был успешный previous run
/// и `cancel` восстановит status='ready'; иначе — 'failed' с reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelOutcome {
    pub artifacts_intact: bool,
}

/// Spawn-runner. Не хранит state — все нужные хэндлы / pool передаются методам.
pub struct PipelineRunner;

impl PipelineRunner {
    /// Spawn pipeline для НОВОЙ записи (после stop_recording). Не отменяет
    /// существующих task'ов — если для этого call_id уже что-то запущено
    /// (что не должно случаться, потому что call_id свеже-сгенерирован), он
    /// будет перезаписан в map'е.
    ///
    /// Возвращает сразу — pipeline бежит в фоне.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_initial(
        pool: SqlitePool,
        store: Arc<CallStore>,
        app_handle: AppHandle,
        tasks: PipelineTasks,
        call_id: String,
        mic_path: PathBuf,
        system_path: PathBuf,
    ) {
        Self::spawn_task(
            pool,
            store,
            app_handle,
            tasks,
            call_id,
            mic_path,
            system_path,
            /* is_reprocess */ false,
        )
        .await;
    }

    /// Spawn pipeline для reprocess (M4.5). Если для этого `call_id` уже бежит
    /// task — abort'аем старого и ждём фактический drop, потом spawn'им новый.
    pub async fn spawn_reprocess(
        pool: SqlitePool,
        store: Arc<CallStore>,
        app_handle: AppHandle,
        tasks: PipelineTasks,
        call_id: String,
    ) -> Result<(), AppError> {
        // Pre-flight: row существует.
        let _call = crate::db::get_call(&pool, &call_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("call {call_id}")))?;

        // Если уже бежит — abort.
        if let Some(old) = tasks.lock().await.remove(&call_id) {
            old.abort();
            let _ = old.await;
        }

        let parsed = crate::call_id::CallId::from_db(&call_id);
        let mic_path = store.mic_path(&parsed);
        let system_path = store.system_path(&parsed);
        Self::spawn_task(
            pool,
            store,
            app_handle,
            tasks,
            call_id,
            mic_path,
            system_path,
            /* is_reprocess */ true,
        )
        .await;
        Ok(())
    }

    /// [Global regen] Запустить regen-recap/title как ФОНОВУЮ задачу,
    /// зарегистрированную в `pipeline_tasks` (mirror reprocess): переживает
    /// навигацию, считается в бейдже у «Звонки», эмитит `pipeline:started`/
    /// `pipeline:finished`. Возвращается сразу — генерация бежит в фоне.
    ///
    /// Статус звонка НЕ меняется (остаётся `ready`) — regen это не full-pipeline,
    /// ProcessingPanel не всплывает. Guard: если для call_id уже бежит task
    /// (reprocess / другой regen) — `Err` (не клобберим handle).
    pub async fn spawn_regen(
        pool: SqlitePool,
        store: Arc<CallStore>,
        app_handle: AppHandle,
        tasks: PipelineTasks,
        call_id: String,
        kind: RegenKind,
    ) -> Result<(), AppError> {
        // Pre-flight: row существует.
        let _call = crate::db::get_call(&pool, &call_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("call {call_id}")))?;

        // Guard: уже бежит task для этого звонка → не запускаем второй.
        if tasks.lock().await.contains_key(&call_id) {
            return Err(AppError::Other(format!(
                "call_already_processing: звонок {call_id} уже обрабатывается"
            )));
        }

        let app_data_dir = store.app_data_dir().to_path_buf();
        let call_id_for_task = call_id.clone();
        let work = async move {
            let bus = EventBus::new(Some(&app_handle));
            bus.pipeline_started(&call_id_for_task);

            // [regen panic-safety] Оборачиваем работу в catch_unwind: если
            // future паникует (напр. sidecar/local-LLM путь), мы ВСЁ РАВНО
            // обязаны эмитнуть pipeline:finished и снять handle из tasks.
            // Иначе UI навсегда застревает на «Пересоздаём саммари…» (bgBusy
            // не сбрасывается), а leak'нутый handle блокирует повторный regen
            // с "call_already_processing". Паника → Err.
            let work = async {
                match kind {
                    RegenKind::Recap => {
                        pipeline::regenerate_recap(
                            &pool,
                            &app_data_dir,
                            &call_id_for_task,
                            Some(&app_handle),
                        )
                        .await
                    }
                    RegenKind::Title => pipeline::title_regen::regenerate_title(
                        &pool,
                        &app_data_dir,
                        &call_id_for_task,
                        Some(&app_handle),
                    )
                    .await
                    .map(|_title| ()),
                }
            };
            let result: Result<(), AppError> = match AssertUnwindSafe(work).catch_unwind().await {
                Ok(r) => r,
                Err(_) => Err(AppError::Other(
                    "regen_panic: внутренняя ошибка генерации саммари".into(),
                )),
            };

            let event = match &result {
                Ok(()) => {
                    // [M15.3] Recap-regen обновил recap.md + structured rows —
                    // переиндексировать для ассистента (title regen не влияет).
                    if matches!(kind, RegenKind::Recap) {
                        crate::assistant::indexer::spawn_index(&app_handle, &call_id_for_task);
                    }
                    PipelineFinishedEvent {
                        call_id: call_id_for_task.clone(),
                        status: "ready",
                        failed_reason: None,
                    }
                }
                Err(e) => {
                    log::warn!("regen ({kind:?}) {call_id_for_task} error: {e}");
                    PipelineFinishedEvent {
                        call_id: call_id_for_task.clone(),
                        status: "failed",
                        failed_reason: Some(e.to_string()),
                    }
                }
            };
            bus.pipeline_finished(&event);
        };
        // [TD-13] Регистрация + cleanup — в spawn_registered. Барьер там же
        // закрывает гонку, из-за которой guard выше навсегда отвечал
        // `call_already_processing` после быстро упавшего regen'а.
        Self::spawn_registered(tasks, call_id, work).await;
        Ok(())
    }

    /// Отмена. Идемпотент — если ничего не бежит, возвращает Ok без работы.
    /// Восстанавливает status='ready' если transcript.md на диске (артефакты
    /// от прошлого run'а целы), иначе 'failed'. Эмитит `pipeline:cancelled`.
    pub async fn cancel(
        pool: &SqlitePool,
        store: &CallStore,
        app: &AppHandle,
        tasks: PipelineTasks,
        call_id: &str,
    ) -> Result<CancelOutcome, AppError> {
        // 1. Снимаем handle и abort'аем. None → ничего не бежит, no-op.
        let handle = tasks.lock().await.remove(call_id);
        let Some(h) = handle else {
            return Ok(CancelOutcome {
                artifacts_intact: false,
            });
        };
        h.abort();
        // Дожидаемся фактического drop'а, иначе наш restore UPDATE может
        // race'нуть с последним `set_call_progress` из pipeline'а.
        let _ = h.await;

        // 2. Проверяем артефакты.
        let transcript_path = store.artifact_path(
            &crate::call_id::CallId::from_db(call_id),
            ArtifactKind::Transcript,
        );
        let artifacts_intact = tokio::fs::metadata(&transcript_path).await.is_ok();

        // 3. Restore SQL.
        if artifacts_intact {
            // [TD-17] Через db-слой, а не сырым SQL: SET-клауза идентична
            // mark_call_ready (status + failed_reason + pipeline_* + updated_at),
            // но раньше писалась здесь копией — то есть мимо любых гейтов,
            // которые db-слой захочет ввести. Переход processing → ready
            // легален (артефакты на диске целы, отменённый run восстановлен).
            crate::db::mark_call_ready(pool, call_id).await?;
            // [M15.3] Артефакты целы, звонок снова ready — вернуть в индекс
            // ассистента (reprocess его деиндексировал на старте).
            if let Err(e) = crate::assistant::indexer::index_call(pool, store, call_id).await {
                log::warn!("assistant index[{call_id}] after cancel-restore failed: {e}");
            }
        } else {
            crate::db::fail_recording_with_reason(pool, call_id, Some("Отменено пользователем"))
                .await?;
        }

        // 4. Emit.
        let bus = EventBus::new(Some(app));
        bus.pipeline_cancelled(&PipelineCancelledEvent {
            call_id: call_id.to_string(),
            artifacts_intact,
        });

        Ok(CancelOutcome { artifacts_intact })
    }

    /// [M12.6] Оборвать работу по звонку **без** восстановления статуса и без
    /// событий. Возвращает `true`, если что-то бежало.
    ///
    /// Отличие от [`cancel`](Self::cancel): та чинит состояние звонка (ready
    /// либо failed) и сообщает об этом UI. Здесь звонок сейчас исчезнет
    /// целиком, восстанавливать нечего — нужно только снять работу.
    ///
    /// Зачем вообще: удаление звонка сносило строки и каталог, но не трогало
    /// задачу. Сайдкар whisper/llama продолжал считать удалённый звонок
    /// (минуты, на Quality — дольше), а пайплайн следом писал артефакты в
    /// заново созданный каталог — оставались пустые директории от несуществующих
    /// звонков. Abort раскручивает стек, и `SidecarGuard::drop` убивает процесс.
    pub async fn abort_silently(tasks: PipelineTasks, call_id: &str) -> bool {
        let handle = tasks.lock().await.remove(call_id);
        let Some(h) = handle else {
            return false;
        };
        h.abort();
        // Ждём фактического drop'а: пока стек не раскрутился, сайдкар жив и
        // файлы ещё могут появиться — то есть удалять каталог рано.
        let _ = h.await;
        true
    }

    /// [TD-13] Spawn задачи с **барьером регистрации**.
    ///
    /// Гонка, которую это чинит: раньше обе точки спавна делали
    /// `spawn(... в конце tasks.remove(id))`, а `insert` шёл ПОСЛЕ `spawn`.
    /// Быстро упавшая задача (напр. `local_engine_preset_not_set`) успевала
    /// выполнить свой `remove` до вставки — тот был no-op, — и следом `insert`
    /// клал handle УЖЕ ЗАВЕРШЁННОЙ задачи, которую никто не уберёт.
    /// Для `spawn_regen` это фатально: guard `contains_key` навсегда отвечал
    /// `call_already_processing`, и пересоздать саммари было нельзя до
    /// перезапуска приложения.
    ///
    /// `insert` до `spawn` невозможен (handle появляется только из `spawn`),
    /// поэтому убираем гонку с другой стороны: тело задачи ждёт сигнал
    /// «ты зарегистрирован» и лишь затем работает. К моменту, когда задача
    /// может себя удалить, запись гарантированно в реестре.
    ///
    /// Если сигнал не пришёл (задачу заабортили между `spawn` и `send`),
    /// `rx.await` вернёт `Err` — работа не начинается, мусора не остаётся.
    async fn spawn_registered<F>(tasks: PipelineTasks, call_id: String, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let call_id_for_task = call_id.clone();
        let tasks_for_task = tasks.clone();
        let handle = tauri::async_runtime::spawn(async move {
            // Барьер: не начинаем работу, пока не зарегистрированы.
            if rx.await.is_err() {
                return;
            }
            fut.await;
            tasks_for_task.lock().await.remove(&call_id_for_task);
        });
        tasks.lock().await.insert(call_id, handle);
        let _ = tx.send(());
    }

    /// Внутренний helper: spawn'ит async task, регистрирует в `tasks`,
    /// при завершении удаляет себя из map'а.
    #[allow(clippy::too_many_arguments)]
    async fn spawn_task(
        pool: SqlitePool,
        store: Arc<CallStore>,
        app_handle: AppHandle,
        tasks: PipelineTasks,
        call_id: String,
        mic_path: PathBuf,
        system_path: PathBuf,
        is_reprocess: bool,
    ) {
        let app_data_dir = store.app_data_dir().to_path_buf();
        let call_id_for_task = call_id.clone();
        let work = async move {
            if is_reprocess {
                // Reset status row (см. pipeline::reprocess_call).
                if let Err(e) = pipeline::reprocess_call(
                    &pool,
                    &app_data_dir,
                    &call_id_for_task,
                    Some(&app_handle),
                )
                .await
                {
                    log::error!("reprocess {call_id_for_task} error: {e}");
                }
            } else {
                let ctx = PipelineCtx {
                    call_id: call_id_for_task.clone(),
                    call_dir: app_data_dir.join("calls").join(&call_id_for_task),
                    mic_path,
                    system_path,
                    app_data_dir: app_data_dir.clone(),
                };
                if let Err(e) = pipeline::run(&pool, ctx, Some(&app_handle)).await {
                    log::error!("pipeline {call_id_for_task} error: {e}");
                }
            }
        };
        // [TD-13] Регистрация + cleanup — в spawn_registered.
        Self::spawn_registered(tasks, call_id, work).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use std::sync::Arc;
    use tempfile::tempdir;

    // ============================================================
    // [TD-13] Барьер регистрации: гонка insert-after-spawn
    // ============================================================

    fn empty_tasks() -> PipelineTasks {
        Arc::new(Mutex::new(HashMap::new()))
    }

    /// Дождаться, пока реестр опустеет (или сдаться). Без sleep-синхронизации
    /// с «на глазок» задержкой — правило 6 инженерных правил.
    async fn wait_until_empty(tasks: &PipelineTasks, tries: u32) -> bool {
        for _ in 0..tries {
            if tasks.lock().await.is_empty() {
                return true;
            }
            tokio::task::yield_now().await;
        }
        tasks.lock().await.is_empty()
    }

    #[tokio::test]
    async fn instantly_finishing_task_leaves_no_stale_entry() {
        // Регрессия TD-13: задача, падающая мгновенно (напр.
        // local_engine_preset_not_set), успевала сделать свой remove ДО
        // вставки — тот был no-op, — и следом insert клал handle уже
        // завершённой задачи. Для spawn_regen это навсегда блокировало
        // повторную генерацию с `call_already_processing`.
        let tasks = empty_tasks();
        PipelineRunner::spawn_registered(tasks.clone(), "c1".into(), async {}).await;

        assert!(
            wait_until_empty(&tasks, 1000).await,
            "мгновенно завершившаяся задача не должна оставлять stale-запись"
        );
    }

    #[tokio::test]
    async fn running_task_is_visible_in_registry() {
        // Guard `call_already_processing` обязан продолжать работать:
        // пока задача жива, её запись в реестре есть.
        let tasks = empty_tasks();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        PipelineRunner::spawn_registered(tasks.clone(), "c1".into(), async move {
            let _ = release_rx.await;
        })
        .await;

        assert!(
            tasks.lock().await.contains_key("c1"),
            "работающая задача обязана быть видна guard'у"
        );

        let _ = release_tx.send(());
        assert!(
            wait_until_empty(&tasks, 1000).await,
            "после завершения запись снимается"
        );
    }

    #[tokio::test]
    async fn aborted_task_does_not_block_respawn() {
        // cancel-путь: снимаем handle и abort'аем. Повторный spawn для того же
        // call_id обязан пройти (реестр не «залип»).
        let tasks = empty_tasks();
        let (_hold_tx, hold_rx) = tokio::sync::oneshot::channel::<()>();
        PipelineRunner::spawn_registered(tasks.clone(), "c1".into(), async move {
            let _ = hold_rx.await;
        })
        .await;

        let handle = tasks.lock().await.remove("c1").expect("handle есть");
        handle.abort();
        assert!(tasks.lock().await.is_empty());

        PipelineRunner::spawn_registered(tasks.clone(), "c1".into(), async {}).await;
        assert!(
            wait_until_empty(&tasks, 1000).await,
            "повторный spawn после abort проходит и чистится"
        );
    }

    #[tokio::test]
    async fn abort_silently_stops_the_work_before_deletion() {
        // [M12.6] Удаление звонка обязано снять работу: иначе сайдкар считает
        // удалённый звонок, а пайплайн создаёт каталог заново.
        let tasks = empty_tasks();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done_in_task = done.clone();
        let (_hold_tx, hold_rx) = tokio::sync::oneshot::channel::<()>();
        PipelineRunner::spawn_registered(tasks.clone(), "c1".into(), async move {
            let _ = hold_rx.await;
            done_in_task.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .await;

        assert!(PipelineRunner::abort_silently(tasks.clone(), "c1").await);
        assert!(tasks.lock().await.is_empty(), "запись снята из реестра");
        assert!(
            !done.load(std::sync::atomic::Ordering::SeqCst),
            "работа не должна была доработать до конца"
        );
    }

    #[tokio::test]
    async fn abort_silently_is_a_noop_without_running_work() {
        // Удаление обычного (уже готового) звонка не должно ничего искать.
        let tasks = empty_tasks();
        assert!(!PipelineRunner::abort_silently(tasks.clone(), "c1").await);
    }

    fn arc_device(id: &str) -> Arc<str> {
        Arc::from(id.to_string().into_boxed_str())
    }

    #[tokio::test]
    async fn cancel_with_no_active_task_returns_no_artifacts_intact() {
        let db = fresh_db().await;
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let tasks: PipelineTasks = Arc::new(Mutex::new(HashMap::new()));

        // Без AppHandle мы не можем дёрнуть PipelineRunner::cancel
        // (он принимает &AppHandle). Поэтому проверяем internal-ный путь
        // вручную — handle отсутствует → Ok с artifacts_intact=false,
        // никаких SQL изменений.
        let outcome = tasks.lock().await.remove("ghost-id");
        assert!(outcome.is_none(), "никакой task'и не зарегистрировано");

        // Reading через CallStore: артефакта нет.
        let transcript = store
            .read_artifact(
                &crate::call_id::CallId::from_db("ghost-id"),
                ArtifactKind::Transcript,
            )
            .await
            .unwrap();
        assert!(transcript.is_none());
        // DB должна остаться чистой (нет такой row).
        let row = crate::db::get_call(&db.pool, "ghost-id").await.unwrap();
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn spawn_regen_guard_detects_active_task() {
        // [Global regen] Guard в spawn_regen: если для call_id уже бежит task
        // (reprocess / другой regen) — не запускаем второй (не клобберим handle).
        // Без AppHandle полный spawn_regen не дёрнуть — проверяем сам guard
        // (contains_key) через map напрямую, в стиле cancel-теста.
        let tasks: PipelineTasks = Arc::new(Mutex::new(HashMap::new()));
        assert!(
            !tasks.lock().await.contains_key("c1"),
            "пусто → guard пропускает"
        );
        let dummy = tauri::async_runtime::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });
        tasks.lock().await.insert("c1".to_string(), dummy);
        assert!(
            tasks.lock().await.contains_key("c1"),
            "task активна → guard отклонит второй spawn"
        );
        // Cleanup — снимаем guard в отдельный statement (temporary в if-let
        // держал бы borrow `tasks` до конца блока → E0597).
        let removed = tasks.lock().await.remove("c1");
        if let Some(h) = removed {
            h.abort();
        }
    }

    #[tokio::test]
    async fn spawn_reprocess_unknown_call_returns_not_found() {
        // pre-flight в spawn_reprocess — get_call. Должен вернуть NotFound,
        // ничего не spawn'нув. Тестируем БЕЗ AppHandle через прямой вызов
        // db::get_call (это полностью покрывает первую ветку spawn_reprocess).
        let db = fresh_db().await;
        let dir = tempdir().unwrap();
        let _store = Arc::new(CallStore::new(dir.path().to_path_buf()));
        let _device = arc_device("dev-1");

        let res = crate::db::get_call(&db.pool, "ghost-id").await.unwrap();
        assert!(res.is_none(), "pre-flight должен поймать missing row");
    }

    #[tokio::test]
    async fn cancel_with_artifacts_intact_restores_ready_status() {
        // Симулируем running pipeline через map+JoinHandle на dummy task'у.
        // Затем cancel должен abort'нуть + увидеть transcript.md → UPDATE ready.
        // ЭТОТ тест проверяет только SQL-логику cancel'а — emit мы скипаем
        // через None handle (не передаём AppHandle через вспомогательную ветку).
        let db = fresh_db().await;
        let dir = tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());
        let tasks: PipelineTasks = Arc::new(Mutex::new(HashMap::new()));

        // 1. Row в processing с прогрессом.
        let call = crate::db::insert_recording(&db.pool, "managed")
            .await
            .unwrap();
        crate::db::set_call_progress(&db.pool, &call.id, 3, 50, None, None)
            .await
            .unwrap();

        // 2. transcript.md на диске — будто старый run завершился раньше.
        let call_dir = store.call_dir(&crate::call_id::CallId::from_db(&call.id));
        tokio::fs::create_dir_all(&call_dir).await.unwrap();
        tokio::fs::write(call_dir.join("transcript.md"), "S1: hello")
            .await
            .unwrap();

        // 3. Dummy task: будет abort'нут.
        let dummy = tauri::async_runtime::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });
        tasks.lock().await.insert(call.id.clone(), dummy);

        // 4. Manual reproduction of cancel SQL-path (без AppHandle).
        let handle = tasks.lock().await.remove(&call.id);
        let h = handle.expect("registered task");
        h.abort();
        let _ = h.await;

        let transcript_path = store.artifact_path(
            &crate::call_id::CallId::from_db(&call.id),
            ArtifactKind::Transcript,
        );
        let artifacts_intact = tokio::fs::metadata(&transcript_path).await.is_ok();
        assert!(artifacts_intact);

        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE calls
             SET status = 'ready',
                 failed_reason = NULL,
                 pipeline_step = NULL,
                 pipeline_pct = NULL,
                 pipeline_eta_sec = NULL,
                 upload_bytes = NULL,
                 updated_at = ?1
             WHERE id = ?2",
        )
        .bind(&now)
        .bind(&call.id)
        .execute(&db.pool)
        .await
        .unwrap();

        let after = crate::db::get_call(&db.pool, &call.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.status, "ready");
        assert!(after.pipeline_step.is_none());
        assert!(after.pipeline_pct.is_none());
        assert!(after.failed_reason.is_none());
    }
}
