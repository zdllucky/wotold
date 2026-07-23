//! [M15.12] Mini-eval harness retrieval-качества — golden QA на фикстурах
//! (`eval_fixtures/`, паттерн `pipeline/golden_eval.rs`).
//!
//! Два уровня:
//! - **A — BM25 baseline** (детерминированный, бежит в CI): регресс-защита
//!   санитизации/морфологии + явная документация слабостей лексики
//!   (`expect_bm25: "miss"` — кейсы, ради которых существует Ph2).
//! - **B — гибрид на реальной e5-модели** (`#[ignore]` + env
//!   `WOTOLD_EVAL_MODEL_DIR` с `model.onnx` + `tokenizer.json`): hit@3/5,
//!   MRR, корректность «пусто» + распределения cosine (подбор
//!   `retrieval::COSINE_SIM_FLOOR`).
//!
//! Запуск уровня B:
//! `WOTOLD_EVAL_MODEL_DIR=<dir> cargo test --features assistant-embed \
//!  -- --ignored eval_level_b --nocapture`

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use sqlx::SqlitePool;

use crate::assistant::retrieval::{self, Scope};
use crate::assistant::types::AssistantPassageKind;
use crate::db::assistant::{replace_call_passages, PassageHit, PassageInput};
use crate::db::test_support::fresh_db;

const CORPUS: &str = include_str!("eval_fixtures/fixture_corpus.json");
const CASES: &[&str] = &[
    include_str!("eval_fixtures/case_01_lexical_pilot.json"),
    include_str!("eval_fixtures/case_02_synonym_deadline.json"),
    include_str!("eval_fixtures/case_03_paraphrase_budget.json"),
    include_str!("eval_fixtures/case_04_semantic_money.json"),
    include_str!("eval_fixtures/case_05_call_scope_font.json"),
    include_str!("eval_fixtures/case_06_call_scope_other.json"),
    include_str!("eval_fixtures/case_07_structured_decision.json"),
    include_str!("eval_fixtures/case_08_open_question.json"),
    include_str!("eval_fixtures/case_09_action_layouts.json"),
    include_str!("eval_fixtures/case_10_negative_offtopic.json"),
    include_str!("eval_fixtures/case_11_negative_smalltalk.json"),
    include_str!("eval_fixtures/case_12_crosslingual.json"),
];

#[derive(Deserialize)]
struct CorpusFile {
    calls: Vec<CorpusCall>,
}

#[derive(Deserialize)]
struct CorpusCall {
    id: String,
    passages: Vec<CorpusPassage>,
}

#[derive(Deserialize)]
struct CorpusPassage {
    key: String,
    kind: String,
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    start_ms: Option<i64>,
    text: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CaseScope {
    // Строка "global" из фикстуры — значение не читается, важен вариант.
    Global(#[allow(dead_code)] String),
    Call { call: String },
}

#[derive(Deserialize)]
struct EvalCase {
    id: String,
    question: String,
    scope: CaseScope,
    k: usize,
    expect_bm25: String,
    // Читается уровнем B (cfg assistant-embed) — в default-сборке поле спит.
    #[allow(dead_code)]
    expect_hybrid: String,
    relevant_passage_keys: Vec<String>,
}

fn kind_of(s: &str) -> AssistantPassageKind {
    match s {
        "transcript" => AssistantPassageKind::Transcript,
        "recap" => AssistantPassageKind::Recap,
        "decision" => AssistantPassageKind::Decision,
        "action_item" => AssistantPassageKind::ActionItem,
        "open_question" => AssistantPassageKind::OpenQuestion,
        other => panic!("unknown passage kind in fixture: {other}"),
    }
}

fn parse_cases() -> Vec<EvalCase> {
    CASES
        .iter()
        .map(|s| serde_json::from_str::<EvalCase>(s).expect("case fixture parses"))
        .collect()
}

/// Засеять корпус → маппинг key→passage rowid (порядок вставки стабилен).
async fn seed_corpus(pool: &SqlitePool) -> HashMap<String, i64> {
    let corpus: CorpusFile = serde_json::from_str(CORPUS).expect("corpus parses");
    let mut map = HashMap::new();
    for call in &corpus.calls {
        sqlx::query(
            "INSERT INTO calls (id, started_at, duration_sec, status, path_label, created_at, updated_at)
             VALUES (?1, CURRENT_TIMESTAMP, 300, 'ready', 'managed', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(&call.id)
        .execute(pool)
        .await
        .unwrap();
        let inputs: Vec<PassageInput> = call
            .passages
            .iter()
            .map(|p| PassageInput {
                kind: kind_of(&p.kind),
                speaker: p.speaker.clone(),
                start_ms: p.start_ms,
                end_ms: p.start_ms.map(|s| s + 10_000),
                text: p.text.clone(),
                token_est: (p.text.len() / 4).max(1) as i64,
            })
            .collect();
        replace_call_passages(pool, &call.id, &inputs)
            .await
            .unwrap();
        let rows = crate::db::assistant_embeddings::list_call_passage_texts(pool, &call.id)
            .await
            .unwrap();
        assert_eq!(rows.len(), call.passages.len(), "все пассажи вставлены");
        for (row, p) in rows.iter().zip(&call.passages) {
            map.insert(p.key.clone(), row.0);
        }
    }
    map
}

fn relevant_ids(case: &EvalCase, key_map: &HashMap<String, i64>) -> HashSet<i64> {
    case.relevant_passage_keys
        .iter()
        .map(|k| *key_map.get(k).unwrap_or_else(|| panic!("unknown key {k}")))
        .collect()
}

/// Позиция (0-based) первого релевантного в top-k, если есть.
fn rank_of_first_relevant(hits: &[PassageHit], k: usize, rel: &HashSet<i64>) -> Option<usize> {
    hits.iter().take(k).position(|h| rel.contains(&h.id))
}

fn scope_of<'a>(case: &'a EvalCase) -> Scope<'a> {
    match &case.scope {
        CaseScope::Global(_) => Scope::Global,
        CaseScope::Call { call } => Scope::Call(call),
    }
}

#[tokio::test]
async fn eval_level_a_bm25_baseline() {
    let db = fresh_db().await;
    let key_map = seed_corpus(&db.pool).await;

    for case in parse_cases() {
        let hits = retrieval::search(&db.pool, &case.question, scope_of(&case))
            .await
            .unwrap();
        let rel = relevant_ids(&case, &key_map);
        let rank = rank_of_first_relevant(&hits, case.k, &rel);
        println!(
            "A {}: hits={} first_rel_rank={:?} expect={}",
            case.id,
            hits.len(),
            rank,
            case.expect_bm25
        );
        match case.expect_bm25.as_str() {
            "hit" => assert!(rank.is_some(), "{}: BM25 обязан находить", case.id),
            // «miss» — документированная слабость лексики (мотивация Ph2).
            // Если BM25 внезапно начал попадать (правка корпуса/морфологии) —
            // кейс перестал измерять гибрид, обнови фикстуру.
            "miss" => assert!(rank.is_none(), "{}: задуман как BM25-miss", case.id),
            "empty" => assert!(hits.is_empty(), "{}: ждали пустую выдачу", case.id),
            other => panic!("unknown expect_bm25 {other}"),
        }
    }
}

/// [M16.7] Уровень D — live-gate на 18 РЕАЛЬНЫХ вопросах юзера (сняты из
/// живой БД 2026-07-22, 18/20 были «нет ответа»). Прогон против копии
/// реальной БД + resident llama-server. Acceptance M16: ≥14/18 содержательных.
///
/// Запуск:
/// `WOTOLD_LIVE_DB_DIR=<dir с app.db> WOTOLD_LIVE_LLM_URL=http://127.0.0.1:47331 \
///  WOTOLD_LIVE_APPDATA=<app-data с calls/> [WOTOLD_EVAL_MODEL_DIR=<e5 dir>] \
///  cargo test -- --ignored live_gate_m16 --nocapture`
///
/// Вопросы: (текст, call-scope по титулу LIKE — None = глобальный).
const LIVE_USER_QUESTIONS: &[(&str, Option<&str>)] = &[
    ("Сколько звонков было итого", None),
    ("Что решили на звонке по командам", None),
    ("Что решили по реформированию команд в итоге?", None),
    ("Что за проект Дамир делает", None),
    ("Решения планёрки продукта", None),
    ("В чем суть изменений в итоге", Some("Реструктуризация")),
    ("о чем звонок", Some("Реструктуризация")),
    ("сколько звонков записано", None),
    ("Что должен сделать дамир", None),
    ("О чем был последний звонок", None),
    ("Что в итоге решили по ескроу", Some("Реструктуризация")),
    ("Кто такой Александр", Some("Реструктуризация")),
    ("Что по переводам", None),
    ("что по проекту", None),
    ("Что там делает Глеб?", None),
    ("Когда был последний звонок?", None),
    ("Когда обсуждали приватность?", None),
    ("Что там по делению команд", None),
];

#[tokio::test]
#[ignore = "live gate: env WOTOLD_LIVE_DB_DIR + WOTOLD_LIVE_LLM_URL + WOTOLD_LIVE_APPDATA"]
async fn live_gate_m16_user_questions() {
    use crate::assistant::{ask_core_with, AskArgs};
    use crate::events::EventBus;

    let db_dir = std::env::var("WOTOLD_LIVE_DB_DIR").expect("WOTOLD_LIVE_DB_DIR");
    let url = std::env::var("WOTOLD_LIVE_LLM_URL").expect("WOTOLD_LIVE_LLM_URL");
    let appdata = std::env::var("WOTOLD_LIVE_APPDATA").expect("WOTOLD_LIVE_APPDATA");

    let pool = crate::db::init(std::path::Path::new(&db_dir))
        .await
        .expect("init db copy");
    // Миграция 0021 вычистила индекс — восстанавливаем штатным backfill'ом
    // (артефакты читаются из реального app-data, пишем в КОПИЮ БД).
    let store = crate::call_store::CallStore::new(std::path::PathBuf::from(&appdata));
    crate::assistant::indexer::backfill(&pool, &store).await;

    // Гибрид: реальный эмбеддер если задан, вектора корпуса — backfill'ом.
    #[cfg(feature = "assistant-embed")]
    let embedder = match std::env::var("WOTOLD_EVAL_MODEL_DIR") {
        Ok(dir) => {
            let e = crate::assistant::embedder::onnx_load_from_dir(std::path::Path::new(&dir))
                .expect("embedder");
            let n = crate::assistant::indexer::embed_backfill_with(&pool, e.clone())
                .await
                .unwrap();
            println!("live: embedded {n} passages");
            Some(e)
        }
        Err(_) => None,
    };
    #[cfg(not(feature = "assistant-embed"))]
    let embedder: Option<std::sync::Arc<dyn crate::assistant::embedder::TextEmbedder>> = None;

    let preset = crate::local_engine::preset::LocalEnginePreset::Balanced;
    let provider = crate::local_engine::llm::LocalLlamaProvider::for_preset(
        std::path::Path::new(&appdata),
        preset.llm_model_id(),
    )
    .with_server(Some(url))
    .with_cache_prompt(true);
    let bus = EventBus::new(None);

    // Резолв call-scope по подстроке титула.
    async fn call_by_title(pool: &SqlitePool, like: &str) -> Option<String> {
        sqlx::query_as::<_, (String,)>(
            "SELECT id FROM calls WHERE status='ready' AND title LIKE '%' || ?1 || '%'
             ORDER BY started_at DESC LIMIT 1",
        )
        .bind(like)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|(id,)| id)
    }

    let mut meaningful = 0usize;
    for (q, scope_title) in LIVE_USER_QUESTIONS {
        let call_id = match scope_title {
            Some(t) => call_by_title(&pool, t).await,
            None => None,
        };
        let t0 = std::time::Instant::now();
        let out = ask_core_with(
            &provider,
            &pool,
            &bus,
            AskArgs {
                chat_id: None,
                call_id,
                question: (*q).to_string(),
            },
            embedder.clone(),
        )
        .await;
        match out {
            Ok(o) => {
                let text = &o.message.text;
                let is_meaningful = text != crate::assistant::answer::NO_DIRECT_ANSWER_TEXT
                    && text != crate::assistant::EMPTY_GLOBAL_TEXT
                    && !text.is_empty();
                // «Когда обсуждали приватность» — темы в корпусе НЕТ: честный
                // empty и есть правильный ответ, засчитываем.
                let honest_empty_ok = *q == "Когда обсуждали приватность?"
                    && text == crate::assistant::EMPTY_GLOBAL_TEXT;
                if is_meaningful || honest_empty_ok {
                    meaningful += 1;
                }
                let short: String = text.chars().take(140).collect();
                println!(
                    "[{}ms] {} {} → {}",
                    t0.elapsed().as_millis(),
                    if is_meaningful || honest_empty_ok {
                        "OK "
                    } else {
                        "MISS"
                    },
                    q,
                    short.replace('\n', " ")
                );
            }
            Err(e) => println!("ERR {q} → {e}"),
        }
    }
    println!(
        "live gate M16: {meaningful}/{} содержательных",
        LIVE_USER_QUESTIONS.len()
    );
    assert!(
        meaningful >= 14,
        "acceptance M16: ≥14/18, получили {meaningful}"
    );
}

/// Уровень B — реальная модель. Метрики + распределения cosine для порога.
#[cfg(feature = "assistant-embed")]
#[tokio::test]
#[ignore = "требует e5-модель: env WOTOLD_EVAL_MODEL_DIR (model.onnx + tokenizer.json)"]
async fn eval_level_b_hybrid_real_model() {
    use std::path::PathBuf;

    use crate::assistant::embed_cache::EmbedCache;
    use crate::assistant::embedder;

    let dir = PathBuf::from(
        std::env::var("WOTOLD_EVAL_MODEL_DIR").expect("WOTOLD_EVAL_MODEL_DIR не задан"),
    );
    let emb = embedder::onnx_load_from_dir(&dir).expect("модель загружается");

    let db = fresh_db().await;
    let key_map = seed_corpus(&db.pool).await;
    // Вектора корпуса — прод-путём backfill'а.
    let n = crate::assistant::indexer::embed_backfill_with(&db.pool, emb.clone())
        .await
        .unwrap();
    println!("B: embedded {n} passages");
    let cache = EmbedCache::new();

    let (mut hit3, mut hit5, mut mrr, mut n_hit_cases) = (0usize, 0usize, 0.0f64, 0usize);
    for case in parse_cases() {
        let hits = retrieval::search_hybrid(
            &db.pool,
            &case.question,
            scope_of(&case),
            Some(emb.clone()),
            &cache,
            None,
        )
        .await
        .unwrap();
        let rel = relevant_ids(&case, &key_map);
        let rank = rank_of_first_relevant(&hits, case.k, &rel);

        // Распределение cosine. Итог подбора порога (прогон 2026-07-22,
        // intfloat qint8): ГЛОБАЛЬНЫЙ cosine-floor НЕВОЗМОЖЕН — диапазоны
        // перекрываются полностью (garbage-запрос case_10 даёт top-cos
        // 0.8190, а синонимный релевант case_02 — 0.7785). e5-абсолюты не
        // калиброваны между запросами. Честное «не найдено» для гибрида
        // обеспечивает answer-слой (NO_DIRECT_ANSWER fallback, M15.7).
        let qvec = emb.embed_query(&case.question).unwrap();
        let snap = cache.snapshot(&db.pool).await.unwrap();
        let (mut best_rel, mut best_irrel) = (f32::MIN, f32::MIN);
        for r in snap.iter() {
            let dot: f32 = r.vec.iter().zip(&qvec).map(|(a, b)| a * b).sum();
            if rel.contains(&r.passage_id) {
                best_rel = best_rel.max(dot);
            } else {
                best_irrel = best_irrel.max(dot);
            }
        }
        println!(
            "B {}: hits={} first_rel_rank={:?} best_rel_cos={:.4} best_irrel_cos={:.4} expect={}",
            case.id,
            hits.len(),
            rank,
            best_rel,
            best_irrel,
            case.expect_hybrid
        );

        match case.expect_hybrid.as_str() {
            "hit" => {
                assert!(rank.is_some(), "{}: гибрид обязан находить", case.id);
                let r = rank.unwrap();
                n_hit_cases += 1;
                if r < 3 {
                    hit3 += 1;
                }
                if r < 5 {
                    hit5 += 1;
                }
                mrr += 1.0 / ((r + 1) as f64);
            }
            // Негативный кейс: релевантных пассажей нет; cosine-канал всё
            // равно принесёт кандидатов (см. коммент выше) — фиксируем это
            // поведение, честность отдаём answer-слою.
            "no_relevant" => {
                assert!(rank.is_none(), "{}: релевантных быть не должно", case.id);
                assert!(
                    !hits.is_empty(),
                    "{}: гибрид без порога отдаёт кандидатов (LLM-guard)",
                    case.id
                );
            }
            other => panic!("unknown expect_hybrid {other}"),
        }
    }
    println!(
        "B summary: hit@3 {hit3}/{n_hit_cases}, hit@5 {hit5}/{n_hit_cases}, MRR {:.3}",
        mrr / (n_hit_cases as f64)
    );
}
