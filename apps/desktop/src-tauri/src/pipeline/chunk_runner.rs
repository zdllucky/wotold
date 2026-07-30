//! [M13.1.3b] Per-chunk pipeline для chunked pipelined transcription.
//!
//! Standalone async fn, который для одного chunk'а делает:
//! 1. `db::chunks::mark_chunk_processing` (FSM gate pending → processing)
//! 2. `provider.transcribe(mic_path, opts)` с `opts.prompt` (context priming
//!    из chunk N-1)
//! 3. Serialize `DiarizedTranscript` в JSON
//! 4. `db::chunks::mark_chunk_done(end_ms, transcript_json)`
//! 5. Extract last ~50 слов transcript'а → return как `transcript_tail` для
//!    `prev_prompt` следующего chunk'а
//!
//! На error: `mark_chunk_failed(reason)` + propagate `AppError`.
//!
//! Phase 1 scope: только STT. Diarization per-chunk + per-segment embeddings
//! для global re-clustering — Phase 2 (M13.2.*). chunk_runner будет туда
//! расширен без breaking changes сигнатуры (новые fields в Output).
//!
//! Wiring в recording flow (start_recording orchestration через
//! CHUNKED_PIPELINE feature flag) — M13.1.5c sprint (complete).

use std::path::PathBuf;

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::events::{ChunkDoneEvent, EventBus};
use crate::pipeline::chunk_lang::{extract_tail_words, pick_pinned_lang};
use crate::pipeline::clusters::load_and_extract_clusters;
use crate::providers::transcription::{
    DiarizedTranscript, TranscriptSegment, TranscriptionError, TranscriptionOpts,
    TranscriptionProvider,
};
use crate::{db, AppError};

/// Параметры для одного запуска `run_chunk`. Все владелие (нет lifetime'ов
/// в callsite'е), упрощает spawn в task'е.
#[derive(Debug, Clone)]
pub struct ChunkRunInput {
    pub call_id: String,
    pub chunk_idx: u32,
    /// Smещение start chunk'а от начала записи (ms). Передаётся через
    /// `insert_chunk` в DB row (откуда assembly читает через ChunkRow.start_ms),
    /// в самом `run_chunk` не используется напрямую.
    #[allow(dead_code)]
    pub start_ms: u64,
    /// End chunk'а (ms от начала записи). Известен после rotation на стороне
    /// orchestrator'а (chunk_start_next_ms = chunk_end_this_ms).
    pub end_ms: u64,
    pub mic_path: PathBuf,
    pub system_path: PathBuf,
    /// Контекст priming — последние ~50 слов transcript'а chunk N-1. `None`
    /// для idx=0 или если chunk N-1 failed (lose context — это OK, точность
    /// первой фразы падает ~10pp но overall transcript не страдает).
    pub prev_prompt: Option<String>,
    /// 'auto' или BCP47. Передаётся в provider.
    pub lang: String,
    /// [M13.2.1] App-data root — нужен resolve `models/embedder.onnx` для
    /// WeSpeaker. `None` в unit-тестах → embedder = StubEmbedder
    /// → empty embeddings_json в DB (degraded ok).
    pub app_data_dir: Option<PathBuf>,
    /// [M13.2.3] Tauri AppHandle для emit'а `transcript:chunk_done`. `None`
    /// в unit-тестах / headless — event silently no-op.
    pub app_handle: Option<AppHandle>,
    /// [M13 follow-up] Прогнать sortformer и по mic-дорожке для multi-voice
    /// записей. Default true. Owner-tagging НЕ применяется здесь — local
    /// `speaker:N` tags остаются, finalize'ится в chunk_assembly через
    /// `owner_identify::identify_owner_speaker`.
    pub mic_diarization: bool,
    /// [P1.2] Labs «Force N speakers» override для sortformer's `num_clusters`.
    /// `None` = auto-detect. `Some(N)` clamp'ится к 1..=MAX_LOCAL_SPEAKERS в
    /// `SortformerDiarizer::with_num_speakers`.
    pub mic_diarization_num_speakers: Option<i32>,
}

/// Результат успешного `run_chunk`. `transcript_tail` идёт в `prev_prompt`
/// следующего chunk'а. `segment_count` для логирования/UI prog ress.
#[derive(Debug, Clone)]
pub struct ChunkRunOutput {
    pub transcript_tail: String,
    /// Для логирования / Phase 3 UI прогресса. Caller (orchestrator) сейчас
    /// не использует — но tests + future code будут.
    #[allow(dead_code)]
    pub segment_count: usize,
}

/// Запустить chunk pipeline (STT only, dual-track в Phase 1). Транскрибирует
/// mic + system параллельно через два provider'а. Возвращает tail из mic'а
/// для prev_prompt следующего chunk'а.
///
/// Семантика ошибок:
/// - **Mic fail** — fatal: mark_chunk_failed + Err (owner voice = критичен).
/// - **System fail, mic ok** — degraded ok: mark_chunk_done с system=None,
///   warn log. Assembly обрабатывает None как пустой system track.
/// - **Both fail** — same as mic fail.
///
/// Diarization per-chunk + per-segment embeddings для global re-clustering —
/// Phase 2 (M13.2.*); chunk_runner будет расширен без breaking changes.
pub async fn run_chunk<P: TranscriptionProvider + ?Sized>(
    pool: &SqlitePool,
    mic_provider: &P,
    system_provider: &P,
    input: ChunkRunInput,
) -> Result<ChunkRunOutput, AppError> {
    let ChunkRunInput {
        call_id,
        chunk_idx,
        start_ms: _,
        end_ms,
        mic_path,
        system_path,
        prev_prompt,
        lang,
        app_data_dir,
        app_handle,
        mic_diarization,
        mic_diarization_num_speakers,
    } = input;
    let bus = EventBus::new(app_handle.as_ref());

    // 1. FSM gate: pending → processing.
    db::chunks::mark_chunk_processing(pool, &call_id, chunk_idx).await?;

    // [P12.3] Language pinning per-call. Если на предыдущем chunk'е
    // whisper уже задетектил язык (сохранено в calls.lang_detected) —
    // используем его независимо от lang argument. Это предотвращает
    // `[FOREIGN]` спам когда каждый chunk детектит язык независимо
    // и иногда ошибается.
    //
    // Override rules:
    //   - DB lang_detected non-null AND input lang == 'auto' → use DB.
    //   - DB lang_detected non-null AND input lang explicit (e.g. 'ru')
    //     → user explicit override wins, use input lang.
    //   - DB lang_detected null → use input lang as is.
    let effective_lang = if lang == "auto" {
        match db::get_call(pool, &call_id).await {
            Ok(Some(c)) => c.lang_detected.unwrap_or(lang.clone()),
            _ => lang.clone(),
        }
    } else {
        lang.clone()
    };

    // 2. Параллельный mic+system STT. Prompt-priming идёт только в mic —
    //    system track имеет другой speaker (собеседник), prev_prompt от
    //    owner-mic дал бы ложный bias.
    let mic_opts = TranscriptionOpts {
        lang: effective_lang.clone(),
        diarization: true,
        prompt: prev_prompt,
    };
    let sys_opts = TranscriptionOpts {
        lang: effective_lang,
        diarization: true,
        prompt: None,
    };

    let mic_fut = mic_provider.transcribe(&mic_path, mic_opts);
    let sys_fut = system_provider.transcribe(&system_path, sys_opts);
    let (mic_res, sys_res) = tokio::join!(mic_fut, sys_fut);

    let mic_transcript = match mic_res {
        Ok(t) => t,
        Err(e) => {
            let reason = format!("transcribe mic: {e}");
            // [M13 review fix] Log db-mark-failed error (но всё равно propagate
            // STT error). Без этого если pool exhausted, row застрял бы в
            // `processing` и причина silently swallow'нулась.
            if let Err(db_err) =
                db::chunks::mark_chunk_failed(pool, &call_id, chunk_idx, &reason).await
            {
                log::error!("chunk {call_id}/{chunk_idx} mark_failed after mic error: {db_err}");
            }
            // [M13.2.3] Emit chunk_done(status=failed) перед propagate — Phase 3
            // UI рендерит per-chunk статус strip.
            bus.transcript_chunk_done(&ChunkDoneEvent {
                call_id: call_id.clone(),
                chunk_idx,
                status: "failed",
                segment_count: 0,
            });
            return Err(translate_transcription_error(e));
        }
    };

    // System failure — degraded ok: piping logs + None в DB.
    let sys_transcript = match sys_res {
        Ok(t) => Some(t),
        Err(e) => {
            log::warn!("chunk {call_id}/{chunk_idx} system STT failed (degraded ok): {e}");
            None
        }
    };

    // [M13 follow-up] Если включена mic_diarization + есть app_data_dir →
    // прогоняем mic через sortformer. Получаем speaker:N tags вместо
    // STT-default'ного «owner»-эквивалента. Owner identification (M3.7
    // invariant) идёт после Phase 2 reclustering в chunk_assembly.
    // Degraded path: на macOS не-сборках или без моделей возвращается
    // mic_transcript без изменений → caller force_owner_track.
    let mic_transcript = if mic_diarization {
        #[cfg(target_os = "macos")]
        {
            match app_data_dir.as_deref() {
                Some(dir) => {
                    crate::pipeline::diarize_mic_track(
                        dir,
                        &mic_path,
                        mic_transcript,
                        mic_diarization_num_speakers,
                        &call_id,
                    )
                    .await
                }
                None => mic_transcript,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            mic_transcript
        }
    } else {
        mic_transcript
    };

    // 3. Serialize → DB persist.
    let mic_json = serde_json::to_string(&mic_transcript)
        .map_err(|e| AppError::Other(format!("mic transcript serialize: {e}")))?;
    let sys_json = match sys_transcript.as_ref() {
        Some(t) => Some(
            serde_json::to_string(t)
                .map_err(|e| AppError::Other(format!("system transcript serialize: {e}")))?,
        ),
        None => None,
    };

    let segment_count = mic_transcript.segments.len()
        + sys_transcript
            .as_ref()
            .map(|t| t.segments.len())
            .unwrap_or(0);
    let transcript_tail = extract_tail_words(&mic_transcript, 50);

    // 4. [M13.2.1] Извлечь per-chunk WeSpeaker cluster embeddings (mean-pooled
    // per-speaker_tag, L2-normalized). Non-fatal: ошибка лишь скипает embeddings
    // → assembly не сможет cross-chunk remap для этого chunk'а (identity).
    let embeddings_json = if let Some(dir) = app_data_dir.as_deref() {
        match build_chunk_embeddings_json(
            &mic_transcript,
            sys_transcript.as_ref(),
            &mic_path,
            &system_path,
            dir,
        )
        .await
        {
            Ok(j) => Some(j),
            Err(e) => {
                log::warn!("chunk {call_id}/{chunk_idx} embeddings extract failed (degraded): {e}");
                Some("{}".to_string())
            }
        }
    } else {
        // app_data_dir отсутствует — production-mode без resolve'а embedder'а
        // не имеет смысла, но в unit-тестах это нормально → persist пустой JSON
        // для consistency (None зарезервирован под legacy pre-Phase 2 rows).
        None
    };

    db::chunks::mark_chunk_done(
        pool,
        &call_id,
        chunk_idx,
        end_ms,
        &mic_json,
        sys_json.as_deref(),
        embeddings_json.as_deref(),
    )
    .await?;

    // [P12.3 + P-fix] Сохранить detected language на call row после успешного
    // chunk'а. Используется на последующих chunks как override 'auto'
    // (см. effective_lang logic выше) — предотвращает [FOREIGN] спам.
    //
    // [P-fix] Раньше пин брался безусловно из mic-трека. Тихий mic (owner
    // молчит) часто mis-детектит «en» → форсил «en» на весь звонок → русская
    // речь шла как [FOREIGN]. Теперь берём язык из трека с бóльшим объёмом
    // речи и только если он уверенный (≥ порога слов).
    if let Some(detected) = pick_pinned_lang(&mic_transcript, sys_transcript.as_ref()) {
        if let Err(e) = db::set_call_meta(pool, &call_id, Some(&detected), "local").await {
            log::warn!("chunk {call_id}/{chunk_idx}: set_call_meta(lang={detected}) failed: {e}");
        }
    }

    // [M13.2.3] Emit chunk_done(status=done) после persist'а.
    bus.transcript_chunk_done(&ChunkDoneEvent {
        call_id: call_id.clone(),
        chunk_idx,
        status: "done",
        segment_count: mic_transcript.segments.len(),
    });

    Ok(ChunkRunOutput {
        transcript_tail,
        segment_count,
    })
}

/// [M13.2.1] Собрать `HashMap<speaker_tag, Vec<f32>>` (JSON) из mic+system
/// transcript'ов одного chunk'а. Embedder резолвится внутри
/// `load_and_extract_clusters` (StubEmbedder если модель не скачана → empty
/// embeddings, identity remap). [TD-18] async: WAV+ONNX в blocking-пуле.
async fn build_chunk_embeddings_json(
    mic_transcript: &DiarizedTranscript,
    sys_transcript: Option<&DiarizedTranscript>,
    mic_path: &std::path::Path,
    system_path: &std::path::Path,
    app_data_dir: &std::path::Path,
) -> Result<String, AppError> {
    // Merged = все сегменты обоих дорожек. extract_clusters сам routes
    // owner-tagged → mic.wav, прочие → system.wav.
    let mut merged: Vec<TranscriptSegment> = Vec::with_capacity(
        mic_transcript.segments.len() + sys_transcript.map(|s| s.segments.len()).unwrap_or(0),
    );
    merged.extend(mic_transcript.segments.iter().cloned());
    if let Some(sys) = sys_transcript {
        merged.extend(sys.segments.iter().cloned());
    }
    let clusters = load_and_extract_clusters(
        merged,
        mic_path.to_path_buf(),
        system_path.to_path_buf(),
        app_data_dir,
        "chunk embeddings",
    )
    .await?;
    serde_json::to_string(&clusters)
        .map_err(|e| AppError::Other(format!("serialize chunk embeddings: {e}")))
}

fn translate_transcription_error(e: TranscriptionError) -> AppError {
    AppError::Other(format!("chunk transcription failed: {e}"))
}

#[cfg(test)]
mod tests {
    use crate::call_id::CallId;
    /// [TD-05] Тестовые id — каноничные v4: `CallStore` принимает только
    /// валидированный `CallId`, прежние литералы вроде "c1" им быть не могут.
    const TEST_CALL_A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
    #[allow(dead_code)]
    const TEST_CALL_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
    #[allow(dead_code)]
    const TEST_CALL_GHOST: &str = "99999999-9999-4999-8999-999999999999";
    #[allow(dead_code)]
    fn cid(s: &str) -> CallId {
        CallId::parse(s).expect("тестовый id должен быть каноничным uuid")
    }

    use super::*;
    use crate::db::test_support::fresh_db;
    use crate::providers::transcription::{
        DiarizedTranscript, TranscriptSegment, TranscriptionError, TranscriptionOpts,
        TranscriptionProvider,
    };
    use async_trait::async_trait;
    use sqlx::SqlitePool;
    use std::path::{Path, PathBuf};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };

    /// Mock provider — записывает last opts (для проверки prompt
    /// propagation) + возвращает заранее заготовленный transcript или error.
    struct MockProvider {
        last_opts: Arc<Mutex<Option<TranscriptionOpts>>>,
        transcript: Option<DiarizedTranscript>,
        fail_with: Option<TranscriptionError>,
        called: AtomicBool,
    }

    impl MockProvider {
        fn ok(transcript: DiarizedTranscript) -> Self {
            Self {
                last_opts: Arc::new(Mutex::new(None)),
                transcript: Some(transcript),
                fail_with: None,
                called: AtomicBool::new(false),
            }
        }
        fn err(e: TranscriptionError) -> Self {
            Self {
                last_opts: Arc::new(Mutex::new(None)),
                transcript: None,
                fail_with: Some(e),
                called: AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    impl TranscriptionProvider for MockProvider {
        async fn transcribe(
            &self,
            _audio_path: &Path,
            opts: TranscriptionOpts,
        ) -> Result<DiarizedTranscript, TranscriptionError> {
            self.called.store(true, Ordering::Relaxed);
            *self.last_opts.lock().unwrap() = Some(opts);
            if let Some(e) = self.fail_with.as_ref() {
                return Err(match e {
                    TranscriptionError::Provider(s) => TranscriptionError::Provider(s.clone()),
                    _ => TranscriptionError::Provider("mock failure".into()),
                });
            }
            Ok(self.transcript.clone().unwrap())
        }
    }

    fn fake_transcript(segments: Vec<&str>) -> DiarizedTranscript {
        DiarizedTranscript {
            version: 1,
            lang_detected: Some("ru".into()),
            duration_sec: 600.0,
            provider: "mock".into(),
            segments: segments
                .into_iter()
                .enumerate()
                .map(|(i, text)| TranscriptSegment {
                    start: i as f64 * 1.0,
                    end: (i as f64 + 1.0),
                    text: text.into(),
                    speaker_tag: "speaker:0".into(),
                    confidence: None,
                })
                .collect(),
        }
    }

    async fn setup_chunk(pool: &SqlitePool, call_id: &str, chunk_idx: u32) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 'recording', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(call_id)
        .execute(pool)
        .await
        .unwrap();
        db::chunks::insert_chunk(
            pool,
            call_id,
            chunk_idx,
            0,
            Path::new("/tmp/mic.wav"),
            Path::new("/tmp/system.wav"),
        )
        .await
        .unwrap();
    }

    fn input(call_id: &str, chunk_idx: u32, prev_prompt: Option<&str>) -> ChunkRunInput {
        ChunkRunInput {
            call_id: call_id.into(),
            chunk_idx,
            start_ms: chunk_idx as u64 * 600_000,
            end_ms: (chunk_idx as u64 + 1) * 600_000,
            mic_path: PathBuf::from("/tmp/mic.wav"),
            system_path: PathBuf::from("/tmp/system.wav"),
            prev_prompt: prev_prompt.map(String::from),
            lang: "auto".into(),
            // [M13.2.1] None в unit-тестах — embedder резолвится только в
            // production через pipeline ctx; tests stub'ают через cluster
            // pipeline отдельно.
            app_data_dir: None,
            app_handle: None,
            // [M13 follow-up] Off в unit-тестах — sortformer требует
            // app_data_dir + macOS sidecar.
            mic_diarization: false,
            mic_diarization_num_speakers: None,
        }
    }

    #[tokio::test]
    async fn success_path_marks_done_and_returns_tail() {
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, TEST_CALL_A, 0).await;
        let mic = MockProvider::ok(fake_transcript(vec!["Привет.", "Как дела?"]));
        let sys = MockProvider::ok(fake_transcript(vec!["Здравствуйте."]));
        let out = run_chunk(&db_t.pool, &mic, &sys, input(TEST_CALL_A, 0, None))
            .await
            .unwrap();
        assert!(out.transcript_tail.contains("Как дела"));
        // mic = 2 segments + sys = 1 segment.
        assert_eq!(out.segment_count, 3);
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, TEST_CALL_A)
            .await
            .unwrap();
        assert_eq!(rows[0].status, "done");
        assert_eq!(rows[0].end_ms, Some(600_000));
        assert!(rows[0].transcript_json.is_some());
        assert!(rows[0].system_transcript_json.is_some());
    }

    #[tokio::test]
    async fn prev_prompt_propagated_to_mic_only() {
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, TEST_CALL_A, 1).await;
        let mic = MockProvider::ok(fake_transcript(vec!["Дальше."]));
        let sys = MockProvider::ok(fake_transcript(vec!["Ответ."]));
        let _ = run_chunk(
            &db_t.pool,
            &mic,
            &sys,
            input(TEST_CALL_A, 1, Some("последние слова чанка 0")),
        )
        .await
        .unwrap();
        let mic_opts = mic.last_opts.lock().unwrap().clone().unwrap();
        let sys_opts = sys.last_opts.lock().unwrap().clone().unwrap();
        assert_eq!(mic_opts.prompt.as_deref(), Some("последние слова чанка 0"));
        // System не должен получать mic prev_prompt (другой speaker).
        assert!(sys_opts.prompt.is_none());
    }

    #[tokio::test]
    async fn mic_error_marks_failed_and_propagates_err() {
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, TEST_CALL_A, 0).await;
        let mic = MockProvider::err(TranscriptionError::Provider("simulated mic fail".into()));
        let sys = MockProvider::ok(fake_transcript(vec!["something"]));
        let err = run_chunk(&db_t.pool, &mic, &sys, input(TEST_CALL_A, 0, None))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("simulated mic fail"));
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, TEST_CALL_A)
            .await
            .unwrap();
        assert_eq!(rows[0].status, "failed");
    }

    #[tokio::test]
    async fn system_error_degraded_ok_mic_persisted_sys_none() {
        // Mic ok, system fails → chunk done с system_transcript_json=NULL.
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, TEST_CALL_A, 0).await;
        let mic = MockProvider::ok(fake_transcript(vec!["mic content"]));
        let sys = MockProvider::err(TranscriptionError::Provider("sys boom".into()));
        let out = run_chunk(&db_t.pool, &mic, &sys, input(TEST_CALL_A, 0, None))
            .await
            .unwrap();
        // segment_count учитывает только mic когда sys = None.
        assert_eq!(out.segment_count, 1);
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, TEST_CALL_A)
            .await
            .unwrap();
        assert_eq!(rows[0].status, "done");
        assert!(rows[0].transcript_json.is_some());
        assert!(rows[0].system_transcript_json.is_none());
    }

    #[tokio::test]
    async fn both_tracks_fail_marks_failed() {
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, TEST_CALL_A, 0).await;
        let mic = MockProvider::err(TranscriptionError::Provider("mic dead".into()));
        let sys = MockProvider::err(TranscriptionError::Provider("sys dead".into()));
        let err = run_chunk(&db_t.pool, &mic, &sys, input(TEST_CALL_A, 0, None))
            .await
            .unwrap_err();
        // Mic fail доминирует — это критичная ошибка.
        assert!(format!("{err}").contains("mic dead"));
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, TEST_CALL_A)
            .await
            .unwrap();
        assert_eq!(rows[0].status, "failed");
    }

    #[tokio::test]
    async fn tail_extraction_truncates_to_50_words() {
        // Construct transcript с 60 словами — tail должен быть последние 50.
        let big_text = (0..60)
            .map(|i| format!("слово{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let transcript = DiarizedTranscript {
            version: 1,
            lang_detected: Some("ru".into()),
            duration_sec: 10.0,
            provider: "mock".into(),
            segments: vec![TranscriptSegment {
                start: 0.0,
                end: 10.0,
                text: big_text,
                speaker_tag: "speaker:0".into(),
                confidence: None,
            }],
        };
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, TEST_CALL_A, 0).await;
        let provider = MockProvider::ok(transcript);
        let out = run_chunk(
            &db_t.pool,
            &provider,
            &provider,
            input(TEST_CALL_A, 0, None),
        )
        .await
        .unwrap();
        let word_count = out.transcript_tail.split_whitespace().count();
        assert_eq!(word_count, 50);
        // Первое слово tail'а должно быть слово10 (60−50).
        assert!(out.transcript_tail.starts_with("слово10 "));
    }

    #[tokio::test]
    async fn fails_when_chunk_not_in_pending_status() {
        // Если row уже processed (либо просто marker'а нет — row отсутствует),
        // mark_chunk_processing fail'ится — run_chunk пропагирует.
        let db_t = fresh_db().await;
        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES ('c1', CURRENT_TIMESTAMP, 'recording', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&db_t.pool)
        .await
        .unwrap();
        // НЕ создаём chunk row.
        let provider = MockProvider::ok(fake_transcript(vec!["test"]));
        let err = run_chunk(
            &db_t.pool,
            &provider,
            &provider,
            input(TEST_CALL_A, 0, None),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("not in 'pending'"));
        // Provider НЕ должен был быть вызван если FSM gate fail'нулся first.
        assert!(!provider.called.load(Ordering::Relaxed));
    }

    /// [M13 fix] End-to-end seam что должны были ловить M13.1.6/M13.2.4 (real
    /// WAV smoke, отложены). Прогоняет rotated chunks 0,1 + финальный 2 через
    /// **реальный** путь enqueue→run_chunk→assembly→merge со stub-провайдером +
    /// tiny hound-WAV фикстурами. Ловит: run_chunk читает chunks/{idx}/ (не
    /// root), assembly оффсетит все чанки, merge включает ВСЕ (включая chunk 0
    /// и финальный) → root WAV полной длины.
    #[tokio::test]
    async fn e2e_chunks_to_assembly_and_merge_full_length() {
        use crate::call_store::CallStore;
        use crate::pipeline::{audio_merger, chunk_assembly};
        use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

        let db_t = fresh_db().await;
        let dir = tempfile::tempdir().unwrap();
        let store = CallStore::new(dir.path().to_path_buf());

        sqlx::query(
            "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
             VALUES ('aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa', CURRENT_TIMESTAMP, 'processing', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&db_t.pool)
        .await
        .unwrap();

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let write_wav = |path: &Path, n: i16| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut w = WavWriter::create(path, spec).unwrap();
            for i in 0..n {
                w.write_sample(i).unwrap();
            }
            w.finalize().unwrap();
        };

        // chunks 0,1 (rotated) + 2 (финальный), разные длины сэмплов.
        let counts: [i16; 3] = [3, 2, 4];
        for (idx, &n) in counts.iter().enumerate() {
            let idx = idx as u32;
            write_wav(&store.chunk_mic_path(&cid(TEST_CALL_A), idx), n);
            write_wav(&store.chunk_system_path(&cid(TEST_CALL_A), idx), n);
            db::chunks::insert_chunk(
                &db_t.pool,
                TEST_CALL_A,
                idx,
                u64::from(idx) * 600_000,
                &store.chunk_mic_path(&cid(TEST_CALL_A), idx),
                &store.chunk_system_path(&cid(TEST_CALL_A), idx),
            )
            .await
            .unwrap();
            let mic = MockProvider::ok(fake_transcript(vec!["a"]));
            let sys = MockProvider::ok(fake_transcript(vec!["b"]));
            let inp = ChunkRunInput {
                call_id: TEST_CALL_A.into(),
                chunk_idx: idx,
                start_ms: u64::from(idx) * 600_000,
                end_ms: (u64::from(idx) + 1) * 600_000,
                mic_path: store.chunk_mic_path(&cid(TEST_CALL_A), idx),
                system_path: store.chunk_system_path(&cid(TEST_CALL_A), idx),
                prev_prompt: None,
                lang: "auto".into(),
                app_data_dir: None,
                app_handle: None,
                mic_diarization: false,
                mic_diarization_num_speakers: None,
            };
            run_chunk(&db_t.pool, &mic, &sys, inp).await.unwrap();
        }

        // Assembly: 3 chunk'а с offset'ами.
        let (mic_t, _sys_t) =
            chunk_assembly::load_chunked_transcripts(&db_t.pool, TEST_CALL_A, None)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            mic_t.segments.len(),
            3,
            "все 3 chunk'а собраны (не обрезано)"
        );
        // duration = max chunk end_ms = 1_800_000 → 1800s.
        assert!((mic_t.duration_sec - 1800.0).abs() < 1e-9);

        // Merge: root mic samples = сумма всех 3 chunk'ов (3+2+4=9), в порядке.
        let (mic_r, _) = audio_merger::merge_both_tracks(
            &store.chunks_dir(&cid(TEST_CALL_A)),
            &store.call_dir(&cid(TEST_CALL_A)),
        );
        let report = mic_r.expect("mic merge ok");
        assert_eq!(report.total_samples, 9, "merge включает ВСЕ chunk'и");
        let merged: Vec<i16> = WavReader::open(store.call_dir(&cid(TEST_CALL_A)).join("mic.wav"))
            .unwrap()
            .into_samples::<i16>()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(merged, vec![0, 1, 2, 0, 1, 0, 1, 2, 3]);
    }
}
