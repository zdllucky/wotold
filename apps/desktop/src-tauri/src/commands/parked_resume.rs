//! Возобновление звонков, припаркованных из-за нехватки модулей движка.
//!
//! # Почему парковка, а не «заморозка очереди»
//!
//! Живая заморозка оставляла бы звонок в статусе `processing`. На следующем
//! старте `sweep_stale_calls` переводит все `processing` в `failed` без
//! причины, а такие строки — кандидаты авто-восстановления с лимитом двух
//! попыток. Два рестарта в состоянии «модулей нет» сожгли бы лимит на
//! условии, которое восстановление в принципе не лечит.
//!
//! Поэтому звонок падает штатным путём с маркером `local_engine_not_ready`
//! (durable, в базе), а после докачки поднимается отсюда — без действий
//! пользователя.
//!
//! Семафоры `resource_queue` для заморозки не годятся отдельно: при закрытии
//! они деградируют в отсутствие сериализации, то есть пропустили бы работу.

use std::sync::Arc;

use tauri::AppHandle;

use crate::{
    call_id::CallId, call_store::CallStore, db, services::pipeline_runner::PipelineRunner,
    state::AppState,
};

/// Сколько звонков поднимаем за одно событие готовности. Ровно та же причина,
/// что у лимита авто-восстановления: не забивать очередь тяжёлых ресурсов
/// разом. Остальные поднимутся на следующем событии или на старте.
const PARKED_RESUME_MAX_PER_EVENT: usize = 5;
/// Анти-луп: модуль скачан, но обработка всё равно падает — не крутим вечно.
const PARKED_RESUME_MAX_TRIES: u32 = 2;
/// Маркер попыток в каталоге звонка (аналог `.auto-recover-tries`).
const TRIES_MARKER: &str = ".parked-resume-tries";

/// Что известно о припаркованном звонке. Собирается с диска; вердикт ниже —
/// чистая функция.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParkedFacts {
    /// `transcript.md` на диске — STT уже был, хватит переобработки.
    pub has_transcript: bool,
    /// Есть root- или chunk-WAV. Без аудио поднимать нечего.
    pub has_audio: bool,
    /// Сколько раз уже пробовали поднять (маркер-файл).
    pub tries: u32,
}

/// Вердикт по одному припаркованному звонку.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParkedPlan {
    /// Транскрипт есть — обычная переобработка.
    Reprocess,
    /// Транскрипта нет — путь восстановления (reconstruct + STT недостающего).
    Recover,
    /// Лимит на событие исчерпан: остальных не смотрим.
    StopCapReached,
    /// Аудио нет — поднимать нечего.
    SkipNoAudio,
    /// Попытки исчерпаны.
    SkipTriesExhausted,
}

/// Решение по припаркованному звонку. Порядок условий важен: лимит на событие
/// проверяется первым, иначе один «вечно падающий» звонок вытеснял бы из
/// очереди здоровые.
pub(crate) fn plan_parked_resume(started: usize, facts: &ParkedFacts) -> ParkedPlan {
    if started >= PARKED_RESUME_MAX_PER_EVENT {
        return ParkedPlan::StopCapReached;
    }
    if !facts.has_audio {
        return ParkedPlan::SkipNoAudio;
    }
    if facts.tries >= PARKED_RESUME_MAX_TRIES {
        return ParkedPlan::SkipTriesExhausted;
    }
    if facts.has_transcript {
        return ParkedPlan::Reprocess;
    }
    ParkedPlan::Recover
}

/// Поднять припаркованные звонки. Зовётся после успешной докачки модулей и на
/// старте, если движок готов. Ошибки не поднимаются выше: это фоновая уборка.
pub async fn resume_parked_calls(app: AppHandle) {
    let (pool, store, tasks, app_data_dir) = {
        let state = tauri::Manager::state::<AppState>(&app);
        (
            state.db.clone(),
            state.store.clone(),
            state.pipeline_tasks.clone(),
            state.app_data_dir.clone(),
        )
    };

    let parked = match db::list_parked_calls(&pool).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("parked_resume: запрос кандидатов не удался: {e}");
            return;
        }
    };
    if parked.is_empty() {
        return;
    }

    let mut started = 0usize;
    let mut deferred = 0usize;
    for call_id in parked {
        let parsed = CallId::from_db(call_id.as_str());
        let call_dir = store.call_dir(&parsed);
        let marker = call_dir.join(TRIES_MARKER);
        let tries: u32 = std::fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let facts = ParkedFacts {
            has_transcript: call_dir.join("transcript.md").exists(),
            has_audio: call_has_audio(&store, &parsed),
            tries,
        };

        let plan = plan_parked_resume(started, &facts);
        match plan {
            ParkedPlan::StopCapReached => {
                deferred += 1;
                continue;
            }
            ParkedPlan::SkipNoAudio => continue,
            ParkedPlan::SkipTriesExhausted => {
                log::warn!(
                    "parked_resume[{call_id}]: {tries} попыток исчерпано — оставляем failed"
                );
                continue;
            }
            ParkedPlan::Reprocess | ParkedPlan::Recover => {}
        }

        // Счётчик до запуска — падение самого возобновления не должно давать
        // бесконечные повторы.
        if let Err(e) = std::fs::write(&marker, (tries + 1).to_string()) {
            log::warn!("parked_resume[{call_id}]: маркер попыток не записан: {e}");
        }

        let launched = match plan {
            ParkedPlan::Reprocess => PipelineRunner::spawn_reprocess(
                pool.clone(),
                store.clone(),
                app.clone(),
                tasks.clone(),
                call_id.clone(),
            )
            .await
            .map_err(|e| e.to_string()),
            _ => super::recovery::spawn_recover_chunked(
                pool.clone(),
                store.clone(),
                tasks.clone(),
                app_data_dir.clone(),
                app.clone(),
                parsed,
            )
            .await
            .map_err(|e| e.to_string()),
        };
        match launched {
            Ok(()) => {
                started += 1;
                log::info!("parked_resume[{call_id}]: {plan:?} запущен после докачки модулей");
            }
            Err(e) => log::warn!("parked_resume[{call_id}]: не стартовал: {e}"),
        }
    }
    if deferred > 0 {
        log::warn!(
            "parked_resume: лимит {PARKED_RESUME_MAX_PER_EVENT} за событие достигнут, \
             отложено {deferred} звонков — поднимутся на следующем событии или старте"
        );
    }
}

/// Есть ли на диске хоть какое-то аудио звонка. Тот же критерий, что у
/// авто-восстановления.
fn call_has_audio(store: &Arc<CallStore>, call_id: &CallId) -> bool {
    use crate::pipeline::audio_merger::{list_chunk_wavs, TrackKind};
    let chunks_dir = store.chunks_dir(call_id);
    store.mic_path(call_id).exists()
        || store.system_path(call_id).exists()
        || !list_chunk_wavs(&chunks_dir, TrackKind::Mic).is_empty()
        || !list_chunk_wavs(&chunks_dir, TrackKind::System).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(has_transcript: bool, has_audio: bool, tries: u32) -> ParkedFacts {
        ParkedFacts {
            has_transcript,
            has_audio,
            tries,
        }
    }

    #[test]
    fn transcript_present_means_plain_reprocess() {
        assert_eq!(
            plan_parked_resume(0, &facts(true, true, 0)),
            ParkedPlan::Reprocess
        );
    }

    #[test]
    fn no_transcript_goes_through_recovery() {
        assert_eq!(
            plan_parked_resume(0, &facts(false, true, 0)),
            ParkedPlan::Recover
        );
    }

    #[test]
    fn without_audio_there_is_nothing_to_resume() {
        assert_eq!(
            plan_parked_resume(0, &facts(true, false, 0)),
            ParkedPlan::SkipNoAudio
        );
        assert_eq!(
            plan_parked_resume(0, &facts(false, false, 0)),
            ParkedPlan::SkipNoAudio
        );
    }

    #[test]
    fn tries_limit_stops_a_permanently_failing_call() {
        assert_eq!(
            plan_parked_resume(0, &facts(true, true, PARKED_RESUME_MAX_TRIES)),
            ParkedPlan::SkipTriesExhausted
        );
        assert_eq!(
            plan_parked_resume(0, &facts(true, true, PARKED_RESUME_MAX_TRIES - 1)),
            ParkedPlan::Reprocess
        );
    }

    #[test]
    fn per_event_cap_wins_over_everything_else() {
        // Иначе один звонок без аудио «съедал» бы решение за здоровые.
        assert_eq!(
            plan_parked_resume(PARKED_RESUME_MAX_PER_EVENT, &facts(true, true, 0)),
            ParkedPlan::StopCapReached
        );
        assert_eq!(
            plan_parked_resume(PARKED_RESUME_MAX_PER_EVENT, &facts(false, false, 99)),
            ParkedPlan::StopCapReached
        );
    }

    #[test]
    fn cap_boundary_lets_the_last_slot_through() {
        assert_eq!(
            plan_parked_resume(PARKED_RESUME_MAX_PER_EVENT - 1, &facts(false, true, 0)),
            ParkedPlan::Recover
        );
    }

    /// Связка гейта и парковки: текст ошибки готовности обязан попадать под
    /// SQL-условие `list_parked_calls`. Две стороны живут в разных файлах, и
    /// расхождение здесь тихое — звонок остаётся failed навсегда, хотя софт
    /// уже докачан.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn readiness_error_text_is_picked_up_by_the_parked_query() {
        use crate::db::test_support::fresh_db;

        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        crate::db::set_setting(
            &db.pool,
            crate::local_engine::preset::SETTING_ACTIVE_PRESET,
            "light",
        )
        .await
        .unwrap();

        // Ровно тот текст, который пайплайн положит в failed_reason.
        let reason = crate::local_engine::readiness::assert_ready(&db.pool, tmp.path())
            .await
            .expect_err("на пустом каталоге движок не готов")
            .to_string();

        let call = crate::db::insert_recording(&db.pool, "managed")
            .await
            .unwrap();
        crate::db::finish_recording(&db.pool, &call.id, 5.0)
            .await
            .unwrap();
        crate::db::fail_recording_with_reason(&db.pool, &call.id, Some(&reason))
            .await
            .unwrap();

        let parked = crate::db::list_parked_calls(&db.pool).await.unwrap();
        assert_eq!(
            parked,
            vec![call.id],
            "звонок с причиной «{reason}» должен подниматься после докачки"
        );
    }
}
