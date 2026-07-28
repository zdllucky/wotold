//! Pipeline orchestrator для speaker identification (#25 / M3.4).
//!
//! # Статус: не подключён (сверка 2026-07)
//!
//! Живой путь привязки спикеров — `pipeline::run_cluster_pipeline`: он берёт
//! кластеры из уже посчитанных эмбеддингов, ранжирует по консентящим
//! `voice_samples` и пишет `set_call_speaker_suggestion` с
//! `source = "embedding"`. Эмбеддинг-канал этого модуля им вытеснен целиком и
//! повторяет дорогую часть (extract_segment + ONNX на каждого спикера).
//!
//! Не вытеснен только **LLM-канал** (`llm_hint` + `merge_signals`): подсказка
//! по обращениям в транскрипте, R2 паспорта — booster без автопривязки.
//! Сейчас источники `"llm"`/`"both"` в БД не появляются никогда. Подключение —
//! отдельный майлстоун, а не сверка: это лишний проход локальной LLM по всему
//! транскрипту на каждую запись, то есть заметная плата по времени, и решение
//! о ней принимает владелец. См. ROADMAP §«Диаризация и матчинг».
//!
//! Модуль оставлен как готовая реализация этого канала. Менять его смысла нет:
//! при wire-up эмбеддинг-часть заменяется на кластеры из живого пути.
//!
//! Шаги:
//! 1. Группируем сегменты по `speaker_tag` (skip OWNER_TAG — M3.7 авто-bind).
//! 2. Для каждой группы — concat audio из system.wav, compute embedding.
//! 3. Параллельно: matching против consenting `voice_samples` + LLM hint.
//! 4. Merge сигналов → `MergedSuggestion[]`.
//! 5. `db::insert_speaker_suggestions(call_id, ...)`.

use std::collections::HashMap;
use std::path::Path;

use sqlx::SqlitePool;

use crate::{
    audio_io, db,
    embeddings::Embedder,
    llm_hint::{self, LlmHintContact},
    matching, merge_signals,
    pipeline::merge::OWNER_TAG,
    providers::{llm::LlmProvider, transcription::DiarizedTranscript},
    AppError,
};

const MIN_MATCH_SCORE: f32 = 0.5;
const TOP_N: usize = 3;

pub struct IdentifyCtx<'a> {
    pub call_id: &'a str,
    pub system_path: &'a Path,
    pub system_transcript: &'a DiarizedTranscript,
    /// Полный merged-транскрипт в Markdown — даём LLM для context-based hint.
    pub transcript_md: &'a str,
    pub embedder: &'a dyn Embedder,
    /// Optional LLM provider. None → embedding-only matching.
    pub llm: Option<&'a dyn LlmProvider>,
    pub llm_model: Option<&'a str>,
}

pub async fn identify_speakers(pool: &SqlitePool, ctx: IdentifyCtx<'_>) -> Result<(), AppError> {
    // 1. Группируем segments по speaker_tag (skip owner).
    let mut by_speaker: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
    for seg in &ctx.system_transcript.segments {
        if seg.speaker_tag == OWNER_TAG {
            continue;
        }
        by_speaker
            .entry(seg.speaker_tag.clone())
            .or_default()
            .push((seg.start, seg.end));
    }
    if by_speaker.is_empty() {
        log::info!("identify_speakers {}: nothing to identify", ctx.call_id);
        return Ok(());
    }

    // 2. Embedding per speaker.
    let mut embedding_by_speaker: HashMap<String, Vec<f32>> = HashMap::new();
    for (tag, spans) in &by_speaker {
        let mut samples: Vec<f32> = Vec::new();
        let mut sr = 16_000_u32;
        for &(start, end) in spans {
            match audio_io::extract_segment(ctx.system_path, start, end) {
                Ok(clip) => {
                    sr = clip.sample_rate;
                    samples.extend(clip.samples);
                }
                Err(e) => {
                    log::warn!("extract_segment {tag} {start}..{end}: {e}");
                }
            }
        }
        if samples.is_empty() {
            continue;
        }
        match ctx.embedder.extract(&samples, sr) {
            Ok(emb) => {
                embedding_by_speaker.insert(tag.clone(), emb);
            }
            Err(e) => log::warn!("embedder.extract {tag}: {e}"),
        }
    }

    // 3a. Cosine match per speaker.
    let consenting = matching::list_consenting_samples(pool).await?;
    let mut emb_candidates: HashMap<String, Vec<matching::MatchCandidate>> = HashMap::new();
    for (tag, emb) in &embedding_by_speaker {
        let ranked = matching::rank_candidates(emb, &consenting, MIN_MATCH_SCORE, TOP_N);
        if !ranked.is_empty() {
            emb_candidates.insert(tag.clone(), ranked);
        }
    }

    // 3b. LLM hint (best-effort). Берём всех неowner-контактов как кандидатов.
    let mut llm_hints: HashMap<String, matching::MatchCandidate> = HashMap::new();
    if let (Some(provider), Some(model)) = (ctx.llm, ctx.llm_model) {
        let hint_contacts = llm_hint_contacts(pool).await?;
        if !hint_contacts.is_empty() {
            llm_hints =
                llm_hint::request_speaker_hints(provider, ctx.transcript_md, &hint_contacts, model)
                    .await
                    .unwrap_or_default();
        }
    }

    // 4. Merge.
    let merged = merge_signals::merge(emb_candidates, llm_hints);

    // 5. Persist.
    db::insert_speaker_suggestions(pool, ctx.call_id, &merged).await?;
    log::info!(
        "identify_speakers {}: {} suggestion(s) written",
        ctx.call_id,
        merged.len()
    );
    Ok(())
}

/// Не-owner контакты с display_name + role/org для prompt'а LLM.
async fn llm_hint_contacts(pool: &SqlitePool) -> Result<Vec<LlmHintContact>, AppError> {
    use sqlx::Row;
    let rows = sqlx::query("SELECT id, display_name, role, org FROM contacts WHERE is_owner = 0")
        .fetch_all(pool)
        .await?;
    let out = rows
        .into_iter()
        .map(|r| {
            Ok::<_, AppError>(LlmHintContact {
                id: r.try_get::<String, _>("id")?,
                display_name: r.try_get::<String, _>("display_name")?,
                role: r.try_get::<Option<String>, _>("role")?,
                org: r.try_get::<Option<String>, _>("org")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::fresh_db;
    use crate::embeddings::Embedder;
    use crate::providers::transcription::{DiarizedTranscript, TranscriptSegment};

    /// Detрерминированный embedder для теста: возвращает заранее заданные
    /// вектора по индексу вызова. Не зависит от samples — нужен только для
    /// проверки оркестрации.
    struct FixedEmbedder {
        outputs: std::sync::Mutex<Vec<Vec<f32>>>,
    }

    impl Embedder for FixedEmbedder {
        fn extract(&self, _samples: &[f32], _sr: u32) -> Result<Vec<f32>, AppError> {
            let mut q = self.outputs.lock().unwrap();
            if q.is_empty() {
                return Err(AppError::Other("no more fixed outputs".into()));
            }
            Ok(q.remove(0))
        }
    }

    fn write_silent_wav(path: &std::path::Path, sec: f64) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sec * 16_000.0) as usize;
        for _ in 0..n {
            w.write_sample(0_i16).unwrap();
        }
        w.finalize().unwrap();
    }

    fn ts(start: f64, end: f64, speaker: &str) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: "hi".into(),
            speaker_tag: speaker.into(),
            confidence: None,
        }
    }

    #[tokio::test]
    async fn identifies_speakers_via_embedding_only() {
        let db = fresh_db().await;
        // Контакт с consent + voice_sample
        let owner = crate::db::ensure_owner_contact(&db.pool).await.unwrap();
        let _ = owner;
        sqlx::query(
            "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?4, ?4)",
        )
        .bind("c1")
        .bind("Alice")
        .bind(r#"{"consent_voice":"true"}"#)
        .bind("2026-05-20T00:00:00Z")
        .execute(&db.pool)
        .await
        .unwrap();
        let emb = vec![1.0_f32, 0.0, 0.0];
        let blob = crate::embeddings::embedding_to_bytes(&emb);
        sqlx::query(
            "INSERT INTO voice_samples (id, contact_id, embedding, quality, created_at)
             VALUES ('vs-1', 'c1', ?1, 0.9, '2026-05-20T00:00:00Z')",
        )
        .bind(blob)
        .execute(&db.pool)
        .await
        .unwrap();

        // calls row
        let call = crate::db::insert_recording(&db.pool, "managed")
            .await
            .unwrap();

        // system.wav 1 sec silence (доста для extract_segment)
        let dir = tempfile::tempdir().unwrap();
        let system_path = dir.path().join("system.wav");
        write_silent_wav(&system_path, 1.0);

        let sys = DiarizedTranscript {
            version: 1,
            lang_detected: None,
            duration_sec: 1.0,
            provider: "test".into(),
            segments: vec![ts(0.0, 0.5, "Speaker 0"), ts(0.5, 1.0, "owner")],
        };

        let embedder = FixedEmbedder {
            outputs: std::sync::Mutex::new(vec![vec![0.95_f32, 0.05, 0.0]]),
        };

        let ctx = IdentifyCtx {
            call_id: &call.id,
            system_path: &system_path,
            system_transcript: &sys,
            transcript_md: "",
            embedder: &embedder,
            llm: None,
            llm_model: None,
        };

        identify_speakers(&db.pool, ctx).await.unwrap();

        // Проверяем что call_speakers есть и привязан к c1.
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT speaker_tag, suggestion_contact_id, suggestion_source
             FROM call_speakers WHERE call_id = ?1",
        )
        .bind(&call.id)
        .fetch_all(&db.pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        let tag: String = rows[0].try_get("speaker_tag").unwrap();
        let cid: Option<String> = rows[0].try_get("suggestion_contact_id").unwrap();
        let src: String = rows[0].try_get("suggestion_source").unwrap();
        assert_eq!(tag, "Speaker 0");
        assert_eq!(cid.as_deref(), Some("c1"));
        assert_eq!(src, "embedding");
    }

    #[tokio::test]
    async fn skip_owner_segments_and_no_contacts_yields_no_rows() {
        let db = fresh_db().await;
        let call = crate::db::insert_recording(&db.pool, "managed")
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let system_path = dir.path().join("system.wav");
        write_silent_wav(&system_path, 0.5);

        let sys = DiarizedTranscript {
            version: 1,
            lang_detected: None,
            duration_sec: 0.5,
            provider: "test".into(),
            segments: vec![ts(0.0, 0.5, "owner")],
        };

        let embedder = FixedEmbedder {
            outputs: std::sync::Mutex::new(vec![]),
        };
        let ctx = IdentifyCtx {
            call_id: &call.id,
            system_path: &system_path,
            system_transcript: &sys,
            transcript_md: "",
            embedder: &embedder,
            llm: None,
            llm_model: None,
        };
        identify_speakers(&db.pool, ctx).await.unwrap();

        use sqlx::Row;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM call_speakers WHERE call_id = ?1")
                .bind(&call.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let _ = count;
        // Не падаем; rows нет — owner skipped, embedder не вызывался, suggestions пусты.
        let r = sqlx::query("SELECT count(*) as n FROM call_speakers WHERE call_id = ?1")
            .bind(&call.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let n: i64 = r.try_get("n").unwrap();
        assert_eq!(n, 0);
    }
}
