//! [M15.3] Indexer ассистента: transcript.md/recap.md/structured rows →
//! `assistant_passages` (FTS синхронизируют триггеры миграции 0019).
//!
//! Источник транскрипт-пассажей — `transcript.md` (финальная склейка:
//! абсолютные таймкоды, финальные speaker-теги). Chunk-JSON НЕ читаем:
//! там секунды относительно чанка + пришлось бы повторять merge/remap
//! из `chunk_assembly` (PRD §6.1, поправка M15.3).
//!
//! Идемпотентность: `index_call` всегда делает полную переиндексацию
//! (`replace_call_passages` — DELETE+INSERT одной транзакцией).

use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::assistant::passages::{
    build_call_meta_passage, build_recap_passages, build_structured_passages,
    build_transcript_passages, parse_transcript_turns,
};
use crate::call_store::{ArtifactKind, CallStore};
use crate::db::assistant::PassageInput;
use crate::AppError;

/// Полная (пере)индексация звонка. Возвращает (passage_count, token_total).
/// Отсутствие transcript.md/recap.md — не ошибка (индексируем что есть).
pub async fn index_call(
    pool: &SqlitePool,
    store: &CallStore,
    call_id: &str,
) -> Result<(i64, i64), AppError> {
    // [M15.10] Эмбеддер резолвится из shared-кэша по app_data_dir store —
    // сигнатуры ready-хуков не меняются. Нет модели/feature → None → FTS-only.
    let embedder = crate::assistant::embedder::shared(store.app_data_dir()).await;
    index_call_with(pool, store, call_id, embedder).await
}

/// DI-вариант `index_call` — тесты подсовывают `MockEmbedder`.
pub(crate) async fn index_call_with(
    pool: &SqlitePool,
    store: &CallStore,
    call_id: &str,
    embedder: Option<std::sync::Arc<dyn crate::assistant::embedder::TextEmbedder>>,
) -> Result<(i64, i64), AppError> {
    let mut passages: Vec<PassageInput> = Vec::new();

    // [M16.6] Подтверждённые привязки спикер→контакт: имена в пассажи
    // (поле speaker + префиксы строк текста → имя ищется через FTS).
    let names: std::collections::HashMap<String, String> =
        crate::db::list_call_speakers(pool, call_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.confirmed)
            .filter_map(|s| s.contact_display_name.map(|n| (s.speaker_tag, n)))
            .collect();

    // [M16.6] Карточка звонка (титул + дата + участники) — первый пассаж.
    let call_row: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT title, started_at FROM calls WHERE id = ?1")
            .bind(call_id)
            .fetch_optional(pool)
            .await?;
    if let Some((title, started_at)) = call_row {
        let mut participants: Vec<String> = names.values().cloned().collect();
        participants.sort();
        participants.dedup();
        passages.extend(build_call_meta_passage(
            title.as_deref(),
            &started_at,
            &participants,
        ));
    }

    if let Some(md) = store
        .read_artifact(
            &crate::call_id::CallId::from_db(call_id),
            ArtifactKind::Transcript,
        )
        .await?
    {
        passages.extend(build_transcript_passages(
            &parse_transcript_turns(&md),
            &names,
        ));
    }
    if let Some(md) = store
        .read_artifact(
            &crate::call_id::CallId::from_db(call_id),
            ArtifactKind::Recap,
        )
        .await?
    {
        passages.extend(build_recap_passages(&md));
    }
    let decisions = crate::db::decisions::list_decisions(pool, call_id).await?;
    let action_items = crate::db::list_action_items(pool, call_id).await?;
    let open_questions = crate::db::open_questions::list_open_questions(pool, call_id).await?;
    passages.extend(build_structured_passages(
        &decisions,
        &action_items,
        &open_questions,
        &names,
    ));

    let (count, tokens) =
        crate::db::assistant::replace_call_passages(pool, call_id, &passages).await?;
    log::info!("assistant index[{call_id}]: {count} passages, ~{tokens} tokens");

    // [M15.10] Batch-эмбеддинг вставленных пассажей. Ошибки НЕ роняют
    // индексацию: FTS-индекс важнее, недостающие вектора доберёт
    // embed_backfill (list_passages_missing_embedding).
    if let Some(emb) = embedder {
        if let Err(e) = embed_call_passages(pool, emb, call_id).await {
            log::warn!("assistant embed[{call_id}]: {e}");
        }
    }
    Ok((count, tokens))
}

/// Векторизовать все пассажи звонка (после `replace_call_passages`, который
/// id вставленных строк не возвращает — отдельный SELECT).
pub(crate) async fn embed_call_passages(
    pool: &SqlitePool,
    emb: std::sync::Arc<dyn crate::assistant::embedder::TextEmbedder>,
    call_id: &str,
) -> Result<usize, AppError> {
    let rows = crate::db::assistant_embeddings::list_call_passage_texts(pool, call_id).await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let dim = emb.dim() as i64;
    let blobs = embed_batch(emb, rows).await?;
    crate::db::assistant_embeddings::upsert_embeddings(pool, dim, &blobs).await?;
    Ok(blobs.len())
}

/// Инференс батча вне async-потока (ONNX ~5-90мс на пассаж).
async fn embed_batch(
    emb: std::sync::Arc<dyn crate::assistant::embedder::TextEmbedder>,
    rows: Vec<(i64, String)>,
) -> Result<Vec<(i64, Vec<u8>)>, AppError> {
    tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = rows.iter().map(|(_, t)| t.as_str()).collect();
        let vecs = emb.embed_passages(&refs)?;
        // [TD-19] `zip` молча усекает по короткой стороне: при недоборе
        // векторов (баг реализации TextEmbedder, OOM в ONNX) лишние пассажи
        // просто выпадали без ошибки и оставались missing — а backfill-цикл
        // ниже крутится «пока есть missing», то есть уходил в вечный цикл
        // без прогресса и без единого лога.
        if vecs.len() != rows.len() {
            return Err(AppError::Other(format!(
                "embed_passages вернул {} векторов на {} пассажей",
                vecs.len(),
                rows.len()
            )));
        }
        Ok(rows
            .iter()
            .zip(vecs.iter())
            .map(|((id, _), v)| (*id, crate::embeddings::embedding_to_bytes(v)))
            .collect())
    })
    .await
    .map_err(|e| AppError::Other(format!("embed join: {e}")))?
}

/// [M15.10] Размер батча фонового embed-backfill'а.
const EMBED_BACKFILL_BATCH: i64 = 64;

/// Фоновый backfill векторов: добирает пассажи без эмбеддинга батчами —
/// существующие Ph1-звонки и хвосты после warn'ов embed-hook'а. No-op без
/// модели/feature. Перед стартом — инвалидация по id модели (M15.10.3).
pub async fn embed_backfill(pool: &SqlitePool, app_data_dir: &std::path::Path) {
    let Some(emb) = crate::assistant::embedder::shared(app_data_dir).await else {
        return;
    };
    if let Err(e) = crate::assistant::embedder::ensure_embed_model_current(pool).await {
        log::warn!("assistant embed backfill: ensure model: {e}");
        return;
    }
    match embed_backfill_with(pool, emb).await {
        Ok(0) => {}
        Ok(n) => log::info!("assistant embed backfill: {n} passages embedded"),
        Err(e) => log::warn!("assistant embed backfill: {e}"),
    }
}

/// DI-вариант backfill'а (тесты — MockEmbedder). Ошибка прерывает цикл
/// (не зацикливаемся на стабильно падающем батче), недобранное останется
/// в missing-листинге до следующего старта.
pub(crate) async fn embed_backfill_with(
    pool: &SqlitePool,
    emb: std::sync::Arc<dyn crate::assistant::embedder::TextEmbedder>,
) -> Result<usize, AppError> {
    let mut total = 0usize;
    loop {
        let rows = crate::db::assistant_embeddings::list_passages_missing_embedding(
            pool,
            EMBED_BACKFILL_BATCH,
        )
        .await?;
        if rows.is_empty() {
            break;
        }
        let dim = emb.dim() as i64;
        let rows_len = rows.len();
        let blobs = embed_batch(emb.clone(), rows).await?;
        // [TD-19] Страховка от вечного цикла: если итерация не дала прогресса,
        // те же строки вернутся из list_passages_missing_embedding на следующем
        // круге. Проверка выше уже ловит недобор ошибкой, но guard оставляем —
        // цикл не должен зависеть от одного-единственного инварианта.
        if blobs.is_empty() {
            log::warn!(
                "embed_backfill: итерация не дала прогресса на {rows_len} строках — прерываем"
            );
            break;
        }
        crate::db::assistant_embeddings::upsert_embeddings(pool, dim, &blobs).await?;
        total += blobs.len();
    }
    Ok(total)
}

/// Fire-and-forget индексация из ready-хуков пайплайна. Ошибки — warn,
/// пайплайн не роняем. Self-heal: при фейле сносим index_state, чтобы
/// startup-backfill переиндексировал (иначе regen-случай навсегда оставил бы
/// в поиске до-regen контент — старая запись state скрывает звонок от sweep'а).
pub fn spawn_index(app: &AppHandle, call_id: &str) {
    let app = app.clone();
    let call_id = call_id.to_string();
    tauri::async_runtime::spawn(async move {
        let (pool, store) = {
            let state = tauri::Manager::state::<crate::state::AppState>(&app);
            (state.db.clone(), state.store.clone())
        };
        if let Err(e) = index_call(&pool, &store, &call_id).await {
            log::warn!("assistant index[{call_id}] failed: {e}");
            if let Err(e2) = crate::db::assistant::clear_index_state(&pool, &call_id).await {
                log::warn!("assistant index[{call_id}]: clear_index_state failed too: {e2}");
            }
        }
    });
}

/// Деиндексация (reprocess: звонок уходит из ready).
pub async fn deindex_call(pool: &SqlitePool, call_id: &str) -> Result<(), AppError> {
    crate::db::assistant::delete_call_passages(pool, call_id).await
}

/// Startup-backfill: ready-звонки без записи в assistant_index_state.
/// Последовательно (не грузим диск), ошибки отдельных звонков — warn.
pub async fn backfill(pool: &SqlitePool, store: &CallStore) {
    let pending: Vec<(String,)> = match sqlx::query_as(
        "SELECT c.id FROM calls c
         LEFT JOIN assistant_index_state s ON s.call_id = c.id
         WHERE c.status = 'ready' AND s.call_id IS NULL
         ORDER BY c.started_at ASC",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!("assistant backfill: query failed: {e}");
            return;
        }
    };
    if pending.is_empty() {
        return;
    }
    let mut ok = 0usize;
    for (call_id,) in &pending {
        // Защита от гонки с live-хуками (regen/reprocess во время sweep'а):
        // если звонок уже ушёл из ready или получил index_state — скип.
        let still_pending: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM calls c
             LEFT JOIN assistant_index_state s ON s.call_id = c.id
             WHERE c.id = ?1 AND c.status = 'ready' AND s.call_id IS NULL",
        )
        .bind(call_id)
        .fetch_optional(pool)
        .await
        // Ошибка запроса ≠ «уже не pending» — логируем перед скипом
        // (rust-review Ph2), молчание маскировало бы падение БД.
        .unwrap_or_else(|e| {
            log::warn!("assistant backfill[{call_id}]: still_pending check failed: {e}");
            None
        });
        if still_pending.is_none() {
            continue;
        }
        match index_call(pool, store, call_id).await {
            Ok(_) => ok += 1,
            Err(e) => log::warn!("assistant backfill[{call_id}] failed: {e}"),
        }
    }
    log::info!("assistant backfill: {ok}/{} calls indexed", pending.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::passages::{Turn, SAMPLE_MD_FOR_TESTS as SAMPLE_MD};
    use crate::assistant::types::AssistantPassageKind;
    use crate::db::test_support::fresh_db;
    use std::path::PathBuf;

    // ── index_call / backfill (fresh_db + временный CallStore) ──

    async fn seed_call(pool: &sqlx::SqlitePool, id: &str, status: &str) {
        sqlx::query(
            "INSERT INTO calls (id, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 300, ?2, 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(id)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }

    fn store_with_artifacts(dir: &std::path::Path, call_id: &str) -> CallStore {
        let call_dir = dir.join("calls").join(call_id);
        std::fs::create_dir_all(&call_dir).unwrap();
        std::fs::write(call_dir.join("transcript.md"), SAMPLE_MD).unwrap();
        std::fs::write(
            call_dir.join("recap.md"),
            "# Рекап\n\nОбсудили сроки пилота и приватность.\n",
        )
        .unwrap();
        CallStore::new(PathBuf::from(dir))
    }

    #[tokio::test]
    async fn index_call_end_to_end_with_fts() {
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        seed_call(&db.pool, "c1", "ready").await;
        let store = store_with_artifacts(tmp.path(), "c1");

        let (count, tokens) = index_call(&db.pool, &store, "c1").await.unwrap();
        assert!(count >= 2, "transcript + recap passages, got {count}");
        assert!(tokens > 0);

        let hits = crate::db::assistant::search_fts(&db.pool, "\"пилот\"*", 10, None, None)
            .await
            .unwrap();
        assert!(!hits.is_empty(), "FTS must find indexed transcript");
        // Переиндексация идемпотентна.
        let (count2, _) = index_call(&db.pool, &store, "c1").await.unwrap();
        assert_eq!(count, count2);
    }

    // ============================================================
    // [TD-19] Батч эмбеддингов без молчаливых потерь
    // ============================================================

    /// Эмбеддер-недоборщик: возвращает на один вектор меньше, чем просили.
    /// Имитирует баг реализации TextEmbedder / OOM в ONNX.
    struct ShortEmbedder;

    impl crate::assistant::embedder::TextEmbedder for ShortEmbedder {
        fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, AppError> {
            // На один меньше — ровно тот случай, что `zip` глотал.
            Ok(texts.iter().skip(1).map(|_| vec![0.0f32; 8]).collect())
        }
        fn embed_query(&self, _q: &str) -> Result<Vec<f32>, AppError> {
            Ok(vec![0.0f32; 8])
        }
        fn dim(&self) -> usize {
            8
        }
    }

    #[tokio::test]
    async fn embed_batch_errors_on_vector_count_mismatch() {
        // Регрессия TD-19: `rows.iter().zip(vecs.iter())` усекал по короткой
        // стороне — лишние пассажи выпадали БЕЗ ошибки и оставались missing.
        let rows = vec![
            (1i64, "первый".to_string()),
            (2i64, "второй".to_string()),
            (3i64, "третий".to_string()),
        ];
        let err = embed_batch(std::sync::Arc::new(ShortEmbedder), rows)
            .await
            .expect_err("недобор векторов обязан быть ошибкой, а не тихим усечением");
        let msg = format!("{err}");
        assert!(
            msg.contains("2") && msg.contains("3"),
            "ошибка обязана назвать оба числа, получили: {msg}"
        );
    }

    #[tokio::test]
    async fn embed_backfill_terminates_on_faulty_embedder() {
        // Ключевое: цикл «пока есть missing» не должен крутиться вечно.
        // Тест завершается — значит бесконечного цикла нет. Сам факт
        // возврата (Err или Ok) и есть проверка.
        use crate::assistant::embedder::test_support::MockEmbedder;
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        seed_call(&db.pool, "c1", "ready").await;
        let store = store_with_artifacts(tmp.path(), "c1");
        index_call_with(
            &db.pool,
            &store,
            "c1",
            Some(std::sync::Arc::new(MockEmbedder)),
        )
        .await
        .unwrap();

        // Сносим эмбеддинги, чтобы backfill нашёл missing-строки.
        sqlx::query("DELETE FROM assistant_embeddings")
            .execute(&db.pool)
            .await
            .unwrap();

        let res = embed_backfill_with(&db.pool, std::sync::Arc::new(ShortEmbedder)).await;
        // Либо Err от проверки количества, либо Ok с break по нулевому
        // прогрессу — оба исхода означают «цикл завершился».
        assert!(
            res.is_err() || res.unwrap_or(0) == 0,
            "backfill обязан прерваться, а не наматывать круги"
        );
    }

    // ── [M15.10] embed-hook + embed_backfill (MockEmbedder) ──

    async fn count_embeddings(pool: &sqlx::SqlitePool) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM assistant_embeddings")
            .fetch_one(pool)
            .await
            .unwrap();
        n
    }

    #[tokio::test]
    async fn index_call_with_mock_embedder_writes_vectors() {
        use crate::assistant::embedder::test_support::MockEmbedder;

        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        seed_call(&db.pool, "c1", "ready").await;
        let store = store_with_artifacts(tmp.path(), "c1");

        let (count, _) = index_call_with(
            &db.pool,
            &store,
            "c1",
            Some(std::sync::Arc::new(MockEmbedder)),
        )
        .await
        .unwrap();
        assert_eq!(
            count_embeddings(&db.pool).await,
            count,
            "каждый пассаж получает вектор"
        );
        let (dim,): (i64,) = sqlx::query_as("SELECT DISTINCT dim FROM assistant_embeddings")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(dim as usize, crate::assistant::embedder::EMBED_DIM);

        // Переиндексация идемпотентна и по векторам (каскад + re-embed).
        let (count2, _) = index_call_with(
            &db.pool,
            &store,
            "c1",
            Some(std::sync::Arc::new(MockEmbedder)),
        )
        .await
        .unwrap();
        assert_eq!(count, count2);
        assert_eq!(count_embeddings(&db.pool).await, count2);
    }

    #[tokio::test]
    async fn index_call_without_embedder_writes_fts_only() {
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        seed_call(&db.pool, "c1", "ready").await;
        let store = store_with_artifacts(tmp.path(), "c1");

        let (count, _) = index_call_with(&db.pool, &store, "c1", None).await.unwrap();
        assert!(count > 0);
        assert_eq!(
            count_embeddings(&db.pool).await,
            0,
            "без эмбеддера — только FTS"
        );
    }

    #[tokio::test]
    async fn embed_backfill_fills_missing_and_is_idempotent() {
        use crate::assistant::embedder::test_support::MockEmbedder;

        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        seed_call(&db.pool, "c1", "ready").await;
        let store = store_with_artifacts(tmp.path(), "c1");
        let (count, _) = index_call_with(&db.pool, &store, "c1", None).await.unwrap();
        assert_eq!(count_embeddings(&db.pool).await, 0);

        let n = embed_backfill_with(&db.pool, std::sync::Arc::new(MockEmbedder))
            .await
            .unwrap();
        assert_eq!(n as i64, count, "backfill добирает все пассажи без вектора");
        assert_eq!(count_embeddings(&db.pool).await, count);

        // Повторный прогон — нечего добирать.
        let n2 = embed_backfill_with(&db.pool, std::sync::Arc::new(MockEmbedder))
            .await
            .unwrap();
        assert_eq!(n2, 0);
    }

    #[tokio::test]
    async fn index_call_without_artifacts_keeps_only_call_card() {
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        seed_call(&db.pool, "c1", "ready").await;
        let store = CallStore::new(tmp.path().to_path_buf());
        // [M16.6] Артефактов нет, но карточка звонка (титул+дата) есть всегда.
        let (count, tokens) = index_call(&db.pool, &store, "c1").await.unwrap();
        assert_eq!(count, 1, "только call_meta карточка");
        assert!(tokens > 0);
        // index_state есть (backfill не зациклится).
        let stats = crate::db::assistant::index_stats(&db.pool).await.unwrap();
        assert_eq!(stats.indexed_calls, 1);
    }

    // ── [M16.6] Резолв имён + карточка звонка ──

    #[test]
    fn transcript_speaker_names_resolved_from_map() {
        let turns = vec![
            Turn {
                speaker_tag: "speaker:1".into(),
                start_ms: 0,
                text: "предлагаю стартовать".into(),
            },
            Turn {
                speaker_tag: "speaker:2".into(),
                start_ms: 5_000,
                text: "согласен".into(),
            },
        ];
        let names =
            std::collections::HashMap::from([("speaker:1".to_string(), "Дамир Н.".to_string())]);
        let ps = build_transcript_passages(&turns, &names);
        assert_eq!(
            ps[0].speaker.as_deref(),
            Some("Дамир Н."),
            "привязанный — имя"
        );
        assert!(
            ps[0].text.contains("Дамир Н.: предлагаю"),
            "имя в тексте (FTS): {}",
            ps[0].text
        );
        assert!(
            ps[0].text.contains("speaker:2: согласен"),
            "непривязанный — сырой тег"
        );
    }

    #[test]
    fn call_meta_card_contains_title_date_participants() {
        let card = build_call_meta_passage(
            Some("Планёрка продукта"),
            "2026-07-01T09:29:36+00:00",
            &["Дамир".to_string(), "Глеб".to_string()],
        )
        .unwrap();
        assert_eq!(card.kind, AssistantPassageKind::CallMeta);
        assert_eq!(
            card.text,
            "Звонок «Планёрка продукта» — 01.07.2026. Участники: Дамир, Глеб."
        );
        // Без титула и участников — только дата.
        let bare = build_call_meta_passage(None, "2026-07-01T09:29:36+00:00", &[]).unwrap();
        assert_eq!(bare.text, "Звонок от 01.07.2026.");
    }

    #[tokio::test]
    async fn index_call_card_is_searchable_by_title_word() {
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        sqlx::query(
            "INSERT INTO calls (id, title, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES ('c1', 'Реструктуризация организаций', '2026-07-01T09:29:36+00:00', 300, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&db.pool)
        .await
        .unwrap();
        let store = store_with_artifacts(tmp.path(), "c1");
        index_call(&db.pool, &store, "c1").await.unwrap();

        // Слово из титула теперь находит звонок (раньше титулы не в индексе).
        let hits =
            crate::db::assistant::search_fts(&db.pool, "\"реструктуризац\"*", 10, None, None)
                .await
                .unwrap();
        assert!(!hits.is_empty(), "карточка звонка обязана матчиться");
        assert_eq!(hits[0].kind, "call_meta");
    }

    #[tokio::test]
    async fn deindex_call_clears_index_and_stats() {
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        seed_call(&db.pool, "c1", "ready").await;
        let store = store_with_artifacts(tmp.path(), "c1");
        index_call(&db.pool, &store, "c1").await.unwrap();

        deindex_call(&db.pool, "c1").await.unwrap();

        let stats = crate::db::assistant::index_stats(&db.pool).await.unwrap();
        assert_eq!(stats.indexed_calls, 0);
        let hits = crate::db::assistant::search_fts(&db.pool, "\"пилот\"*", 10, None, None)
            .await
            .unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn backfill_indexes_only_ready_without_state() {
        let db = fresh_db().await;
        let tmp = tempfile::tempdir().unwrap();
        seed_call(&db.pool, "ready1", "ready").await;
        seed_call(&db.pool, "proc1", "processing").await;
        seed_call(&db.pool, "done_before", "ready").await;
        let store = store_with_artifacts(tmp.path(), "ready1");
        // done_before уже индексирован — backfill не должен его трогать.
        index_call(&db.pool, &store, "done_before").await.unwrap();

        backfill(&db.pool, &store).await;

        let stats = crate::db::assistant::index_stats(&db.pool).await.unwrap();
        assert_eq!(
            stats.indexed_calls, 2,
            "ready1 + done_before, без processing"
        );
    }
}
