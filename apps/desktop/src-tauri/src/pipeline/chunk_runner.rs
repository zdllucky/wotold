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

use crate::embeddings::{self, StubEmbedder};
use crate::events::{ChunkDoneEvent, EventBus};
use crate::pipeline::clusters::extract_clusters;
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

    // 2. Параллельный mic+system STT. Prompt-priming идёт только в mic —
    //    system track имеет другой speaker (собеседник), prev_prompt от
    //    owner-mic дал бы ложный bias.
    let mic_opts = TranscriptionOpts {
        lang: lang.clone(),
        diarization: true,
        prompt: prev_prompt,
    };
    let sys_opts = TranscriptionOpts {
        lang,
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
        ) {
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

/// [M13.2.1] Собрать `HashMap<speaker_tag, Vec<f32>>` (JSON-сериализованный)
/// из mic+system transcript'ов одного chunk'а. Embedder резолвится через
/// `try_load_onnx_embedder` (= StubEmbedder если voice-onnx feature off /
/// модель не скачана → empty embeddings, identity remap в assembly).
fn build_chunk_embeddings_json(
    mic_transcript: &DiarizedTranscript,
    sys_transcript: Option<&DiarizedTranscript>,
    mic_path: &std::path::Path,
    system_path: &std::path::Path,
    app_data_dir: &std::path::Path,
) -> Result<String, AppError> {
    let model_path = app_data_dir.join("models").join("embedder.onnx");
    let embedder: Box<dyn embeddings::Embedder> =
        match embeddings::try_load_onnx_embedder(&model_path) {
            Some(e) => e,
            None => Box::new(StubEmbedder),
        };

    // Merged = все сегменты обоих дорожек. extract_clusters сам routes
    // owner-tagged → mic.wav, прочие → system.wav.
    let mut merged: Vec<TranscriptSegment> = Vec::with_capacity(
        mic_transcript.segments.len() + sys_transcript.map(|s| s.segments.len()).unwrap_or(0),
    );
    merged.extend(mic_transcript.segments.iter().cloned());
    if let Some(sys) = sys_transcript {
        merged.extend(sys.segments.iter().cloned());
    }
    let clusters = extract_clusters(&merged, mic_path, system_path, embedder.as_ref())?;
    serde_json::to_string(&clusters)
        .map_err(|e| AppError::Other(format!("serialize chunk embeddings: {e}")))
}

/// Извлечь последние `max_words` слов из transcript'а. Для prompt priming
/// whisper-cli — точность первой фразы chunk N+1 вырастает с 80% до 95%.
fn extract_tail_words(transcript: &DiarizedTranscript, max_words: usize) -> String {
    // Concat сегменты в одну строку слов, отбрасываем speaker tags. Просто
    // последние N "пробело-разделённых" токенов — для whisper.cpp prompt
    // нужен plain text без diarization markup.
    let all_text: String = transcript
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let words: Vec<&str> = all_text.split_whitespace().collect();
    if words.len() <= max_words {
        all_text.trim().to_string()
    } else {
        words[words.len() - max_words..].join(" ")
    }
}

fn translate_transcription_error(e: TranscriptionError) -> AppError {
    AppError::Other(format!("chunk transcription failed: {e}"))
}

#[cfg(test)]
mod tests {
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
        setup_chunk(&db_t.pool, "c1", 0).await;
        let mic = MockProvider::ok(fake_transcript(vec!["Привет.", "Как дела?"]));
        let sys = MockProvider::ok(fake_transcript(vec!["Здравствуйте."]));
        let out = run_chunk(&db_t.pool, &mic, &sys, input("c1", 0, None))
            .await
            .unwrap();
        assert!(out.transcript_tail.contains("Как дела"));
        // mic = 2 segments + sys = 1 segment.
        assert_eq!(out.segment_count, 3);
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, "c1")
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
        setup_chunk(&db_t.pool, "c1", 1).await;
        let mic = MockProvider::ok(fake_transcript(vec!["Дальше."]));
        let sys = MockProvider::ok(fake_transcript(vec!["Ответ."]));
        let _ = run_chunk(
            &db_t.pool,
            &mic,
            &sys,
            input("c1", 1, Some("последние слова чанка 0")),
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
        setup_chunk(&db_t.pool, "c1", 0).await;
        let mic = MockProvider::err(TranscriptionError::Provider("simulated mic fail".into()));
        let sys = MockProvider::ok(fake_transcript(vec!["something"]));
        let err = run_chunk(&db_t.pool, &mic, &sys, input("c1", 0, None))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("simulated mic fail"));
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, "c1")
            .await
            .unwrap();
        assert_eq!(rows[0].status, "failed");
    }

    #[tokio::test]
    async fn system_error_degraded_ok_mic_persisted_sys_none() {
        // Mic ok, system fails → chunk done с system_transcript_json=NULL.
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, "c1", 0).await;
        let mic = MockProvider::ok(fake_transcript(vec!["mic content"]));
        let sys = MockProvider::err(TranscriptionError::Provider("sys boom".into()));
        let out = run_chunk(&db_t.pool, &mic, &sys, input("c1", 0, None))
            .await
            .unwrap();
        // segment_count учитывает только mic когда sys = None.
        assert_eq!(out.segment_count, 1);
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, "c1")
            .await
            .unwrap();
        assert_eq!(rows[0].status, "done");
        assert!(rows[0].transcript_json.is_some());
        assert!(rows[0].system_transcript_json.is_none());
    }

    #[tokio::test]
    async fn both_tracks_fail_marks_failed() {
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, "c1", 0).await;
        let mic = MockProvider::err(TranscriptionError::Provider("mic dead".into()));
        let sys = MockProvider::err(TranscriptionError::Provider("sys dead".into()));
        let err = run_chunk(&db_t.pool, &mic, &sys, input("c1", 0, None))
            .await
            .unwrap_err();
        // Mic fail доминирует — это критичная ошибка.
        assert!(format!("{err}").contains("mic dead"));
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, "c1")
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
        setup_chunk(&db_t.pool, "c1", 0).await;
        let provider = MockProvider::ok(transcript);
        let out = run_chunk(&db_t.pool, &provider, &provider, input("c1", 0, None))
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
        let err = run_chunk(&db_t.pool, &provider, &provider, input("c1", 0, None))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("not in 'pending'"));
        // Provider НЕ должен был быть вызван если FSM gate fail'нулся first.
        assert!(!provider.called.load(Ordering::Relaxed));
    }
}
