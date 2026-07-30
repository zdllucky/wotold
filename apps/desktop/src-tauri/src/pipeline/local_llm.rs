//! [TD-35] Обвязка локальной LLM: провайдер, прогрев, резидентный сервер и
//! регенерация рекапа на локальной модели.
//!
//! Выделено из `pipeline/mod.rs` (1914 строк при лимите 800, правило 8).
//! Граница естественная: всё здесь — про то, как поднять и погасить локальную
//! модель, а не про то, из чего состоит обработка звонка. Логика не менялась.

use std::path::Path;

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::{db, AppError};

use super::settings::PipelineSettings;
use super::{local_orchestrator, recap, recap_progress, recap_steps, speaker_prompt_ctx};

/// [P5.1 / TD-36] Ярлык движка по id модели — тот, что уходит в
/// `calls.summary_engine` и в баннер рекапа.
///
/// Был скопирован дословно в двух местах (маршрут записи и регенерация), и
/// это ровно тот случай, когда копия молча устаревает: добавили бы четвёртый
/// размер модели — один путь показывал бы «local-qwen», другой честный
/// ярлык, и различить их можно было бы только по звонку.
pub(crate) fn local_engine_label(llm_id: &str) -> &'static str {
    match llm_id {
        id if id.contains("1.5b") || id.contains("1_5b") => "local-qwen-1.5b",
        id if id.contains("3b") => "local-qwen-3b",
        id if id.contains("7b") => "local-qwen-7b",
        _ => "local-qwen",
    }
}

/// [P5.1] Resolve current local preset → engine label string для persist
/// в `calls.summary_engine`. Best-effort: на ошибке чтения preset либо
/// None preset возвращает None — caller передаёт это в `set_recap_failure`
/// и `summary_engine` остаётся unchanged (safer чем persist неправильный).
pub(crate) async fn local_engine_label_from_pool(pool: &SqlitePool) -> Option<String> {
    let raw = db::get_setting(pool, crate::local_engine::preset::SETTING_ACTIVE_PRESET)
        .await
        .ok()
        .flatten()?;
    let preset = crate::local_engine::preset::LocalEnginePreset::from_str(&raw)?;
    Some(preset.engine_label().to_string())
}

/// M4.5 паспорта: ручная регенерация рекапа без повторной транскрипции.
/// Используется когда:
///   - первая попытка LLM упала (квота / network) и пользователь хочет повторить
///   - сменили модель в Settings и хотят пересоздать рекап на ней
///   - в транскрипт были внесены правки (будущий M4.6)
///
/// Читает `transcript.md` с диска, перегенерит `recap.md` + `action_items`.
/// transcript.md обязателен — иначе AppError. Ошибки LLM пробрасываются
/// в UI как Err (а не silently skip как в pipeline::run).
#[cfg(target_os = "macos")]
pub async fn regenerate_recap(
    pool: &SqlitePool,
    app_data_dir: &std::path::Path,
    call_id: &str,
    app: Option<&AppHandle>,
) -> Result<(), AppError> {
    let call = db::get_call(pool, call_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("call {call_id}")))?;

    let call_dir = app_data_dir.join("calls").join(call_id);
    let transcript_path = call_dir.join("transcript.md");
    let transcript_md = tokio::fs::read_to_string(&transcript_path)
        .await
        .map_err(|e| AppError::Other(format!("transcript.md отсутствует: {e}")))?;

    // [Bug-fix] Pre-clear stale recap_failed_reason — чтобы баннер с прошлой
    // ошибкой не сбивал юзера с толку пока новая попытка идёт.
    let _ = db::set_recap_failed_reason(pool, call_id, None).await;

    // [Phase 2 R3] Typed settings — один read, typed fields, edge cases
    // (malformed threshold, "auto" lang) изолированы.
    let s = PipelineSettings::load(pool).await?;
    let effective_lang = s.effective_recap_lang(call.lang_detected.as_deref());

    let app = app.ok_or_else(|| {
        AppError::Other("regenerate_recap требует AppHandle (внутренняя ошибка)".into())
    })?;
    let result = regenerate_recap_local(
        pool,
        app_data_dir,
        &call_dir,
        call_id,
        &transcript_md,
        effective_lang.as_deref(),
        app,
        &s,
    )
    .await;
    match result {
        Ok(()) => {
            let _ = db::set_recap_failed_reason(pool, call_id, None).await;
            Ok(())
        }
        Err(e) => {
            // [P5.1] Persist engine label atomically с reason —
            // banner badge ↔ failure text всегда matched.
            let engine_label = local_engine_label_from_pool(pool).await;
            let _ =
                db::set_recap_failure(pool, call_id, Some(&e.to_string()), engine_label.as_deref())
                    .await;
            Err(e)
        }
    }
}

/// Non-macOS: local-движок недоступен (R9).
#[cfg(not(target_os = "macos"))]
pub async fn regenerate_recap(
    _pool: &SqlitePool,
    _app_data_dir: &std::path::Path,
    _call_id: &str,
    _app: Option<&AppHandle>,
) -> Result<(), AppError> {
    Err(AppError::Other(
        "Локальный движок недоступен на этой платформе (только macOS, R9).".into(),
    ))
}

/// [DRY] Собрать `LocalLlamaProvider` для активного preset'а: гейт готовности
/// движка (`readiness::assert_ready`) → speculative draft gate → build с
/// per-preset timeout + AppHandle. Возвращает `(provider, preset)`
/// (`LocalEnginePreset` — Copy). Используется `regenerate_recap_local` +
/// `title_regen` (local path) и маршрутом записи через `run_recap`.
#[cfg(target_os = "macos")]
pub async fn build_local_llm_provider(
    pool: &SqlitePool,
    app_data_dir: &std::path::Path,
    app: &AppHandle,
    s: &PipelineSettings,
    // [TD-36] Метка звонка для очереди ресурсов: QueueMonitor показывает, чей
    // звонок сейчас у LLM. Раньше её ставил только маршрут записи — потому
    // что он собирал провайдера сам, мимо этой функции.
    call_id: Option<&str>,
) -> Result<
    (
        crate::local_engine::llm::LocalLlamaProvider,
        crate::local_engine::preset::LocalEnginePreset,
    ),
    AppError,
> {
    use crate::local_engine::{
        llm::LocalLlamaProvider,
        models::{self, ModelId},
        preset::LocalEnginePreset,
        readiness,
    };

    // Единый гейт готовности: пресет выбран и все обязательные модули на диске.
    // До этого здесь жила своя копия проверки — та же, что в `prepare_local_run`,
    // с теми же текстами и своим набором моделей.
    readiness::assert_ready(pool, app_data_dir).await?;
    let preset = readiness::active_preset(pool)
        .await?
        .ok_or_else(|| AppError::Other("local_engine_preset_not_set".into()))?;
    let llm_id = preset.llm_model_id();

    // Speculative decoding gate (mirror run_local_inner).
    let draft_path: Option<std::path::PathBuf> =
        if s.summary_speculative_decoding && preset == LocalEnginePreset::Quality {
            Some(models::model_path(
                app_data_dir,
                ModelId::QWEN25_0_5B.as_str(),
            ))
        } else {
            None
        };

    // [P1.3] Per-preset timeout: Light 5min / Balanced 10min / Quality 15min.
    let mut provider = LocalLlamaProvider::for_preset(app_data_dir, llm_id)
        .with_timeout(crate::local_engine::llm::timeout_for_preset(preset));
    if let Some(id) = call_id {
        provider = provider.with_call(id.to_string());
    }
    let provider = provider
        .with_app(app.clone())
        .await
        .with_draft_model(draft_path);

    // [B2] Если resident llama-server поднят для ЭТОГО preset — provider пойдёт
    // HTTP-путём (модель уже в RAM), без спавна one-shot процесса. Иначе None →
    // обычный one-shot.
    let server = {
        let state = tauri::Manager::state::<crate::state::AppState>(app);
        let guard = state.llm_server.lock().await;
        guard
            .as_ref()
            .filter(|srv| srv.preset() == preset)
            // [TD-08] Забираем и url, и api-key: сервер теперь требует авторизацию.
            .map(|srv| crate::local_engine::llm::ServerHandle {
                url: srv.url().to_string(),
                api_key: srv.api_key().to_string(),
            })
    };
    let provider = provider.with_server(server);

    Ok((provider, preset))
}

/// [warm-up B1] Прогрев local-LLM при старте приложения: один крошечный
/// `generate`, чтобы llama.cpp скомпилил Metal-шейдеры (~30с, разово после
/// апгрейда бинаря) и загрузил модель в page-cache ДО первого рекапа. Иначе
/// этот cold-start падал на первую пользовательскую генерацию. Best-effort:
/// движок не Local / нет preset'а / нет модели / ошибка генерации — не фатально,
/// только лог. Держит LLM-семафор на время вызова (короткий prompt).
#[cfg(target_os = "macos")]
pub async fn warm_up_local_llm(pool: &SqlitePool, app_data_dir: &Path, app: &AppHandle) {
    use crate::providers::llm::{LlmProvider, LlmRequest};

    let s = match PipelineSettings::load(pool).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("warm-up: settings load failed (skip): {e}");
            return;
        }
    };
    // [B2] Резидентный режим: вместо разового прогрева поднимаем llama-server —
    // модель остаётся в RAM всю сессию (сам старт сервера и есть прогрев).
    if keep_resident_enabled(pool).await {
        start_resident_server(app, pool, app_data_dir).await;
        return;
    }
    let (provider, preset) = match build_local_llm_provider(pool, app_data_dir, app, &s, None).await
    {
        Ok(p) => p,
        Err(e) => {
            log::info!("warm-up: local LLM недоступен (skip): {e}");
            return;
        }
    };
    let started = std::time::Instant::now();
    let request = LlmRequest {
        model: None,
        system: "You are a warm-up ping. Reply with a single empty JSON object.".to_string(),
        input: "{}".to_string(),
        max_tokens: Some(8),
        grammar: None,
        json_schema: None,
    };
    match provider.generate(request).await {
        Ok(_) => log::info!(
            "warm-up: local LLM прогрет (preset={preset:?}) за {}ms — Metal-шейдеры + модель в кэше",
            started.elapsed().as_millis()
        ),
        Err(e) => log::info!("warm-up: прогрев завершился с ошибкой (не фатально): {e}"),
    }
}

/// [B2] Settings-ключ тумблера «держать модель активной».
#[cfg(target_os = "macos")]
pub const SETTING_KEEP_RESIDENT: &str = "local_engine.keep_resident";

/// [B2] Включена ли резидентная модель.
#[cfg(target_os = "macos")]
pub async fn keep_resident_enabled(pool: &SqlitePool) -> bool {
    db::get_setting(pool, SETTING_KEEP_RESIDENT)
        .await
        .ok()
        .flatten()
        .as_deref()
        == Some("1")
}

/// [B2] Резолв активного preset'а из settings (для запуска сервера).
#[cfg(target_os = "macos")]
async fn resolve_active_preset(
    pool: &SqlitePool,
) -> Option<crate::local_engine::preset::LocalEnginePreset> {
    use crate::local_engine::preset::{LocalEnginePreset, SETTING_ACTIVE_PRESET};
    let s = db::get_setting(pool, SETTING_ACTIVE_PRESET)
        .await
        .ok()
        .flatten()?;
    LocalEnginePreset::from_str(&s)
}

/// [B2] Поднять resident `llama-server` и сохранить handle в AppState.
/// Идемпотент: если уже поднят для того же preset — no-op. При смене preset —
/// гасит старый и стартует новый. Возвращает `true` при успехе (иначе caller
/// продолжит на one-shot пути). Не фатально ни при какой ошибке.
#[cfg(target_os = "macos")]
pub async fn start_resident_server(
    app: &AppHandle,
    pool: &SqlitePool,
    app_data_dir: &Path,
) -> bool {
    use crate::local_engine::llm_server::LlamaServer;
    let Some(preset) = resolve_active_preset(pool).await else {
        log::info!("resident server: preset не выбран — skip");
        return false;
    };
    let state = tauri::Manager::state::<crate::state::AppState>(app);
    {
        let guard = state.llm_server.lock().await;
        if guard.as_ref().map(|s| s.preset()) == Some(preset) {
            return true; // уже поднят для этого preset
        }
    }
    // Не поднят либо другой preset — гасим старый и стартуем.
    stop_resident_server(app).await;
    match LlamaServer::start(app, app_data_dir, preset).await {
        Ok(server) => {
            *state.llm_server.lock().await = Some(server);
            true
        }
        Err(e) => {
            log::warn!("resident server start failed (fallback one-shot): {e}");
            false
        }
    }
}

/// [B2] Остановить resident-сервер, если поднят.
#[cfg(target_os = "macos")]
pub async fn stop_resident_server(app: &AppHandle) {
    let state = tauri::Manager::state::<crate::state::AppState>(app);
    let prev = state.llm_server.lock().await.take();
    if let Some(server) = prev {
        server.shutdown();
    }
}

/// [Bug-fix] Local engine path для `regenerate_recap`. Mirror блока в
/// `run_local_inner` (preset resolve → model presence check → LocalLlamaProvider
/// build → local_orchestrator → persist_recap_from_json), но БЕЗ STT/merge
/// stages (transcript.md уже есть). Errors propagate как AppError —
/// regenerate_recap setter caller персистит failed_reason + возвращает Err
/// в UI.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)] // регенерация — internal helper; structured args = backlog
pub(crate) async fn regenerate_recap_local(
    pool: &SqlitePool,
    app_data_dir: &std::path::Path,
    call_dir: &std::path::Path,
    call_id: &str,
    transcript_md: &str,
    lang_detected: Option<&str>,
    app: &AppHandle,
    s: &PipelineSettings,
) -> Result<(), AppError> {
    if transcript_md.trim().is_empty() {
        return Err(AppError::Other("local_engine_transcript_empty".into()));
    }

    let (provider, preset) =
        build_local_llm_provider(pool, app_data_dir, app, s, Some(call_id)).await?;
    // [Q] call_id → LLM-очередь (QueueMonitor видит чей звонок у llama).
    let provider = provider.with_call(call_id);
    let llm_id = preset.llm_model_id();
    log::info!(
        "regenerate_recap_local: call_id={} preset={:?} llm_id={} whisper_id={}",
        call_id,
        preset,
        llm_id.as_str(),
        preset.whisper_model_id().as_str(),
    );

    // [F2] Переписать заголовки подтверждённых спикеров на имена контактов +
    // person-level Known participants блок. На DB-ошибке — fallback на сырой
    // транскрипт без блока (recap важнее идентификации).
    let (prompt_transcript, known_speakers) =
        match speaker_prompt_ctx::build_prompt_transcript(pool, call_id, transcript_md).await {
            Ok(pair) => pair,
            Err(e) => {
                log::warn!("speaker_prompt_ctx failed (fallback to raw tags): {e}");
                (transcript_md.to_string(), None)
            }
        };

    // [F3] Step-события для thinking-блока UI.
    let step_sink = recap_steps::BusStepSink {
        app: Some(app.clone()),
        call_id: call_id.to_string(),
    };
    let orch_ctx = local_orchestrator::LocalOrchestratorCtx {
        transcript_md: &prompt_transcript,
        lang_detected,
        known_speakers: known_speakers.as_deref(),
        preset,
        steps: &step_sink,
    };
    // [P1.3] Wrap LLM future в periodic recap:progress emitter. UI рендерит
    // «Пересоздаём… {sec}s»; на completion (success / fail / timeout) ticker
    // аборт'ится через JoinHandle::abort внутри helper'а.
    let outcome = recap_progress::with_recap_progress_emitter(
        Some(app.clone()),
        call_id.to_string(),
        local_orchestrator::run_v2_pipeline(&provider, orch_ctx),
    )
    .await
    .map_err(|e| AppError::Other(format!("local_engine_llm_failed: {e}")))?;

    let local_engine_label = local_engine_label(llm_id.as_str());
    recap::persist_recap_from_json(
        pool,
        call_id,
        call_dir,
        outcome.summary_json,
        local_engine_label,
        // [F2] Evidence-валидатор матчит против текста, который видел LLM.
        &prompt_transcript,
        None,
        Some(s.summary_v2_enabled),
        outcome.pipeline_mode,
    )
    .await?;

    // Storage UI «активно X дней назад».
    let _ =
        crate::local_engine::models::touch_usage(pool, preset.whisper_model_id().as_str()).await;
    let _ = crate::local_engine::models::touch_usage(pool, llm_id.as_str()).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_label_maps_every_catalog_size() {
        // Ярлык уходит в calls.summary_engine и в баннер рекапа: перепутанный
        // размер модели — это не косметика, а неверная строка в истории звонка.
        assert_eq!(
            local_engine_label("qwen2.5-1.5b-instruct"),
            "local-qwen-1.5b"
        );
        assert_eq!(
            local_engine_label("qwen2.5-1_5b-instruct"),
            "local-qwen-1.5b"
        );
        assert_eq!(local_engine_label("qwen2.5-3b-instruct"), "local-qwen-3b");
        assert_eq!(local_engine_label("qwen2.5-7b-instruct"), "local-qwen-7b");
    }

    #[test]
    fn engine_label_falls_back_without_lying_about_size() {
        // Незнакомый id — общий ярлык, а не «самый популярный размер».
        assert_eq!(local_engine_label("gemma-2-2b-it"), "local-qwen");
        assert_eq!(local_engine_label(""), "local-qwen");
    }

    #[test]
    fn engine_label_prefers_exact_size_over_substring_order() {
        // «1.5b» содержит «5b», но не «3b»/«7b» — порядок веток в match
        // единственное, что удерживает 1.5B от ярлыка обычного размера.
        assert_eq!(local_engine_label("model-1.5b-q4"), "local-qwen-1.5b");
    }
}
