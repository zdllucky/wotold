//! [TD-33] Тесты клея пайплайна: `run`, `reprocess_call`, `regenerate_recap`
//! и их guard rails.
//!
//! Живут отдельным файлом, а не `#[cfg(test)] mod tests` внутри `mod.rs`:
//! тот и без тестов 1911 строк — сверх лимита 800 (правило 8), и гейт
//! справедливо не давал дописать туда ни строки. Дочерний модуль в
//! отдельном файле видит приватные элементы родителя ровно так же.

use super::*;
use crate::db::test_support::fresh_db;

// ============================================================
// [TD-12] finish_event — событие пайплайна эмитится всегда
// ============================================================

#[test]
fn finish_event_ready_when_no_errors() {
    let e = finish_event("call-1", None, None);
    assert_eq!(e.status, "ready");
    assert!(e.failed_reason.is_none());
    assert_eq!(e.call_id, "call-1");
}

#[test]
fn finish_event_failed_on_pipeline_error() {
    let e = finish_event("call-1", Some("stt timeout".into()), None);
    assert_eq!(e.status, "failed");
    assert_eq!(e.failed_reason.as_deref(), Some("stt timeout"));
}

#[test]
fn finish_event_failed_when_mark_ready_fails_after_success() {
    // Регрессия TD-12: пайплайн успешен, но статус ready не записался
    // (busy pool / disk full). Раньше `?` выходил из run() до эмита
    // события — звонок навсегда висел в `processing`. Теперь исход failed,
    // и событие всё равно эмитится.
    let e = finish_event("call-1", None, Some("database is locked".into()));
    assert_eq!(e.status, "failed");
    assert_eq!(e.failed_reason.as_deref(), Some("database is locked"));
}

#[test]
fn finish_event_pipeline_error_wins_over_mark_ready() {
    // Defensive: mark_ready пробуется только на success, но если оба Some —
    // причина пайплайна информативнее.
    let e = finish_event(
        "call-1",
        Some("pipeline boom".into()),
        Some("mark boom".into()),
    );
    assert_eq!(e.status, "failed");
    assert_eq!(e.failed_reason.as_deref(), Some("pipeline boom"));
}

// ============================================================
// [P13] ensure_all_chunks_done — halt gate перед stage 2→3
// ============================================================

async fn insert_call_row(pool: &sqlx::SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO calls (id, started_at, status, path_label, created_at, updated_at)
         VALUES (?1, CURRENT_TIMESTAMP, 'recording', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

// ============================================================
// [P-fix4] call_language — пин языка звонка на оба трека (auto)
// ============================================================

fn lang_track(lang: Option<&str>, segs: Vec<&str>) -> DiarizedTranscript {
    DiarizedTranscript {
        version: 1,
        lang_detected: lang.map(String::from),
        duration_sec: 10.0,
        provider: "local-whisper".into(),
        segments: segs
            .into_iter()
            .enumerate()
            .map(
                |(i, t)| crate::providers::transcription::TranscriptSegment {
                    start: i as f64,
                    end: i as f64 + 1.0,
                    text: t.into(),
                    speaker_tag: "speaker:0".into(),
                    confidence: None,
                },
            )
            .collect(),
    }
}

#[test]
fn call_language_prefers_system_anchor() {
    // mic mis-detect «en» только из [FOREIGN] (0 реальных слов),
    // system — русский с речью → язык звонка = ru.
    let mic = lang_track(Some("en"), vec!["[FOREIGN]", "[FOREIGN]"]);
    let sys = lang_track(
        Some("ru"),
        vec!["добрый день коллеги мы начинаем обсуждение по проекту сегодня"],
    );
    assert_eq!(call_language(&mic, &sys).as_deref(), Some("ru"));
}

#[test]
fn call_language_falls_back_to_mic_when_system_empty() {
    // system пустой → якорь mic.
    let mic = lang_track(
        Some("ru"),
        vec!["это длинная реплика владельца на много слов подряд вот так вот"],
    );
    let sys = lang_track(None, vec![]);
    assert_eq!(call_language(&mic, &sys).as_deref(), Some("ru"));
}

#[test]
fn call_language_none_when_both_below_threshold() {
    let mic = lang_track(Some("en"), vec!["да"]);
    let sys = lang_track(Some("ru"), vec!["угу"]);
    assert_eq!(call_language(&mic, &sys), None);
}

#[tokio::test]
async fn ensure_all_chunks_done_returns_ok_when_no_chunks() {
    // Non-chunked path (cloud / single-pass) — нет rows в call_chunks.
    // Halt не релевантен — pipeline fall back на full-file STT.
    let test_db = fresh_db().await;
    insert_call_row(&test_db.pool, "c1").await;
    assert!(ensure_all_chunks_done(&test_db.pool, "c1").await.is_ok());
}

async fn insert_and_mark_done(pool: &sqlx::SqlitePool, call_id: &str, idx: u32) {
    use std::path::PathBuf;
    db::chunks::insert_chunk(
        pool,
        call_id,
        idx,
        u64::from(idx) * 600_000,
        &PathBuf::from(format!("/m{idx}")),
        &PathBuf::from(format!("/s{idx}")),
    )
    .await
    .unwrap();
    db::chunks::mark_chunk_processing(pool, call_id, idx)
        .await
        .unwrap();
    db::chunks::mark_chunk_done(
        pool,
        call_id,
        idx,
        u64::from(idx + 1) * 600_000,
        r#"{"segments":[]}"#,
        None,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ensure_all_chunks_done_returns_ok_when_all_done() {
    let test_db = fresh_db().await;
    insert_call_row(&test_db.pool, "c1").await;
    for idx in 0..3 {
        insert_and_mark_done(&test_db.pool, "c1", idx).await;
    }
    assert!(ensure_all_chunks_done(&test_db.pool, "c1").await.is_ok());
}

#[tokio::test]
async fn ensure_all_chunks_done_returns_err_on_failed_chunk() {
    use std::path::PathBuf;
    let test_db = fresh_db().await;
    insert_call_row(&test_db.pool, "c1").await;
    // 2 done, 1 failed (user's reported scenario: 1/3).
    for idx in 0..2 {
        insert_and_mark_done(&test_db.pool, "c1", idx).await;
    }
    db::chunks::insert_chunk(
        &test_db.pool,
        "c1",
        2,
        1_200_000,
        &PathBuf::from("/m2"),
        &PathBuf::from("/s2"),
    )
    .await
    .unwrap();
    db::chunks::mark_chunk_processing(&test_db.pool, "c1", 2)
        .await
        .unwrap();
    db::chunks::mark_chunk_failed(&test_db.pool, "c1", 2, "STT timeout")
        .await
        .unwrap();
    let err = ensure_all_chunks_done(&test_db.pool, "c1")
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("chunks_need_retry"), "got: {msg}");
    assert!(msg.contains("1 of 3"), "got: {msg}");
    assert!(msg.contains("[2]"), "got: {msg}");
}

// [M13 fix] chunks_ready — relaxed gate variants.
#[tokio::test]
async fn chunks_ready_no_chunks() {
    let test_db = fresh_db().await;
    insert_call_row(&test_db.pool, "c1").await;
    assert_eq!(
        chunks_ready(&test_db.pool, "c1").await.unwrap(),
        ChunkGate::NoChunks
    );
}

#[tokio::test]
async fn chunks_ready_all_done() {
    let test_db = fresh_db().await;
    insert_call_row(&test_db.pool, "c1").await;
    for idx in 0..2 {
        insert_and_mark_done(&test_db.pool, "c1", idx).await;
    }
    assert_eq!(
        chunks_ready(&test_db.pool, "c1").await.unwrap(),
        ChunkGate::AllDone
    );
}

#[tokio::test]
async fn chunks_ready_partial_when_some_failed() {
    use std::path::PathBuf;
    let test_db = fresh_db().await;
    insert_call_row(&test_db.pool, "c1").await;
    insert_and_mark_done(&test_db.pool, "c1", 0).await;
    db::chunks::insert_chunk(
        &test_db.pool,
        "c1",
        1,
        600_000,
        &PathBuf::from("/m1"),
        &PathBuf::from("/s1"),
    )
    .await
    .unwrap();
    db::chunks::mark_chunk_failed(&test_db.pool, "c1", 1, "boom")
        .await
        .unwrap();
    assert_eq!(
        chunks_ready(&test_db.pool, "c1").await.unwrap(),
        ChunkGate::Partial {
            done: 1,
            total: 2,
            failed: vec![1]
        }
    );
}

#[tokio::test]
async fn chunks_ready_none_done_when_all_failed() {
    use std::path::PathBuf;
    let test_db = fresh_db().await;
    insert_call_row(&test_db.pool, "c1").await;
    db::chunks::insert_chunk(
        &test_db.pool,
        "c1",
        0,
        0,
        &PathBuf::from("/m0"),
        &PathBuf::from("/s0"),
    )
    .await
    .unwrap();
    db::chunks::mark_chunk_failed(&test_db.pool, "c1", 0, "boom")
        .await
        .unwrap();
    assert_eq!(
        chunks_ready(&test_db.pool, "c1").await.unwrap(),
        ChunkGate::NoneDone { total: 1 }
    );
}

#[tokio::test]
async fn ensure_all_chunks_done_returns_err_on_pending_chunk() {
    use std::path::PathBuf;
    let test_db = fresh_db().await;
    insert_call_row(&test_db.pool, "c1").await;
    db::chunks::insert_chunk(
        &test_db.pool,
        "c1",
        0,
        0,
        &PathBuf::from("/m0"),
        &PathBuf::from("/s0"),
    )
    .await
    .unwrap();
    // Status pending — не failed, но и не done. Должен halt.
    let err = ensure_all_chunks_done(&test_db.pool, "c1")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("chunks_need_retry"));
}

// ============================================================
// [Phase 2] reprocess_call — guard rails
// ============================================================

#[tokio::test]
async fn reprocess_call_missing_audio_returns_error() {
    let db = fresh_db().await;
    let tmpdir = tempfile::tempdir().unwrap();

    let call = db::insert_recording(&db.pool, "local").await.unwrap();
    // Аудио намеренно не создаём — pipeline должен отвергнуть.
    let err = reprocess_call(&db.pool, tmpdir.path(), &call.id, None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Аудио файлы"),
        "expected audio-missing error, got: {err}"
    );
}

#[tokio::test]
async fn reprocess_call_unknown_call_id_returns_not_found() {
    let db = fresh_db().await;
    let tmpdir = tempfile::tempdir().unwrap();
    let err = reprocess_call(&db.pool, tmpdir.path(), "ghost-id", None)
        .await
        .unwrap_err();
    // [Phase 1 R6] typed NotFound теперь сериализуется как
    // "not found: call ghost-id".
    assert!(
        matches!(err, AppError::NotFound(_)),
        "expected NotFound, got: {err:?}"
    );
}

// ============================================================
// [Phase 3 R2] run_auto_bind — typed config branching
// ============================================================

use crate::pipeline::settings::AutoBindConfig;

fn settings_with_auto_bind(auto_bind: Option<AutoBindConfig>) -> PipelineSettings {
    PipelineSettings {
        stt_lang: "auto".into(),
        preferred_language: "auto".into(),
        auto_bind,
        summary_v2_enabled: true,
        summary_speculative_decoding: false,
    }
}

async fn insert_consenting_contact_with_samples(
    pool: &sqlx::SqlitePool,
    name: &str,
    sample_count: usize,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO contacts (id, display_name, is_owner, attributes, created_at, updated_at)
         VALUES (?1, ?2, 0, '{\"consent_voice\":\"true\"}', ?3, ?3)",
    )
    .bind(&id)
    .bind(name)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();
    for i in 0..sample_count {
        sqlx::query(
            "INSERT INTO voice_samples
               (id, contact_id, embedding, source_call, quality, created_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
        )
        .bind(format!("vs-{name}-{i}"))
        .bind(&id)
        .bind(vec![0u8; 4])
        .bind(0.9)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
    }
    id
}

async fn insert_speaker_with_score(
    pool: &sqlx::SqlitePool,
    call_id: &str,
    tag: &str,
    suggestion_contact_id: &str,
    score: f64,
) {
    sqlx::query(
        "INSERT INTO call_speakers
           (id, call_id, speaker_tag, suggestion_contact_id, suggestion_score, suggestion_source, confirmed)
         VALUES (?1, ?2, ?3, ?4, ?5, 'embedding', 0)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(call_id)
    .bind(tag)
    .bind(suggestion_contact_id)
    .bind(score)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn run_auto_bind_disabled_skips_db_call() {
    // auto_bind=None → ни одного speaker не привязано, даже если есть
    // высокий-score suggestion + consent + samples.
    let db = fresh_db().await;
    let call = db::insert_recording(&db.pool, "managed").await.unwrap();
    let alice = insert_consenting_contact_with_samples(&db.pool, "Alice", 3).await;
    insert_speaker_with_score(&db.pool, &call.id, "S1", &alice, 0.99).await;

    let s = settings_with_auto_bind(None);
    run_auto_bind(&db.pool, None, &call.id, &s).await.unwrap();

    let speakers = db::list_call_speakers(&db.pool, &call.id).await.unwrap();
    let s1 = speakers.iter().find(|s| s.speaker_tag == "S1").unwrap();
    assert!(!s1.confirmed, "disabled auto_bind не должен привязывать");
    assert!(s1.contact_id.is_none());
    assert!(s1.auto_bound_at.is_none());
}

#[tokio::test]
async fn run_auto_bind_enabled_binds_speakers_with_threshold() {
    // Two speakers: 0.97 (>=0.95) → auto-bound; 0.90 (<0.95) → не привязан.
    let db = fresh_db().await;
    let call = db::insert_recording(&db.pool, "managed").await.unwrap();
    let alice = insert_consenting_contact_with_samples(&db.pool, "Alice", 2).await;
    let bob = insert_consenting_contact_with_samples(&db.pool, "Bob", 2).await;
    insert_speaker_with_score(&db.pool, &call.id, "S1", &alice, 0.97).await;
    insert_speaker_with_score(&db.pool, &call.id, "S2", &bob, 0.90).await;

    let s = settings_with_auto_bind(Some(AutoBindConfig { threshold: 0.95 }));
    run_auto_bind(&db.pool, None, &call.id, &s).await.unwrap();

    let speakers = db::list_call_speakers(&db.pool, &call.id).await.unwrap();
    let s1 = speakers.iter().find(|s| s.speaker_tag == "S1").unwrap();
    let s2 = speakers.iter().find(|s| s.speaker_tag == "S2").unwrap();
    assert!(s1.confirmed, "S1 score 0.97 >= 0.95 → auto-bound");
    assert_eq!(s1.contact_id.as_deref(), Some(alice.as_str()));
    assert!(s1.auto_bound_at.is_some());
    assert!(!s2.confirmed, "S2 score 0.90 < 0.95 → не привязан");
    assert!(s2.contact_id.is_none());
    assert!(s2.auto_bound_at.is_none());
}

#[tokio::test]
async fn reprocess_call_resets_status_and_progress_when_audio_exists() {
    let db = fresh_db().await;
    let tmpdir = tempfile::tempdir().unwrap();

    // Подготовка: row в failed с прогрессом, аудио на диске.
    let call = db::insert_recording(&db.pool, "local").await.unwrap();
    db::fail_recording_with_reason(&db.pool, &call.id, Some("стрый fail"))
        .await
        .unwrap();
    db::set_call_progress(&db.pool, &call.id, 3, 50, Some(10), Some(2048))
        .await
        .unwrap();

    // Создаём пустые WAV файлы — pipeline пройдёт preflight но упадёт
    // на providers (no settings, no creds). Нам это и нужно — мы
    // проверяем что reset SQL выполнился ДО запуска pipeline'а.
    let call_dir = tmpdir.path().join("calls").join(&call.id);
    tokio::fs::create_dir_all(&call_dir).await.unwrap();
    tokio::fs::write(call_dir.join("mic.wav"), &[0u8; 4])
        .await
        .unwrap();
    tokio::fs::write(call_dir.join("system.wav"), &[0u8; 4])
        .await
        .unwrap();

    // Pipeline упадёт (нет AppHandle для sidecar), но reset SQL должен
    // успеть выполниться раньше.
    let _ = reprocess_call(&db.pool, tmpdir.path(), &call.id, None).await;

    // После reset+fail цикл: status='failed' снова (упал на sidecar),
    // но failed_reason обновится. Главное — pipeline_* очищены.
    let after = db::get_call(&db.pool, &call.id).await.unwrap().unwrap();
    // pipeline_step мог быть проставлен step=1 из emit_progress перед
    // падением, или None если падение случилось раньше. Проверяем что
    // мы не залипли в старом 3/50%.
    assert!(
        after.pipeline_step != Some(3) || after.pipeline_pct != Some(50),
        "старый прогресс не должен сохраниться"
    );
    // failed_reason обновился из "стрый fail" на провайдеровскую ошибку.
    assert!(
        after.failed_reason.as_deref() != Some("стрый fail"),
        "старый failed_reason должен быть перезаписан"
    );
}

// ============================================================
// Local engine route — AppHandle guard
// ============================================================

/// Local engine route без AppHandle (headless test runner) должен вернуть
/// осмысленную ошибку, а не паниковать: run_local_inner требует AppHandle
/// для shell sidecar — без него Err с маркером `local_engine_no_app_handle`.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn pipeline_run_requires_app_handle_for_local_engine() {
    let db = fresh_db().await;
    let tmpdir = tempfile::tempdir().unwrap();

    let call = db::insert_recording(&db.pool, "local").await.unwrap();
    let ctx = PipelineCtx {
        call_id: call.id.clone(),
        call_dir: tmpdir.path().join(&call.id),
        mic_path: tmpdir.path().join("mic.wav"),
        system_path: tmpdir.path().join("sys.wav"),
        app_data_dir: tmpdir.path().to_path_buf(),
    };

    let result = run(&db.pool, ctx, None).await;
    let err = result.expect_err("Local engine без app handle → Err");
    let s = err.to_string();
    assert!(
        s.contains("local_engine_no_app_handle"),
        "ожидаемый маркер local_engine_no_app_handle, got: {s}"
    );

    let after = db::get_call(&db.pool, &call.id)
        .await
        .unwrap()
        .expect("call row");
    assert_eq!(after.status, "failed");
    // [TD-12] Причина обязана лечь в строку: без неё UI показывает failed
    // без объяснения, а звонок выглядит просто сломанным.
    assert!(
        after
            .failed_reason
            .as_deref()
            .is_some_and(|r| r.contains("local_engine_no_app_handle")),
        "failed_reason должен нести причину, got: {:?}",
        after.failed_reason
    );
}

// ============================================================
// [TD-33] Клей: halt-gate reprocess'а и guard rails регенерации рекапа
// ============================================================

/// Аудио на диске — иначе `reprocess_call` упрётся в audio-missing guard
/// раньше, чем дойдёт до проверяемой ветки.
async fn write_root_audio(app_data_dir: &std::path::Path, call_id: &str) -> std::path::PathBuf {
    let call_dir = app_data_dir.join("calls").join(call_id);
    tokio::fs::create_dir_all(&call_dir).await.unwrap();
    tokio::fs::write(call_dir.join("mic.wav"), &[0u8; 4])
        .await
        .unwrap();
    tokio::fs::write(call_dir.join("system.wav"), &[0u8; 4])
        .await
        .unwrap();
    call_dir
}

async fn insert_failed_chunk(pool: &sqlx::SqlitePool, call_id: &str, idx: u32) {
    use std::path::PathBuf;
    db::chunks::insert_chunk(
        pool,
        call_id,
        idx,
        u64::from(idx) * 600_000,
        &PathBuf::from(format!("/m{idx}")),
        &PathBuf::from(format!("/s{idx}")),
    )
    .await
    .unwrap();
    db::chunks::mark_chunk_processing(pool, call_id, idx)
        .await
        .unwrap();
    db::chunks::mark_chunk_failed(pool, call_id, idx, "STT timeout")
        .await
        .unwrap();
}

#[tokio::test]
async fn reprocess_call_halts_on_failed_chunk_without_touching_status_processing() {
    // [P13] До halt-гейта pipeline шёл по 1/3 чанков как по 3/3 и собирал
    // обрезанный транскрипт. Проверяем именно клей: ранний выход, причина в
    // обоих полях, и звонок НЕ уехал в processing (иначе UI крутит прогресс
    // задачи, которой нет).
    let db = fresh_db().await;
    let tmpdir = tempfile::tempdir().unwrap();
    let call = db::insert_recording(&db.pool, "local").await.unwrap();
    write_root_audio(tmpdir.path(), &call.id).await;
    insert_and_mark_done(&db.pool, &call.id, 0).await;
    insert_failed_chunk(&db.pool, &call.id, 1).await;

    let err = reprocess_call(&db.pool, tmpdir.path(), &call.id, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("chunks_need_retry"), "got: {err}");

    let after = db::get_call(&db.pool, &call.id)
        .await
        .unwrap()
        .expect("call row");
    assert_eq!(
        after.status, "failed",
        "processing без запущенного пайплайна"
    );
    assert!(
        after
            .failed_reason
            .as_deref()
            .is_some_and(|r| r.contains("chunks_need_retry")),
        "ErrorScreen читает failed_reason, got: {:?}",
        after.failed_reason
    );
    assert!(
        after
            .recap_failed_reason
            .as_deref()
            .is_some_and(|r| r.contains("chunks_need_retry")),
        "recap-баннер читает recap_failed_reason, got: {:?}",
        after.recap_failed_reason
    );
}

#[tokio::test]
async fn reprocess_call_proceeds_when_all_chunks_done() {
    // Зеркало предыдущего: при целых чанках halt-гейт не срабатывает и
    // reprocess доходит до сброса статуса (сам пайплайн упадёт дальше — без
    // AppHandle). Без этой пары гейт легко «починить» так, что он валит всё.
    let db = fresh_db().await;
    let tmpdir = tempfile::tempdir().unwrap();
    let call = db::insert_recording(&db.pool, "local").await.unwrap();
    write_root_audio(tmpdir.path(), &call.id).await;
    insert_and_mark_done(&db.pool, &call.id, 0).await;
    insert_and_mark_done(&db.pool, &call.id, 1).await;

    let err = reprocess_call(&db.pool, tmpdir.path(), &call.id, None)
        .await
        .unwrap_err();
    assert!(
        !err.to_string().contains("chunks_need_retry"),
        "halt-гейт не должен срабатывать на целых чанках, got: {err}"
    );

    let after = db::get_call(&db.pool, &call.id)
        .await
        .unwrap()
        .expect("call row");
    assert!(
        after
            .recap_failed_reason
            .as_deref()
            .is_none_or(|r| !r.contains("chunks_need_retry")),
        "чанки целы — recap-баннер про retry не ставим, got: {:?}",
        after.recap_failed_reason
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn regenerate_recap_unknown_call_returns_not_found() {
    let db = fresh_db().await;
    let tmpdir = tempfile::tempdir().unwrap();
    let err = regenerate_recap(&db.pool, tmpdir.path(), "ghost-id", None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)), "got: {err:?}");
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn regenerate_recap_without_transcript_keeps_previous_failure_reason() {
    // transcript.md обязателен: без него генерировать нечего. Важна вторая
    // половина — pre-clear стоит ПОСЛЕ чтения, поэтому прошлая причина
    // остаётся, и баннер не мигает «всё починилось» на пустом месте.
    let db = fresh_db().await;
    let tmpdir = tempfile::tempdir().unwrap();
    let call = db::insert_recording(&db.pool, "local").await.unwrap();
    db::set_recap_failed_reason(&db.pool, &call.id, Some("прошлая ошибка LLM"))
        .await
        .unwrap();

    let err = regenerate_recap(&db.pool, tmpdir.path(), &call.id, None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("transcript.md"),
        "причина должна называть недостающий файл, got: {err}"
    );

    let after = db::get_call(&db.pool, &call.id)
        .await
        .unwrap()
        .expect("call row");
    assert_eq!(
        after.recap_failed_reason.as_deref(),
        Some("прошлая ошибка LLM")
    );
}
