//! [TD-35] Локальный маршрут обработки звонка: от подготовки моделей до
//! рекапа.
//!
//! Выделено из `pipeline/mod.rs` (1914 строк при лимите 800, правило 8).
//! В `mod.rs` остались маршруты и их обвязка — то, что читают, когда
//! разбираются «куда пошёл звонок»; здесь то, что читают, когда разбираются
//! «что именно с ним сделали». Логика не менялась.

use std::path::Path;

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::{db, AppError};

use super::diarize::{diarize_mic_track, diarize_system_track, relabel_owner_on_mic_full_file};
use super::merge::OWNER_TAG;
use super::settings::PipelineSettings;
use super::stage::Stage;
use super::{
    audio_merger, call_language, chunk_assembly, chunks_ready, emit_progress,
    ensure_anonymous_speakers_present, local_orchestrator, recap, recap_progress, recap_steps,
    run_auto_bind, run_stage, speaker_prompt_ctx, stage_merge_artifacts, stage_recognize_speakers,
    stage_upload, ChunkGate, PipelineCtx,
};
use crate::providers::transcription::{TranscriptionOpts, TranscriptionProvider};

/// [M12.6 Phase 3] Local-engine pipeline route. Полностью offline:
/// LocalWhisperProvider (whisper.cpp sidecar) → merge → cluster (WeSpeaker
/// in-process) → LocalLlamaProvider (llama.cpp sidecar) → recap.md.
///
/// Контракт ошибок per PRD §M12.6.5:
/// - missing model → `local_engine_model_missing`
/// - sidecar/STT crash → `local_engine_stt_failed`
/// - LLM crash → `local_engine_llm_failed`
/// - timeout → `local_whisper_timeout` / `local_llm_timeout`
///
/// UI (M12.5) показывает Cloud-fallback offer по этим маркерам.
#[cfg(target_os = "macos")]
pub(super) async fn run_local_inner(
    pool: &SqlitePool,
    ctx: &PipelineCtx,
    app: Option<&AppHandle>,
    s: &PipelineSettings,
) -> Result<(), AppError> {
    use crate::local_engine::{
        llm::LocalLlamaProvider,
        models::{self, ModelStatus},
        preset::{LocalEnginePreset, SETTING_ACTIVE_PRESET},
        stt::{LocalWhisperProvider, TrackKind},
    };

    let app = app.ok_or_else(|| {
        AppError::Other("local_engine_no_app_handle: pipeline requires Tauri runtime".into())
    })?;

    // 1. Резолвим preset → model ids. Без preset (юзер прошёл onboarding но
    //    выбрал Cloud, потом откатился) — fail с явным reason.
    let preset_str = db::get_setting(pool, SETTING_ACTIVE_PRESET).await?;
    let preset = preset_str
        .as_deref()
        .and_then(LocalEnginePreset::from_str)
        .ok_or_else(|| {
            AppError::Other(
                "local_engine_preset_not_set: выберите Light/Balanced/Quality в Settings → Движок"
                    .into(),
            )
        })?;
    let whisper_id = preset.whisper_model_id();
    let llm_id = preset.llm_model_id();

    // 2. Проверяем что обе модели на диске (exact-size против каталога).
    //    [perf] Полный SHA — только на download-пути (M12.4); ежепрогонное
    //    хеширование whisper+LLM (~1.5-6GB) держало UI на «Сохраняем аудио»
    //    десятки секунд при каждом звонке/reprocess.
    for id in [whisper_id, llm_id] {
        let status = models::check_status_fast(&ctx.app_data_dir, id.as_str()).await?;
        if !matches!(status, ModelStatus::Present { .. }) {
            return Err(AppError::Other(format!(
                "local_engine_model_missing: модель {} не установлена, скачайте в Settings → Движок",
                id.as_str()
            )));
        }
    }

    // 3. Stage Upload — pseudo-step (audio verify + sidecar model load занимают
    //    1-2 сек, UI не получает per-byte progress).
    let upload_hint = audio_byte_total(&ctx.mic_path, &ctx.system_path).await;
    run_stage(
        pool,
        Some(app),
        &ctx.call_id,
        Stage::Upload,
        upload_hint,
        async { stage_upload(upload_hint).await },
    )
    .await?;

    // 4. Stage Transcribe — mic + system параллельно через whisper-cli sidecar.
    //
    // [P13] Halt gate — если есть failed chunks, не идём дальше step 2.
    // Strict all-or-nothing: pipeline не должен build'ить partial transcript.
    // 0 chunks → Ok (non-chunked path, halt не релевантен) — fall back на
    // full-file STT ниже. После retry failed chunks → P11.1 auto-resume
    // re-войдёт сюда + halt пройдёт.
    // [M13 fix] Relaxed gate: partial транскрипт лучше total loss. Раньше
    // `ensure_all_chunks_done` валил ВЕСЬ pipeline (и транскрипт, и merge)
    // если хотя бы один chunk failed → плеер застревал на chunk 0 (~10 мин).
    let gate = chunks_ready(pool, &ctx.call_id).await?;

    // [Tech-debt P0.1 + M13 fix] Audio merge независим от полноты транскрипта:
    // склеиваем chunk WAV'ы в root чтобы плеер получил полную длину даже при
    // partial транскрипте. Sidecar пишет аудио в chunks/{idx}/*.wav; merge
    // сканирует диск (не DB) и включает даже chunk'и с failed STT. NoChunks →
    // skip (non-chunked full-file запись, chunks/ нет). Blocking pool — hound
    // синхронен, для 1-часовой записи 16kHz mono ≈ 115 MB RAM + ~1-2s CPU.
    if !matches!(gate, ChunkGate::NoChunks) {
        let chunks_dir = ctx.call_dir.join("chunks");
        let call_dir_clone = ctx.call_dir.clone();
        let call_id_clone = ctx.call_id.clone();
        tokio::task::spawn_blocking(move || {
            let (mic_r, sys_r) = audio_merger::merge_both_tracks(&chunks_dir, &call_dir_clone);
            if mic_r.is_none() && sys_r.is_none() {
                log::warn!(
                    "audio_merger: оба merge упали для call {call_id_clone} — \
                     плеер будет играть старый root WAV (если есть)"
                );
            }
        })
        .await
        .ok();
    }

    // Halt только если КАЖДЫЙ chunk провален (нечего собирать). Partial —
    // предупреждаем и строим транскрипт из done-подмножества.
    match &gate {
        ChunkGate::NoneDone { total } => {
            return Err(AppError::Other(format!(
                "chunks_need_retry: 0 of {total} chunks done (все провалены — retry в UI)"
            )));
        }
        ChunkGate::Partial {
            done,
            total,
            failed,
        } => {
            log::warn!(
                "call {}: partial transcript — {done}/{total} chunks done, failed idx {failed:?}",
                ctx.call_id
            );
        }
        _ => {}
    }

    // [M13.1.5d] Если за время записи chunk_orchestrator насобирал per-chunk
    // транскрипты (CHUNKED_PIPELINE=ON в start_recording) — пропускаем
    // full-file STT и собираем mic/sys из DB. Cloud engine сюда не доходит
    // (run_inner ветка), для local engine с chunked OFF в `call_chunks`
    // ничего нет → assembly возвращает None → fall back на full-file STT.
    let (mic_t, sys_t) = match chunk_assembly::load_chunked_transcripts(pool, &ctx.call_id).await? {
        Some(tracks) => {
            log::info!(
                "call {}: using chunked transcripts (skip full-file STT)",
                ctx.call_id
            );
            // Audio уже merged выше (независимо от транскрипт-полноты).
            // UI ожидает progress на Stage::Transcribe — эмитим 100%
            // мгновенно чтобы прогресс-бар не висел.
            let step = Stage::Transcribe.step();
            emit_progress(pool, Some(app), &ctx.call_id, step, 100, None, None).await;
            tracks
        }
        None => {
            let mic_stt = LocalWhisperProvider::for_preset(
                &ctx.app_data_dir,
                whisper_id,
                TrackKind::MicOwner,
            )
            .with_call(ctx.call_id.clone())
            .with_app(app.clone())
            .await;
            let sys_stt =
                LocalWhisperProvider::for_preset(&ctx.app_data_dir, whisper_id, TrackKind::System)
                    .with_call(ctx.call_id.clone())
                    .with_app(app.clone())
                    .await;
            let opts = TranscriptionOpts {
                lang: s.stt_lang.clone(),
                diarization: true,
                prompt: None,
            };
            let (mut mic, mut sys) = run_stage(
                pool,
                Some(app),
                &ctx.call_id,
                Stage::Transcribe,
                None,
                async {
                    let mic_fut = mic_stt.transcribe(&ctx.mic_path, opts.clone());
                    let sys_fut = sys_stt.transcribe(&ctx.system_path, opts.clone());
                    let (mic_r, sys_r) = tokio::join!(mic_fut, sys_fut);
                    let mic = mic_r.map_err(|e| {
                        AppError::Other(format!("local_engine_stt_failed (mic): {e}"))
                    })?;
                    let sys = sys_r.map_err(|e| {
                        AppError::Other(format!("local_engine_stt_failed (system): {e}"))
                    })?;
                    Ok::<_, AppError>((mic, sys))
                },
            )
            .await?;

            // [P-fix4] Auto-режим: пин языка звонка на ОБА трека. STT детектит
            // язык на каждом треке независимо; тихий старт mic (владелец слушает)
            // → mis-detect «en» → русская речь уходит в [FOREIGN]. Звонок
            // одноязычный — определяем язык по треку с речью (system как якорь:
            // собеседник обычно говорит чётко с начала) и перезапускаем трек,
            // чей детект отличается. explicit stt_lang сюда не попадает.
            if s.stt_lang == "auto" {
                if let Some(call_lang) = call_language(&mic, &sys) {
                    let pinned = TranscriptionOpts {
                        lang: call_lang.clone(),
                        diarization: true,
                        prompt: None,
                    };
                    if mic.lang_detected.as_deref() != Some(call_lang.as_str()) {
                        // [TD-15] Err-ветка обязательна: при провале повторного
                        // STT (timeout сайдкара, OOM) звонок молча оставался с
                        // mis-detected языком — тем самым [FOREIGN]-спамом, ради
                        // которого фича и писалась, — и по логам нельзя было
                        // понять, что re-STT вообще запускался.
                        match mic_stt.transcribe(&ctx.mic_path, pinned.clone()).await {
                            Ok(re) => {
                                log::info!(
                                    "call {}: re-STT mic pinned lang={call_lang} (was {:?})",
                                    ctx.call_id,
                                    mic.lang_detected
                                );
                                mic = re;
                            }
                            Err(e) => log::warn!(
                                "call {}: re-STT mic (lang={call_lang}) failed, \
                                 оставляем mis-detected {:?}: {e}",
                                ctx.call_id,
                                mic.lang_detected
                            ),
                        }
                    }
                    if sys.lang_detected.as_deref() != Some(call_lang.as_str()) {
                        // [TD-15] См. mic-ветку выше.
                        match sys_stt.transcribe(&ctx.system_path, pinned).await {
                            Ok(re) => {
                                log::info!(
                                    "call {}: re-STT system pinned lang={call_lang} (was {:?})",
                                    ctx.call_id,
                                    sys.lang_detected
                                );
                                sys = re;
                            }
                            Err(e) => log::warn!(
                                "call {}: re-STT system (lang={call_lang}) failed, \
                                 оставляем mis-detected {:?}: {e}",
                                ctx.call_id,
                                sys.lang_detected
                            ),
                        }
                    }
                }
            }
            (mic, sys)
        }
    };

    let lang_detected = mic_t
        .lang_detected
        .clone()
        .or_else(|| sys_t.lang_detected.clone());

    // 4.5. [M12-D5] Multi-speaker diarization на system track. До этого
    //    шага все system segments имеют `speaker:0` (TrackKind::System default
    //    в [`local_engine::stt`]). Sherpa-onnx pyannote segmentation + WeSpeaker
    //    embedding кластеризует фрагменты → каждый sys segment получает
    //    `speaker:0..4` (cap=4). Diarization non-fatal: при отсутствии моделей
    //    или voice-onnx feature off → fall back на оригинальный sys_t,
    //    система-трек остаётся single-bucket (degraded но рабочий).
    // [P1.2 / TD-36] Force-N-speakers Labs override (read once, applied к mic +
    // system). Разбор общий с обвязкой чанков: до этого здесь стоял свой
    // `1..=4`, и «4 голоса» работали в одном пути записи и молча
    // игнорировались в другом.
    let num_speakers_override = super::settings::read_num_speakers_override(pool).await?;
    let sys_t = diarize_system_track(
        &ctx.app_data_dir,
        &ctx.system_path,
        sys_t,
        num_speakers_override,
        &ctx.call_id,
    )
    .await;

    // 4.6. [M13 follow-up] Опциональный multi-voice на mic-дорожке. Default ON
    //    через `MIC_DIARIZATION_ENABLED`. Без этого вся mic уходила в OWNER_TAG
    //    через assemble_transcript (force_owner_track в local_engine::merge).
    //    С включенной настройкой sortformer выдаёт `speaker:N` tags, потом
    //    owner_identify::identify_owner_speaker переименовывает один из них
    //    в OWNER_TAG. На non-chunked пути embeddings собираем здесь же через
    //    extract_clusters; cross-track reflection (owner отражается в system)
    //    не обрабатывается без global reclustering — это limitation
    //    non-chunked path, acceptable т.к. чанкед = default.
    // [P-fix7] mic-диаризация по умолчанию ВЫКЛ. Mic = микрофон владельца =
    // один человек (M2.4); sortformer на нём овершутит, дробя единственный
    // голос owner'а в speaker:unknown/N → owner размазан по «СПИКЕР ?».
    // Opt-in только для нескольких людей у одного микрофона (Labs).
    let mic_on = matches!(
        db::get_setting(pool, "mic_diarization_enabled")
            .await?
            .as_deref(),
        Some("1") | Some("true")
    );
    let mic_diarization = mic_on;
    let mic_t = if mic_diarization {
        let mic_diarized = diarize_mic_track(
            &ctx.app_data_dir,
            &ctx.mic_path,
            mic_t,
            num_speakers_override,
            &ctx.call_id,
        )
        .await;
        relabel_owner_on_mic_full_file(
            pool,
            &ctx.app_data_dir,
            &ctx.mic_path,
            &ctx.system_path,
            mic_diarized,
        )
        .await
    } else {
        mic_t
    };

    // 5. Stage MergeArtifacts — переиспользуем cloud helper (он не знает про
    //    engine, просто пишет transcript.md + raw_stt.json).
    let merged = run_stage(
        pool,
        Some(app),
        &ctx.call_id,
        Stage::MergeArtifacts,
        None,
        async { stage_merge_artifacts(&ctx.call_dir, &mic_t, &sys_t).await },
    )
    .await?;

    db::set_call_meta(pool, &ctx.call_id, lang_detected.as_deref(), "local").await?;

    let owner = db::ensure_owner_contact(pool).await?;
    if let Err(e) = db::auto_bind_owner_speaker(pool, &ctx.call_id, &owner.id, OWNER_TAG).await {
        log::warn!("auto_bind_owner_speaker {} failed: {e}", ctx.call_id);
    }
    ensure_anonymous_speakers_present(pool, &ctx.call_id, &merged).await;

    // 6. Stage RecognizeSpeakers — WeSpeaker cluster (B3.x) переиспользуется
    //    как для cloud-движка. Diarization для local пока упрощённая (system
    //    track всё в speaker:0) — кластер видит «одного» дополнительного спикера
    //    но это ок: voice biometrics matching работает на сэмплах per-call.
    let cluster_result = run_stage(
        pool,
        Some(app),
        &ctx.call_id,
        Stage::RecognizeSpeakers,
        None,
        async { stage_recognize_speakers(pool, ctx, &merged).await },
    )
    .await;
    if let Err(e) = cluster_result {
        log::warn!(
            "cluster pipeline {} failed (non-fatal — skip voice match): {e}",
            ctx.call_id
        );
    }

    if let Err(e) = run_auto_bind(pool, Some(app), &ctx.call_id, s).await {
        log::warn!("auto_bind {} failed (non-fatal): {e}", ctx.call_id);
    }

    // 7. Stage Recap — local LLM. Failed_reason set + recap.md skipped при
    //    ошибке; pipeline всё равно завершает Ok(()) (M4 паспорта — recap
    //    деривативная штука, регенерация по кнопке).
    let recap_step = Stage::Recap.step();
    emit_progress(pool, Some(app), &ctx.call_id, recap_step, 0, None, None).await;

    // Transcript.md обязан существовать — `stage_merge_artifacts` его пишет.
    // Если файл недоступен (race / disk issue), recap должен fail с явным
    // reason, а не silently дёрнуть LLM на пустом входе (получится пустой recap).
    let transcript_md_read = tokio::fs::read_to_string(ctx.call_dir.join("transcript.md")).await;

    // [F2] Переписать заголовки подтверждённых спикеров на имена контактов +
    // person-level Known participants блок. На DB-ошибке — fallback на сырой
    // транскрипт. Evidence-валидатор дальше матчит против переписанного текста
    // (тот, что реально видел LLM).
    let (prompt_transcript, known_speakers) = match &transcript_md_read {
        Ok(md) => match speaker_prompt_ctx::build_prompt_transcript(pool, &ctx.call_id, md).await {
            Ok(pair) => pair,
            Err(e) => {
                log::warn!("speaker_prompt_ctx failed (fallback to raw tags): {e}");
                (md.clone(), None)
            }
        },
        Err(_) => (String::new(), None),
    };
    let transcript_for_evidence = prompt_transcript.clone();
    // [M14 T-04 + T-10 Phase A] Local engine orchestrator: classifier (lightweight
    // ~256 tokens) → main v2 generation с known_call_type hint. На classifier
    // failure orchestrator делает fallback на single-pass без hint.
    // LOCAL_LLM_SYSTEM_PROMPT (legacy v1 ad-hoc) больше не используется на
    // этом path — local теперь идёт через тот же build_v2_system_prompt что
    // и cloud (с CallType hint от классификатора).
    let llm_result = match transcript_md_read {
        Ok(transcript_md) if !transcript_md.trim().is_empty() => {
            // [M14 T-16 P2] Speculative decoding — pass draft model path
            // когда (а) flag enabled (Labs opt-in), (b) preset=Quality
            // (только 7B заметно выигрывает от 0.5B draft), (c) file existence
            // checked внутри provider (graceful fallback на non-speculative).
            let draft_path: Option<std::path::PathBuf> =
                if s.summary_speculative_decoding && preset == LocalEnginePreset::Quality {
                    Some(crate::local_engine::models::model_path(
                        &ctx.app_data_dir,
                        crate::local_engine::models::ModelId::QWEN25_0_5B.as_str(),
                    ))
                } else {
                    None
                };
            // [P1.3] Per-preset timeout (Light 5min / Balanced 10min / Quality 15min).
            let provider = LocalLlamaProvider::for_preset(&ctx.app_data_dir, llm_id)
                .with_call(ctx.call_id.clone())
                .with_timeout(crate::local_engine::llm::timeout_for_preset(preset))
                .with_app(app.clone())
                .await
                .with_draft_model(draft_path);
            // [F3] Step-события для thinking-блока UI.
            let step_sink = recap_steps::BusStepSink {
                app: Some(app.clone()),
                call_id: ctx.call_id.clone(),
            };
            let orch_ctx = local_orchestrator::LocalOrchestratorCtx {
                // [F2] Переписанный транскрипт — LLM видит имена контактов.
                transcript_md: &prompt_transcript,
                lang_detected: lang_detected.as_deref(),
                known_speakers: known_speakers.as_deref(),
                // [M14 T-05/T-06 Phase B] Pass active preset для chunker config —
                // длинные transcripts автоматически идут chunked-path.
                preset,
                steps: &step_sink,
            };
            // [P1.3] Wrap LLM future в periodic recap:progress emitter
            // (mirror regenerate_recap_local). UI рендерит «Пересоздаём… {sec}s».
            recap_progress::with_recap_progress_emitter(
                Some(app.clone()),
                ctx.call_id.clone(),
                local_orchestrator::run_v2_pipeline(&provider, orch_ctx),
            )
            .await
            .map_err(|e| crate::providers::llm::LlmError::Provider(e.to_string()))
        }
        Ok(_) => Err(crate::providers::llm::LlmError::Provider(
            "local_engine_transcript_empty".into(),
        )),
        Err(e) => Err(crate::providers::llm::LlmError::Provider(format!(
            "local_engine_transcript_read: {e}"
        ))),
    };

    // [P5.1] Label нужен в обеих ветках (success persist + failure atomic
    // UPDATE). [TD-36] Вывод ярлыка — общий с путём регенерации.
    let local_engine_label = super::local_llm::local_engine_label(llm_id.as_str());

    match llm_result {
        Ok(outcome) => {
            // [M14 T-02] persist_recap_from_json теперь требует engine_label +
            // transcript_md (для evidence validator) + generation_ms (None
            // на local path; в T-04+ доделаем).
            if let Err(e) = recap::persist_recap_from_json(
                pool,
                &ctx.call_id,
                &ctx.call_dir,
                outcome.summary_json,
                local_engine_label,
                &transcript_for_evidence,
                None,
                // [M14 T-04 Phase A] Local path теперь emit'ит telemetry —
                // classifier + main v2 pipeline через local_orchestrator.
                Some(s.summary_v2_enabled),
                outcome.pipeline_mode,
            )
            .await
            {
                // [P5.1] Engine label atomic с reason — banner consistent.
                let _ = db::set_recap_failure(
                    pool,
                    &ctx.call_id,
                    Some(&format!("local_engine_recap_persist: {e}")),
                    Some(local_engine_label),
                )
                .await;
            } else {
                let _ = db::set_recap_failed_reason(pool, &ctx.call_id, None).await;
                // Storage UI «активно X дней назад».
                let _ = models::touch_usage(pool, whisper_id.as_str()).await;
                let _ = models::touch_usage(pool, llm_id.as_str()).await;
            }
        }
        Err(e) => {
            let reason = format!("local_engine_llm_failed: {e}");
            log::warn!("{reason}");
            // [P5.1] Engine label atomic с reason.
            let _ =
                db::set_recap_failure(pool, &ctx.call_id, Some(&reason), Some(local_engine_label))
                    .await;
        }
    }
    emit_progress(pool, Some(app), &ctx.call_id, recap_step, 100, None, None).await;

    Ok(())
}

/// [V6.2] Размер двух аудио-файлов в байтах — UI показывает «X МБ» в активити
/// strip'е. Best-effort: если файла нет (например только mic пишется), берём
/// что доступно. None если оба отсутствуют.
async fn audio_byte_total(mic: &Path, sys: &Path) -> Option<i64> {
    let mut total: i64 = 0;
    let mut seen = false;
    for path in [mic, sys] {
        if let Ok(meta) = tokio::fs::metadata(path).await {
            total = total.saturating_add(meta.len() as i64);
            seen = true;
        }
    }
    if seen {
        Some(total)
    } else {
        None
    }
}
