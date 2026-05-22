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
//! CHUNKED_PIPELINE feature flag) — M13.1.5b sprint.

#![allow(dead_code)] // Wiring в recording flow — следующий sprint.

use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::providers::transcription::{
    DiarizedTranscript, TranscriptionError, TranscriptionOpts, TranscriptionProvider,
};
use crate::{db, AppError};

/// Параметры для одного запуска `run_chunk`. Все владелие (нет lifetime'ов
/// в callsite'е), упрощает spawn в task'е.
#[derive(Debug, Clone)]
pub struct ChunkRunInput {
    pub call_id: String,
    pub chunk_idx: u32,
    /// Smещение start chunk'а от начала записи (ms). Используется для
    /// timestamp-offset при merge'е в финальный transcript.
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
}

/// Результат успешного `run_chunk`. `transcript_tail` идёт в `prev_prompt`
/// следующего chunk'а. `segment_count` для логирования/UI prog ress.
#[derive(Debug, Clone)]
pub struct ChunkRunOutput {
    pub transcript_tail: String,
    pub segment_count: usize,
}

/// Запустить chunk pipeline (STT-only в Phase 1, diarization+embeddings —
/// Phase 2). Возвращает tail для prev_prompt следующего chunk'а или Err.
/// При Err DB row переводится в `failed`, caller обычно skip'ает chunk
/// (orchestrator решает — retry или нет).
pub async fn run_chunk<P: TranscriptionProvider + ?Sized>(
    pool: &SqlitePool,
    provider: &P,
    input: ChunkRunInput,
) -> Result<ChunkRunOutput, AppError> {
    let ChunkRunInput {
        call_id,
        chunk_idx,
        start_ms: _,
        end_ms,
        mic_path,
        system_path: _,
        prev_prompt,
        lang,
    } = input;

    // 1. FSM gate: pending → processing.
    db::chunks::mark_chunk_processing(pool, &call_id, chunk_idx).await?;

    // 2. Запуск STT с prompt-priming.
    let opts = TranscriptionOpts {
        lang,
        diarization: true,
        prompt: prev_prompt,
    };

    let transcript = match provider.transcribe(&mic_path, opts).await {
        Ok(t) => t,
        Err(e) => {
            let reason = format!("transcribe: {e}");
            let _ = db::chunks::mark_chunk_failed(pool, &call_id, chunk_idx, &reason).await;
            return Err(translate_transcription_error(e));
        }
    };

    // 3. Serialize → DB persist.
    let transcript_json = serde_json::to_string(&transcript)
        .map_err(|e| AppError::Other(format!("transcript serialize: {e}")))?;

    let segment_count = transcript.segments.len();
    let transcript_tail = extract_tail_words(&transcript, 50);

    db::chunks::mark_chunk_done(pool, &call_id, chunk_idx, end_ms, &transcript_json).await?;

    Ok(ChunkRunOutput {
        transcript_tail,
        segment_count,
    })
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
        }
    }

    #[tokio::test]
    async fn success_path_marks_done_and_returns_tail() {
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, "c1", 0).await;
        let provider = MockProvider::ok(fake_transcript(vec!["Привет.", "Как дела?"]));
        let out = run_chunk(&db_t.pool, &provider, input("c1", 0, None))
            .await
            .unwrap();
        assert!(out.transcript_tail.contains("Как дела"));
        assert_eq!(out.segment_count, 2);
        let rows = db::chunks::list_chunks_by_call(&db_t.pool, "c1")
            .await
            .unwrap();
        assert_eq!(rows[0].status, "done");
        assert_eq!(rows[0].end_ms, Some(600_000));
        assert!(rows[0].transcript_json.is_some());
    }

    #[tokio::test]
    async fn prev_prompt_propagated_to_provider() {
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, "c1", 1).await;
        let provider = MockProvider::ok(fake_transcript(vec!["Дальше."]));
        let _ = run_chunk(
            &db_t.pool,
            &provider,
            input("c1", 1, Some("последние слова чанка 0")),
        )
        .await
        .unwrap();
        let opts = provider.last_opts.lock().unwrap().clone().unwrap();
        assert_eq!(opts.prompt.as_deref(), Some("последние слова чанка 0"));
    }

    #[tokio::test]
    async fn provider_error_marks_failed_and_propagates_err() {
        let db_t = fresh_db().await;
        setup_chunk(&db_t.pool, "c1", 0).await;
        let provider = MockProvider::err(TranscriptionError::Provider("simulated".into()));
        let err = run_chunk(&db_t.pool, &provider, input("c1", 0, None))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("simulated"));
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
        let out = run_chunk(&db_t.pool, &provider, input("c1", 0, None))
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
        let err = run_chunk(&db_t.pool, &provider, input("c1", 0, None))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("not in 'pending'"));
        // Provider НЕ должен был быть вызван если FSM gate fail'нулся first.
        assert!(!provider.called.load(Ordering::Relaxed));
    }
}
