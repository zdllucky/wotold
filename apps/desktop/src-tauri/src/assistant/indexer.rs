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

use crate::assistant::types::AssistantPassageKind;
use crate::call_store::{ArtifactKind, CallStore};
use crate::db::assistant::PassageInput;
use crate::pipeline::chunker::estimate_tokens;
use crate::AppError;

/// Целевой размер транскрипт-пассажа (окно speaker-turn'ов), в токенах.
/// ~350 ток ≈ 1.4KB кириллицы — 12-16 пассажей входят в retrieval-бюджет 5.5K.
const TRANSCRIPT_PASSAGE_TARGET_TOKENS: usize = 350;

/// Одна реплика транскрипта (заголовок `**{tag}** [{m}:{ss}]:` + текст).
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub speaker_tag: String,
    pub start_ms: i64,
    pub text: String,
}

/// Парс transcript.md → реплики. Формат из `merge.rs::render_transcript_md`:
/// строка-заголовок `**owner** [0:02]:` (закрывающие `**` ДО таймкода),
/// затем строки текста до следующего заголовка. Битые заголовки скипаются.
pub fn parse_transcript_turns(md: &str) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    let mut current: Option<Turn> = None;
    for line in md.lines() {
        if is_speaker_header_line(line) {
            if let Some(t) = current.take() {
                if !t.text.trim().is_empty() {
                    turns.push(t);
                }
            }
            current = parse_header_line(line);
            if current.is_none() {
                // Инвариант: системные теги (owner/Speaker N) не содержат `**`.
                // Битый заголовок роняет свой блок — оставляем след в логе.
                log::debug!("assistant indexer: unparsable header line skipped: {line:?}");
            }
            continue;
        }
        if let Some(t) = current.as_mut() {
            if !line.trim().is_empty() {
                if !t.text.is_empty() {
                    t.text.push(' ');
                }
                t.text.push_str(line.trim());
            }
        }
    }
    if let Some(t) = current.take() {
        if !t.text.trim().is_empty() {
            turns.push(t);
        }
    }
    turns
}

// Тот же критерий что chunker.rs::is_speaker_header_line — единый формат.
fn is_speaker_header_line(line: &str) -> bool {
    line.starts_with("**") && line.contains("]:")
}

/// `**{tag}** [{m}:{ss}]:` → (tag, ms). None если строка не парсится.
fn parse_header_line(line: &str) -> Option<Turn> {
    let rest = line.strip_prefix("**")?;
    let (tag, after_tag) = rest.split_once("**")?;
    let after_tag = after_tag.trim_start();
    let ts = after_tag.strip_prefix('[')?;
    let (clock, _) = ts.split_once("]:")?;
    let (min, sec) = clock.trim().split_once(':')?;
    let min: i64 = min.trim().parse().ok()?;
    let sec: i64 = sec.trim().parse().ok()?;
    if !(0..60).contains(&sec) || min < 0 {
        return None;
    }
    Some(Turn {
        speaker_tag: tag.trim().to_string(),
        start_ms: (min * 60 + sec) * 1000,
        text: String::new(),
    })
}

/// [M16.6] Резолв speaker-тега в имя: подтверждённая привязка → display_name,
/// иначе сырой тег. Имя попадает и в поле speaker, и в текст пассажа —
/// «что говорил Дамир» начинает матчить FTS, а не только устные упоминания.
fn resolve_speaker<'a>(
    names: &'a std::collections::HashMap<String, String>,
    tag: &'a str,
) -> &'a str {
    names.get(tag).map(String::as_str).unwrap_or(tag)
}

/// Окна последовательных реплик до ~350 ток, overlap = 1 реплика.
/// speaker/start_ms — от первой реплики окна; end_ms = start следующего окна.
/// [M16.6] `names`: speaker_tag → подтверждённое имя контакта.
pub fn build_transcript_passages(
    turns: &[Turn],
    names: &std::collections::HashMap<String, String>,
) -> Vec<PassageInput> {
    let mut windows: Vec<(usize, usize)> = Vec::new(); // [from, to) по turns
    let mut from = 0usize;
    while from < turns.len() {
        let mut to = from;
        let mut tokens = 0usize;
        while to < turns.len() {
            let t = estimate_tokens(&turns[to].text);
            if to > from && tokens + t > TRANSCRIPT_PASSAGE_TARGET_TOKENS {
                break;
            }
            tokens += t;
            to += 1;
        }
        windows.push((from, to));
        if to >= turns.len() {
            break;
        }
        // Overlap: следующее окно стартует с последней реплики текущего —
        // но только для окон из ≥2 реплик, иначе курсор не двигается
        // (одиночная oversized-реплика зацикливала бы нарезку).
        from = if to - from > 1 { to - 1 } else { to };
    }

    windows
        .iter()
        .map(|&(a, b)| {
            let text = turns[a..b]
                .iter()
                .map(|t| format!("{}: {}", resolve_speaker(names, &t.speaker_tag), t.text))
                .collect::<Vec<_>>()
                .join("\n");
            let end_ms = turns.get(b).map(|next| next.start_ms);
            PassageInput {
                kind: AssistantPassageKind::Transcript,
                speaker: Some(resolve_speaker(names, &turns[a].speaker_tag).to_string()),
                start_ms: Some(turns[a].start_ms),
                end_ms,
                token_est: estimate_tokens(&text) as i64,
                text,
            }
        })
        .collect()
}

/// [M16.6] Синтетическая «карточка звонка»: титул + дата + участники.
/// Якорь для «в каком звонке / кто был / о чём» — раньше титулы и даты
/// вообще не индексировались.
pub fn build_call_meta_passage(
    title: Option<&str>,
    started_at: &str,
    participants: &[String],
) -> Option<PassageInput> {
    let date = started_at.get(..10).map(|d| {
        let mut it = d.split('-');
        match (it.next(), it.next(), it.next()) {
            (Some(y), Some(m), Some(day)) => format!("{day}.{m}.{y}"),
            _ => d.to_string(),
        }
    })?;
    let mut text = match title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => format!("Звонок «{t}» — {date}."),
        None => format!("Звонок от {date}."),
    };
    if !participants.is_empty() {
        text.push_str(&format!(" Участники: {}.", participants.join(", ")));
    }
    Some(PassageInput {
        kind: AssistantPassageKind::CallMeta,
        speaker: None,
        start_ms: None,
        end_ms: None,
        token_est: estimate_tokens(&text) as i64,
        text,
    })
}

/// recap.md → пассажи-абзацы. Заголовки (`#…`) скипаются, буллет-группы
/// между пустыми строками идут одним пассажем. start_ms = None.
pub fn build_recap_passages(md: &str) -> Vec<PassageInput> {
    // CRLF-нормализация: иначе `\r\n\r\n` не матчит разделитель абзацев.
    let md = md.replace("\r\n", "\n");
    md.split("\n\n")
        .map(|block| {
            block
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.trim().is_empty())
        .map(|text| PassageInput {
            kind: AssistantPassageKind::Recap,
            speaker: None,
            start_ms: None,
            end_ms: None,
            token_est: estimate_tokens(&text) as i64,
            text,
        })
        .collect()
}

/// Общая форма structured-строки: текст + опциональная цитата-evidence.
fn structured_passage(
    kind: AssistantPassageKind,
    text: &str,
    quote: Option<&str>,
    speaker: Option<&str>,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Option<PassageInput> {
    let base = text.trim();
    if base.is_empty() {
        return None;
    }
    let full = match quote.map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => format!("{base} — цитата: {q}"),
        None => base.to_string(),
    };
    Some(PassageInput {
        kind,
        speaker: speaker.map(str::to_string),
        start_ms,
        end_ms,
        token_est: estimate_tokens(&full) as i64,
        text: full,
    })
}

/// decisions / action_items / open_questions → по одному пассажу на строку.
pub fn build_structured_passages(
    decisions: &[crate::db::decisions::DecisionRow],
    action_items: &[crate::db::ActionItem],
    open_questions: &[crate::db::open_questions::OpenQuestionRow],
    names: &std::collections::HashMap<String, String>,
) -> Vec<PassageInput> {
    let mut out = Vec::new();
    for d in decisions {
        out.extend(structured_passage(
            AssistantPassageKind::Decision,
            &d.text,
            d.evidence_quote.as_deref(),
            d.evidence_speaker
                .as_deref()
                .map(|t| resolve_speaker(names, t)),
            d.evidence_start_ms,
            d.evidence_end_ms,
        ));
    }
    for a in action_items {
        out.extend(structured_passage(
            AssistantPassageKind::ActionItem,
            &a.text,
            a.evidence_quote.as_deref(),
            a.evidence_speaker
                .as_deref()
                .map(|t| resolve_speaker(names, t)),
            a.evidence_start_ms,
            None,
        ));
    }
    for q in open_questions {
        out.extend(structured_passage(
            AssistantPassageKind::OpenQuestion,
            &q.text,
            q.evidence_quote.as_deref(),
            q.evidence_speaker
                .as_deref()
                .map(|t| resolve_speaker(names, t)),
            q.evidence_start_ms,
            None,
        ));
    }
    out
}

// ── Оркестрация ───────────────────────────────────────────────────────

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
        .read_artifact(call_id, ArtifactKind::Transcript)
        .await?
    {
        passages.extend(build_transcript_passages(
            &parse_transcript_turns(&md),
            &names,
        ));
    }
    if let Some(md) = store.read_artifact(call_id, ArtifactKind::Recap).await? {
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
        let blobs = embed_batch(emb.clone(), rows).await?;
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
    use crate::db::test_support::fresh_db;
    use std::path::PathBuf;

    const SAMPLE_MD: &str = "# Transcript\n\n\
**owner** [0:00]:\nДавайте сверимся по срокам пилота.\n\n\
**Speaker 0** [0:11]:\nПо нашей части всё в графике. Отчёт будет к пятнице.\n\n\
**Speaker 1** [1:05]:\nУ меня вопрос по разделению голосов.\n\n\
**owner** [73:20]:\nИтого — фиксируем решения.\n";

    // ── parse_transcript_turns ──

    #[test]
    fn parses_real_transcript_format() {
        let turns = parse_transcript_turns(SAMPLE_MD);
        assert_eq!(turns.len(), 4);
        assert_eq!(turns[0].speaker_tag, "owner");
        assert_eq!(turns[0].start_ms, 0);
        assert_eq!(turns[0].text, "Давайте сверимся по срокам пилота.");
        assert_eq!(turns[1].speaker_tag, "Speaker 0");
        assert_eq!(turns[1].start_ms, 11_000);
        assert_eq!(turns[2].start_ms, 65_000);
        // Минуты без часов: [73:20] = 73*60+20.
        assert_eq!(turns[3].start_ms, (73 * 60 + 20) * 1000);
    }

    #[test]
    fn multiline_turn_text_is_joined() {
        let md = "**owner** [0:05]:\nпервая строка\nвторая строка\n";
        let turns = parse_transcript_turns(md);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].text, "первая строка вторая строка");
    }

    #[test]
    fn broken_or_empty_md_yields_no_turns() {
        assert!(parse_transcript_turns("").is_empty());
        assert!(parse_transcript_turns("# Transcript\n\nпросто текст без заголовков").is_empty());
        // Битый заголовок (нет таймкода) — скип вместе с текстом под ним.
        assert!(parse_transcript_turns("**owner** без таймкода:\nтекст\n").is_empty());
        // Невалидные секунды.
        assert!(parse_transcript_turns("**owner** [0:99]:\nтекст\n").is_empty());
    }

    // ── build_transcript_passages ──

    fn turn(tag: &str, start_ms: i64, len_bytes: usize) -> Turn {
        Turn {
            speaker_tag: tag.into(),
            start_ms,
            text: "д".repeat(len_bytes / 2), // кириллица = 2 байта/символ
        }
    }

    #[test]
    fn windows_respect_target_and_overlap() {
        // Каждая реплика ~150 ток (600 байт) → окно вмещает 2 (300 ≤ 350,
        // третья давала бы 450) → окна с overlap 1: [0,2], [1,3], [2,4].
        let turns: Vec<Turn> = (0..4).map(|i| turn("Speaker 0", i * 10_000, 600)).collect();
        let ps = build_transcript_passages(&turns, &std::collections::HashMap::new());
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[0].start_ms, Some(0));
        assert_eq!(ps[0].end_ms, Some(20_000)); // старт turn[2] (первого вне окна)
        assert_eq!(ps[1].start_ms, Some(10_000)); // overlap: окно с turn[1]
        assert!(ps[2].end_ms.is_none()); // последнее окно
        for p in &ps {
            assert_eq!(p.kind, AssistantPassageKind::Transcript);
            assert!(p.token_est > 0);
        }
    }

    #[test]
    fn oversized_single_turn_is_own_passage() {
        // Реплика больше таргета не делится и не зацикливает алгоритм.
        let turns = vec![turn("owner", 0, 4_000), turn("Speaker 0", 5_000, 100)];
        let ps = build_transcript_passages(&turns, &std::collections::HashMap::new());
        assert_eq!(ps.len(), 2);
        assert!(ps[0].token_est as usize > TRANSCRIPT_PASSAGE_TARGET_TOKENS);
    }

    #[test]
    fn passage_text_carries_speaker_tags() {
        let turns = vec![turn("owner", 0, 40), turn("Speaker 0", 1_000, 40)];
        let ps = build_transcript_passages(&turns, &std::collections::HashMap::new());
        assert_eq!(ps.len(), 1);
        assert!(ps[0].text.starts_with("owner: "));
        assert!(ps[0].text.contains("\nSpeaker 0: "));
        assert_eq!(ps[0].speaker.as_deref(), Some("owner"));
    }

    #[test]
    fn empty_turns_yield_no_passages() {
        assert!(build_transcript_passages(&[], &std::collections::HashMap::new()).is_empty());
    }

    #[test]
    fn window_boundary_exactly_at_target_stays_open() {
        // 175 ток + 175 ток = ровно 350 (НЕ > TARGET) → обе в одном окне;
        // третья (350+175 > 350) — уже нет.
        let turns = vec![
            turn("owner", 0, 700),
            turn("Speaker 0", 1_000, 700),
            turn("Speaker 1", 2_000, 700),
        ];
        let ps = build_transcript_passages(&turns, &std::collections::HashMap::new());
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].end_ms, Some(2_000)); // окно [0,2), следующее начинается с turn[2]... с overlap [1,3)
    }

    // ── build_recap_passages ──

    #[test]
    fn recap_paragraphs_skip_headings() {
        let md = "# Рекап\n\nСинхрон по пилоту перед демо.\n\n## Решения\n\n- Локальный режим на демо.\n- Отчёт к пятнице.\n\n## Пустая секция\n\n";
        let ps = build_recap_passages(md);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].text, "Синхрон по пилоту перед демо.");
        assert!(ps[1].text.contains("Локальный режим"));
        assert!(ps[1].text.contains("\n- Отчёт"));
        assert!(ps.iter().all(|p| p.kind == AssistantPassageKind::Recap));
        assert!(ps.iter().all(|p| p.start_ms.is_none()));
    }

    // ── build_structured_passages ──

    #[test]
    fn structured_rows_map_with_and_without_evidence() {
        let decisions = vec![crate::db::decisions::DecisionRow {
            id: "d1".into(),
            call_id: "c1".into(),
            text: "Показываем локальный режим".into(),
            evidence_quote: Some("давайте зафиксируем".into()),
            evidence_speaker: Some("Speaker 0".into()),
            evidence_start_ms: Some(62_000),
            evidence_end_ms: Some(70_000),
            confidence: Some(0.9),
            order_idx: 0,
        }];
        let items = vec![crate::db::ActionItem {
            id: "a1".into(),
            call_id: "c1".into(),
            text: "Стенд для нагрузочных".into(),
            owner_contact_id: None,
            due: Some("четверг".into()),
            done: false,
            owner_confidence: None,
            due_confidence: None,
            category: Some("commitment".into()),
            evidence_quote: None,
            evidence_speaker: None,
            evidence_start_ms: None,
        }];
        let questions = vec![crate::db::open_questions::OpenQuestionRow {
            id: "q1".into(),
            call_id: "c1".into(),
            text: "   ".into(), // пустой текст — скип
            raised_by: None,
            evidence_quote: None,
            evidence_speaker: None,
            evidence_start_ms: None,
            order_idx: 0,
        }];
        let ps = build_structured_passages(
            &decisions,
            &items,
            &questions,
            &std::collections::HashMap::new(),
        );
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].kind, AssistantPassageKind::Decision);
        assert!(ps[0].text.contains("— цитата: давайте зафиксируем"));
        assert_eq!(ps[0].start_ms, Some(62_000));
        assert_eq!(ps[0].end_ms, Some(70_000));
        assert_eq!(ps[1].kind, AssistantPassageKind::ActionItem);
        assert_eq!(ps[1].text, "Стенд для нагрузочных");
        assert!(ps[1].end_ms.is_none());
    }

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
