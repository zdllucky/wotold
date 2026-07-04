use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    db::{
        self,
        decisions::{replace_decisions, DecisionInput},
        open_questions::{replace_open_questions, OpenQuestionInput},
        ActionItemInput,
    },
    pipeline::{
        summary_v2::{ActionItemCategory, CallSummaryV2, CallType},
        summary_validator::{
            self, strip_unverified_evidence, validate_schema, DEFAULT_FUZZY_THRESHOLD,
        },
    },
    providers::{
        llm::{AnthropicProvider, LlmProvider, LlmRequest},
        ProviderMode,
    },
    AppError,
};

/// Структурный JSON от LLM-провайдера. См. M4.2 паспорта.
/// Поля `version` и `RecapParticipant::contact_id` не читаются сейчас, но
/// сохраняем их в схеме — пригодятся когда добавим версионирование и
/// post-confirmation owner-маппинг (#26).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RecapJson {
    #[serde(default)]
    pub version: Option<u32>,
    /// [B17 V4.0] Short call title — 3-7 слов, headline-style. Сохраняется
    /// в calls.title для отображения в CallDetailPage header вместо fallback
    /// "Звонок · 20 мая".
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub key_points: Vec<String>,
    #[serde(default)]
    pub mom: String,
    #[serde(default)]
    pub action_items: Vec<RecapActionItem>,
    #[serde(default)]
    pub participants: Vec<RecapParticipant>,
}

#[derive(Debug, Deserialize)]
pub struct RecapActionItem {
    pub text: String,
    #[serde(default)]
    pub owner_hint: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RecapParticipant {
    pub speaker_tag: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub contact_id: Option<String>,
}

/// Контекст одного рекап-вызова. Собирается на стороне pipeline::run или
/// команды regenerate_recap.
pub struct RecapCtx<'a> {
    pub call_id: &'a str,
    pub call_dir: &'a Path,
    pub transcript_md: &'a str,
    pub lang_detected: Option<&'a str>,
    pub proxy_base_url: &'a str,
    pub device_id: &'a Arc<str>,
    pub provider_path: &'a str,
    pub model_override: Option<&'a str>,
    /// [M14 T-02] Engine label сохраняется в `calls.summary_engine`.
    /// `cloud-managed` для proxy path; локальный путь будет выставлять
    /// `local-qwen-{1.5b|3b|7b}` в T-04..T-10.
    pub engine_label: &'a str,
    /// [M14 T-14] Summary v2 feature flag. true → cloud_universal v2 prompt.
    /// false → legacy v1 markdown-only prompt (emergency disable).
    pub summary_v2_enabled: bool,
}

/// Генерирует recap.md и action_items по уже сохранённому transcript.md.
/// M4.2-4.4 паспорта. Вызывается:
///   - автоматически после транскрипции (chain in pipeline::run)
///   - из команды regenerate_recap (M4.5, перегенерация без re-STT)
pub async fn run(pool: &SqlitePool, ctx: RecapCtx<'_>) -> Result<(), AppError> {
    let mode = match ctx.provider_path {
        "managed" => {
            if ctx.proxy_base_url.is_empty() {
                return Err(AppError::Other(
                    "Proxy URL не настроен. Settings → Proxy URL (#22 / [B4]).".into(),
                ));
            }
            ProviderMode::Managed {
                proxy_base_url: ctx.proxy_base_url.to_string(),
                device_id: ctx.device_id.to_string(),
            }
        }
        "byo" => {
            return Err(AppError::Other(
                "BYO LLM key ещё не подключён. См. #47 в roadmap.".into(),
            ));
        }
        other => return Err(AppError::Other(format!("unknown provider_path: {other}"))),
    };

    // Собрать known speakers: подтверждённые привязки speaker_tag → contact.
    // Это даст LLM контекст «owner = Damir», «Speaker 0 = Ivan Petrov (Acme)».
    let known_speakers = build_known_speakers_block(pool, ctx.call_id).await?;

    // [M14 T-14] Branch prompt по feature flag. OFF → legacy v1 markdown-only
    // (минимальный JSON, парсится через existing promote_legacy_to_v2 fallback).
    let system_prompt = if ctx.summary_v2_enabled {
        // Cloud path: no pre-classification, LLM решает call_type сам.
        build_v2_system_prompt(ctx.lang_detected, known_speakers.as_deref(), None)
    } else {
        build_legacy_system_prompt(ctx.lang_detected, known_speakers.as_deref())
    };

    let provider = AnthropicProvider::new(mode);
    let request = LlmRequest {
        model: ctx.model_override.map(str::to_string),
        system: system_prompt,
        input: ctx.transcript_md.to_string(),
        max_tokens: Some(4096),
        grammar: None,
        json_schema: None,
    };

    let started = Instant::now();
    let json_value = provider
        .generate(request)
        .await
        .map_err(|e| AppError::Other(format!("llm: {e}")))?;
    let generation_ms = started.elapsed().as_millis() as i64;

    persist_recap_from_json(
        pool,
        ctx.call_id,
        ctx.call_dir,
        json_value,
        ctx.engine_label,
        ctx.transcript_md,
        Some(generation_ms),
        Some(ctx.summary_v2_enabled),
    )
    .await
}

/// [M12.6 Phase 3 / M14 T-02] Извлечь action_items, decisions, open_questions,
/// metadata, recap.md из готового JSON. Используется обоими LLM-путями
/// (cloud через proxy и LocalLlamaProvider).
///
/// Schema versioning (M14): пытаемся parse как `CallSummaryV2` (с
/// `schema_version: 2`). При failure — fallback на legacy `RecapJson` v1
/// плюс promote → CallSummaryV2 (без decisions/open_questions, call_type=other,
/// без evidence). Единый persist path — DB rows для decisions/open_questions
/// tables, summary metadata, расширенный recap.md.
///
/// **Validator** (M14 T-02): после parse'а вызывается
/// `strip_unverified_evidence` — items с evidence quote не verbatim в
/// transcript отбрасываются (не fail всей summary). Degraded ok с telemetry.
#[allow(clippy::too_many_arguments)] // [M14 T-14] flag_state opt-in для telemetry; refactor в structured args = backlog
pub async fn persist_recap_from_json(
    pool: &SqlitePool,
    call_id: &str,
    call_dir: &Path,
    json_value: serde_json::Value,
    engine_label: &str,
    transcript_md: &str,
    generation_ms: Option<i64>,
    flag_state: Option<bool>,
) -> Result<(), AppError> {
    // [M14 T-14] Capture original schema_version из LLM-JSON ДО promote'а —
    // telemetry хочет знать «v1 или v2 produced by LLM», не финальную DB-форму
    // (всегда v2 после promote_legacy_to_v2).
    let llm_schema_version = json_value
        .get("schema_version")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);

    let summary = parse_summary_v2_or_promote_legacy(json_value, call_id)?;
    persist_summary_v2(
        pool,
        call_id,
        call_dir,
        summary,
        engine_label,
        transcript_md,
        generation_ms,
    )
    .await?;

    // [M14 T-14] Local-only telemetry. Cloud path передаёт Some(flag); local
    // path (chunk_assembly / run_local_inner) → None пока T-04..T-10 не готов.
    if let Some(flag) = flag_state {
        if let Err(e) = crate::db::telemetry::record_summary_generation(
            pool,
            crate::db::telemetry::SummaryLogEntry {
                call_id: call_id.to_string(),
                engine: engine_label.to_string(),
                schema_version: llm_schema_version,
                flag_state: flag,
                generation_ms: generation_ms.unwrap_or(0),
            },
        )
        .await
        {
            log::warn!("recap {call_id}: telemetry log failed (non-fatal): {e}");
        }
    }
    Ok(())
}

/// [M14 T-02] Parse JSON value either as CallSummaryV2 (schema_version=2)
/// или fall back to legacy RecapJson v1 + promote to v2 envelope.
///
/// [M14 T-12] `pub(crate)` для golden_eval test access — production callers
/// без изменений.
pub(crate) fn parse_summary_v2_or_promote_legacy(
    json_value: serde_json::Value,
    call_id: &str,
) -> Result<CallSummaryV2, AppError> {
    // [M14 T-02] Try v2 first.
    match serde_json::from_value::<CallSummaryV2>(json_value.clone()) {
        Ok(s) if s.schema_version == 2 => Ok(s),
        Ok(_) | Err(_) => {
            // Fallback: parse v1 + promote.
            let legacy: RecapJson = serde_json::from_value(json_value).map_err(|e| {
                AppError::Other(format!("recap {call_id} JSON shape (v1+v2 failed): {e}"))
            })?;
            log::info!(
                "recap {call_id}: v1 legacy JSON detected, promoting to v2 envelope (no decisions/open_questions/evidence)"
            );
            Ok(promote_legacy_to_v2(legacy))
        }
    }
}

/// Конвертирует legacy RecapJson в CallSummaryV2 envelope. Опускаемые поля
/// (decisions, open_questions, evidence) — пустые/None. call_type=Other.
///
/// [M14 T-12] `pub(crate)` для golden_eval test access.
pub(crate) fn promote_legacy_to_v2(legacy: RecapJson) -> CallSummaryV2 {
    use crate::pipeline::summary_v2::{ActionItemV2, ParticipantV2};
    let action_items = legacy
        .action_items
        .into_iter()
        .enumerate()
        .map(|(i, a)| ActionItemV2 {
            id: format!("legacy-{i}"),
            text: a.text,
            owner_hint: a.owner_hint,
            owner_confidence: None,
            due: a.due,
            due_confidence: None,
            category: ActionItemCategory::Commitment,
            evidence: None,
        })
        .collect();
    let participants = legacy
        .participants
        .into_iter()
        .map(|p| ParticipantV2 {
            speaker_tag: p.speaker_tag,
            display_name: p.display_name,
            role_hint: None,
        })
        .collect();
    CallSummaryV2 {
        schema_version: 2,
        title: legacy.title,
        summary: legacy.summary,
        key_points: legacy.key_points,
        mom: legacy.mom,
        language: String::new(),
        call_type: CallType::Other,
        call_type_confidence: 0.0,
        participants,
        action_items,
        decisions: Vec::new(),
        open_questions: Vec::new(),
        topics: Vec::new(),
        narrative: String::new(),
        type_specific_block: None,
    }
}

/// [M14 T-02] Единый persist путь для CallSummaryV2 — applies validator,
/// пишет action_items + decisions + open_questions + summary metadata в DB +
/// расширенный recap.md на диск.
async fn persist_summary_v2(
    pool: &SqlitePool,
    call_id: &str,
    call_dir: &Path,
    summary: CallSummaryV2,
    engine_label: &str,
    transcript_md: &str,
    generation_ms: Option<i64>,
) -> Result<(), AppError> {
    // 1. Strip unverified evidence — drops items с фабрикованными quotes.
    let (mut summary, nulled) =
        strip_unverified_evidence(summary, transcript_md, DEFAULT_FUZZY_THRESHOLD);
    if nulled > 0 {
        log::info!("recap {call_id}: обнулено {nulled} недостоверных цитат (пункты сохранены)");
    }
    // Dedup duplicates (same intent в разных chunk'ах).
    summary_validator::dedup_items(&mut summary);
    // Schema warnings — non-fatal, log only.
    let schema_errors = validate_schema(&summary);
    if !schema_errors.is_empty() {
        log::warn!(
            "recap {call_id}: {} schema validation warnings (degraded ok)",
            schema_errors.len()
        );
    }

    // 2. Action items с v2 enrichment.
    let contacts = db::list_contacts(pool).await?;
    let action_inputs: Vec<ActionItemInput> = summary
        .action_items
        .iter()
        .map(|ai| {
            let owner_contact_id = ai
                .owner_hint
                .as_deref()
                .and_then(|hint| match_contact_id(&contacts, hint));
            ActionItemInput {
                text: ai.text.clone(),
                owner_contact_id,
                due: ai.due.clone(),
                owner_confidence: ai.owner_confidence.map(f64::from),
                due_confidence: ai.due_confidence.map(f64::from),
                category: Some(ai.category.as_str().to_string()),
                evidence_quote: ai.evidence.as_ref().map(|e| e.quote.clone()),
                evidence_speaker: ai.evidence.as_ref().and_then(|e| e.speaker.clone()),
                evidence_start_ms: ai.evidence.as_ref().and_then(|e| e.start_ms),
            }
        })
        .collect();

    // [recap-blank guard] Рендерим recap.md заранее и проверяем на пустоту ДО
    // любых DB-записей. Слабая локальная модель иногда возвращает summary со
    // всеми пустыми полями → header-only recap («# Рекап\n\n»). Раньше такое
    // молча персистилось как успех → пустой рекап «без контекста» и без
    // recap_failed_reason (юзер не понимает что пошло не так). Теперь — ранний
    // Err: caller выставит recap_failed_reason, UI покажет retry-баннер вместо
    // пустышки. Idempotent — на успешном retry replace_* перезапишет.
    // (Bulk-команда regenerate_empty_recaps остаётся для уборки старых пустых.)
    let md = render_recap_md_v2(&summary, &contacts, &action_inputs);
    if recap_md_is_blank(&md) {
        log::warn!(
            "recap {call_id}: LLM вернул пустое саммари (все секции empty) — не персистим, возвращаем Err"
        );
        return Err(AppError::Other("recap_blank_llm_output".into()));
    }

    db::replace_action_items(pool, call_id, &action_inputs).await?;

    // 3. Decisions table.
    let decision_inputs: Vec<DecisionInput> = summary
        .decisions
        .iter()
        .map(|d| DecisionInput {
            text: d.text.clone(),
            evidence_quote: d.evidence.as_ref().map(|e| e.quote.clone()),
            evidence_speaker: d.evidence.as_ref().and_then(|e| e.speaker.clone()),
            evidence_start_ms: d.evidence.as_ref().and_then(|e| e.start_ms),
            evidence_end_ms: d.evidence.as_ref().and_then(|e| e.end_ms),
            confidence: d.confidence.map(f64::from),
        })
        .collect();
    replace_decisions(pool, call_id, &decision_inputs).await?;

    // 4. Open questions table.
    let oq_inputs: Vec<OpenQuestionInput> = summary
        .open_questions
        .iter()
        .map(|q| OpenQuestionInput {
            text: q.text.clone(),
            raised_by: q.raised_by.clone(),
            evidence_quote: q.evidence.as_ref().map(|e| e.quote.clone()),
            evidence_speaker: q.evidence.as_ref().and_then(|e| e.speaker.clone()),
            evidence_start_ms: q.evidence.as_ref().and_then(|e| e.start_ms),
        })
        .collect();
    replace_open_questions(pool, call_id, &oq_inputs).await?;

    // 5. calls.title.
    if !summary.title.trim().is_empty() {
        db::set_call_title(pool, call_id, &summary.title).await?;
    }

    // 6. calls metadata (engine, schema_version, call_type, generation_ms, …).
    let type_specific_block_json: Option<String> = summary
        .type_specific_block
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());
    db::set_summary_metadata(
        pool,
        call_id,
        db::SummaryMetadata {
            engine: engine_label,
            schema_version: 2,
            call_type: Some(summary.call_type.as_str()),
            call_type_confidence: Some(summary.call_type_confidence),
            pipeline_mode: "one_shot",
            generation_ms,
            input_tokens: None,
            output_tokens: None,
            type_specific_block_json: type_specific_block_json.as_deref(),
        },
    )
    .await?;

    // 7. Extended recap.md — `md` уже отрендерен и провалидирован на пустоту
    // выше (recap-blank guard), просто пишем на диск.
    tokio::fs::write(call_dir.join("recap.md"), md).await?;

    Ok(())
}

fn match_contact_id(contacts: &[db::Contact], hint: &str) -> Option<String> {
    let hint_lower = hint.trim().to_lowercase();
    if hint_lower.is_empty() {
        return None;
    }
    // Точное совпадение имени важнее частичного.
    if let Some(c) = contacts
        .iter()
        .find(|c| c.display_name.to_lowercase() == hint_lower)
    {
        return Some(c.id.clone());
    }
    contacts
        .iter()
        .find(|c| {
            let dn = c.display_name.to_lowercase();
            dn.contains(&hint_lower) || hint_lower.contains(&dn)
        })
        .map(|c| c.id.clone())
}

/// [M14 T-14] Legacy v1 markdown-only prompt — fallback когда
/// `summary_v2_enabled=false`. Минимальный JSON: title/summary/key_points/
/// tasks/participants/lang. Без decisions/open_questions/evidence/call_type.
/// Парсится через existing `promote_legacy_to_v2` envelope.
pub(crate) fn build_legacy_system_prompt(
    lang_detected: Option<&str>,
    known_speakers: Option<&str>,
) -> String {
    let lang = lang_detected.unwrap_or("ru");
    let known_block = known_speakers
        .map(|s| format!("\n\n## Known participants\n{s}"))
        .unwrap_or_default();
    format!(
        "You are a senior meeting analyst for Wotold. Produce a faithful JSON summary of a meeting transcript. Output language: {lang}.\n\
\n\
## RULES\n\
1. NEVER invent facts, names, dates, numbers, or commitments not in transcript.\n\
2. Action items: assign owner only on explicit acceptance ('я возьму', 'I'll take it'). Mere mention NOT enough.\n\
3. Output ONLY ONE JSON object matching schema below. No prose, no markdown fences.\n\
4. NEVER use 'Speaker 0' / 'Спикер 1' in summary/key_points — resolve via known participants OR self-introduction OR generic role ('клиент', 'collega').\n\
\n\
## SCHEMA\n\
\n\
{{\n\
  \"schema_version\": 1,\n\
  \"title\": string,                              // 3-7 слов, headline-style\n\
  \"summary\": string,                            // 1-2 предложения TL;DR\n\
  \"key_points\": string[],                       // 3-7 пунктов конкретики\n\
  \"language\": \"ru\" | \"en\" | \"kk\" | \"mixed\",\n\
  \"participants\": [{{ \"speaker_tag\": string, \"display_name\": string|null }}],\n\
  \"action_items\": [{{\n\
    \"text\": string,\n\
    \"owner_hint\": string|null,\n\
    \"due\": string|null\n\
  }}]\n\
}}{known_block}\n\
\n\
Output ONLY the JSON object. No prose. No markdown fences."
    )
}

pub(crate) fn build_v2_system_prompt(
    lang_detected: Option<&str>,
    known_speakers: Option<&str>,
    known_call_type: Option<crate::pipeline::summary_v2::CallType>,
) -> String {
    let lang = lang_detected.unwrap_or("ru");
    let known_block = known_speakers
        .map(|s| format!("\n\n## Known participants\n{s}"))
        .unwrap_or_default();
    // [M14 T-04] Optional classification hint от лёгкого pre-pass.
    // LLM не классифицирует заново. Cloud callers pass None (full reasoning).
    let type_hint = known_call_type
        .map(|t| {
            format!(
                "\n\n## Classification hint (pre-determined)\nCall type already classified as `{}`. Set `call_type` to this value.",
                t.as_str()
            )
        })
        .unwrap_or_default();

    // [M14 T-02] V2 cloud_universal prompt — type-driven evidence-grounded
    // structured output. PRD §5.1. Cloud путь (Groq Llama 3.3 70B + Anthropic
    // backup) выдаёт CallSummaryV2 schema; backend парсит + validator drops
    // unverified evidence + persist в DB.
    format!(
        "OUTPUT LANGUAGE = {lang}. EVERY string value (title, summary, key_points, decisions/action_items/open_questions text, topics) MUST be written in {lang}. Only enum values (call_type, category) stay English.\n\
\n\
You are a senior meeting analyst for Wotold, a corporate call recording tool. Your job: produce a faithful, evidence-grounded, COMPLETE JSON summary of a meeting transcript.\n\
\n\
## ABSOLUTE RULES (violations are bugs)\n\
\n\
1. NEVER invent facts, names, dates, numbers, or commitments not present in the transcript.\n\
2. Capture EVERY real decision, action item, and open question raised in the call — aim for COMPLETENESS (typical business call has several of each). Leave an array empty ONLY if the call genuinely had none. For each item add `evidence.quote` = a verbatim substring (10-200 chars) from the transcript WHEN you can copy one; if you cannot, set `evidence.quote` to null but ALWAYS KEEP the item (never drop a real point just because you lack a verbatim quote).\n\
3. Owner attribution: only assign an owner if the transcript shows them explicitly accepting the task ('I'll do it', 'я возьму', 'I will take that'). Mere mention of a name is NOT enough. Set `owner_confidence`: 0.9+ only for explicit accept; 0.5 for inferred; 0.0 if no owner.\n\
4. Categorize each action_item:\n\
   - `commitment` — explicit accept ('я сделаю', 'I'll send it')\n\
   - `proposal` — suggested но не accepted\n\
   - `idea` — raised, no clear action\n\
5. Output ONLY ONE JSON object matching the schema. No prose, no markdown fences, no explanation.\n\
6. NEVER use raw 'Speaker 0', 'Speaker 1', 'owner' tags inside `summary`/`key_points`/`action_items.text`. Resolve to names via:\n\
   (a) Known participants block — exact name.\n\
   (b) Self-introduction in transcript.\n\
   (c) Generic role: 'клиент', 'представитель вендора', 'коллега'. NEVER 'Спикер 1'.\n\
\n\
## OUTPUT SCHEMA (strict)\n\
\n\
{{\n\
  \"schema_version\": 2,\n\
  \"title\": string,                              // 3-7 слов, headline-style. Конкретика, без 'Звонок про'. Пример: 'Лонч в августе — Марина'.\n\
  \"summary\": string,                            // 3-5 предложений: о чём встреча, главные итоги, контекст. НЕ одна фраза.\n\
  \"key_points\": string[],                       // 5-10 конкретных пунктов с цифрами/датами/именами/решениями. Не общие фразы.\n\
  \"language\": \"ru\" | \"en\" | \"kk\" | \"mixed\",\n\
  \"call_type\": one of:\n\
    'sales_discovery' | 'sales_demo' | 'product_sync' | 'standup' |\n\
    'customer_interview' | 'one_on_one' | 'strategy_brainstorm' | 'status_update' | 'other'\n\
  \"call_type_confidence\": number (0..1),\n\
  \"participants\": [{{ \"speaker_tag\": string, \"display_name\": string|null, \"role_hint\": string|null }}],\n\
  \"action_items\": [{{\n\
    \"id\": string (короткий unique slug),\n\
    \"text\": string (инфинитив, без префикса '<кто> —'),\n\
    \"owner_hint\": string|null,\n\
    \"owner_confidence\": number (0..1),\n\
    \"due\": string|null (ISO date YYYY-MM-DD или человеческое 'к концу недели'),\n\
    \"due_confidence\": number (0..1),\n\
    \"category\": \"commitment\" | \"proposal\" | \"idea\",\n\
    \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }}\n\
  }}],\n\
  \"decisions\": [{{\n\
    \"id\": string,\n\
    \"text\": string,                            // Чёткое решение принятое в звонке\n\
    \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }},\n\
    \"confidence\": number (0..1)\n\
  }}],\n\
  \"open_questions\": [{{\n\
    \"id\": string,\n\
    \"text\": string,                            // Нерешённый вопрос поднятый в звонке\n\
    \"raised_by\": string|null,\n\
    \"evidence\": {{ \"quote\": string|null, \"speaker\": string|null }}\n\
  }}],\n\
  \"topics\": [{{ \"title\": string, \"points\": string[] }}]  // 2-5 обсуждённых тем, у каждой 1-4 конкретных под-пункта\n\
}}\n\
\n\
## PRIVACY (one_on_one)\n\
\n\
- Если call_type = one_on_one: НЕ включай дословный личный фидбэк в `evidence.quote` —\n\
  перефразируй + evidence=null. action_items ТОЛЬКО рабочие commitments.\n\
\n\
## LANGUAGE & FORMATTING\n\
\n\
- Detect dominant language of transcript. Output ALL string fields (title, summary, key_points, action_items.text, decisions.text, open_questions.text) в этом языке.\n\
- `call_type` и `category` enum values остаются английскими (snake_case).\n\
- Mixed ru/en → respond в dominant + English tech terms as-is.\n\
\n\
## EVIDENCE QUOTE RULES\n\
\n\
- Verbatim substring of transcript. Preserve original language + casing + punctuation.\n\
- 10-200 characters length.\n\
- `evidence.speaker` отдельно (raw speaker_tag from transcript).\n\
- Если нет верifiable anchor → `evidence.quote = null` (backend drop'нет item).\n\
\n\
## EDGE CASES\n\
\n\
- Короткий транскрипт (<5 реплик) или пустой → `summary` = 'Запись не содержит обсуждения по существу.' + empty arrays + call_type=other.\n\
- Если transcript на kk: используй kazakh terms, keep technical English as-is.{known_block}{type_hint}",
    )
}

/// Собирает «Known participants» блок для LLM-контекста: для каждой
/// подтверждённой привязки speaker_tag → contact выводит строку с display_name
/// + опц. org/role. Если привязок нет — None (блок не добавляется).
pub(crate) async fn build_known_speakers_block(
    pool: &SqlitePool,
    call_id: &str,
) -> Result<Option<String>, AppError> {
    let speakers = db::list_call_speakers(pool, call_id).await?;
    let confirmed: Vec<_> = speakers
        .iter()
        .filter(|s| s.confirmed && s.contact_id.is_some() && s.contact_display_name.is_some())
        .collect();
    if confirmed.is_empty() {
        return Ok(None);
    }

    // Подтянем дополнительный контекст (org/role) из contacts table.
    let contacts = db::list_contacts(pool).await?;
    let by_id: std::collections::HashMap<&str, &db::Contact> =
        contacts.iter().map(|c| (c.id.as_str(), c)).collect();

    let mut lines = Vec::new();
    for s in confirmed {
        let cid = s.contact_id.as_deref().unwrap_or("");
        let name = s.contact_display_name.as_deref().unwrap_or("");
        let extras = by_id
            .get(cid)
            .map(|c| {
                let mut bits = Vec::new();
                if let Some(role) = c.role.as_deref().filter(|s| !s.is_empty()) {
                    bits.push(role.to_string());
                }
                if let Some(org) = c.org.as_deref().filter(|s| !s.is_empty()) {
                    bits.push(org.to_string());
                }
                if bits.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", bits.join(", "))
                }
            })
            .unwrap_or_default();
        lines.push(format!("- {} = {}{}", s.speaker_tag, name, extras));
    }
    Ok(Some(lines.join("\n")))
}

/// [M14 T-02] Расширенный render для CallSummaryV2 — добавляет
/// ## Решения / Decisions + ## Открытые вопросы / Open questions секции
/// + category badges + evidence quotes как blockquotes.
///
/// Localization: labels подбираются по `summary.language` (ru/en/kk → ru/en/kk
/// localized; иначе ru fallback).
/// Семантически-пустой recap.md: только heading-строки (`# …`) и пробелы, без
/// тела. v2 render всегда даёт «# Рекап\n\n», поэтому до-фиксный пустой рекап =
/// `"# Рекап\n\n"` (строка непустая). Используется bulk-регеном чтобы найти
/// звонки требующие пересоздания. Mirror TS `isMarkdownBlank`.
pub(crate) fn recap_md_is_blank(md: &str) -> bool {
    !md.lines().any(|line| {
        let t = line.trim();
        !t.is_empty() && !is_md_heading(t)
    })
}

/// [recap-rich] Плейсхолдер вместо имени от слабой модели — не рендерим.
fn is_placeholder_name(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "unknown" | "null" | "none" | "n/a" | "не указано" | "неизвестно" | "белгісіз"
    )
}

fn is_md_heading(trimmed: &str) -> bool {
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes)
        && trimmed[hashes..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
}

fn render_recap_md_v2(
    summary: &CallSummaryV2,
    contacts: &[db::Contact],
    action_inputs: &[ActionItemInput],
) -> String {
    let labels = RecapLabels::for_lang(&summary.language);
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", labels.title));

    // [recap-rich] Вверху — нарратив-минутки (prose) если есть; иначе короткий
    // summary. Оба сразу не рендерим (нарратив уже включает суть).
    let lead = if !summary.narrative.trim().is_empty() {
        summary.narrative.trim()
    } else {
        summary.summary.trim()
    };
    if !lead.is_empty() {
        out.push_str(lead);
        out.push_str("\n\n");
    }

    if !summary.key_points.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.key_points));
        for kp in &summary.key_points {
            out.push_str(&format!("- {}\n", kp.trim()));
        }
        out.push('\n');
    }

    // [recap-rich] Темы — обсуждённые темы с под-пунктами.
    if !summary.topics.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.topics));
        for t in &summary.topics {
            if t.title.trim().is_empty() {
                continue;
            }
            out.push_str(&format!("### {}\n", t.title.trim()));
            for p in &t.points {
                if !p.trim().is_empty() {
                    out.push_str(&format!("- {}\n", p.trim()));
                }
            }
            out.push('\n');
        }
    }

    // [M14 T-02] Decisions section.
    if !summary.decisions.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.decisions));
        for d in &summary.decisions {
            out.push_str(&format!("- {}\n", d.text.trim()));
            if let Some(ev) = d.evidence.as_ref() {
                if !ev.quote.trim().is_empty() {
                    out.push_str(&format!("  > {}\n", ev.quote.trim()));
                }
            }
        }
        out.push('\n');
    }

    // [M14 T-02] Open questions section.
    if !summary.open_questions.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.open_questions));
        for q in &summary.open_questions {
            // [recap-rich] Слабая модель кладёт плейсхолдеры в raised_by
            // («unknown» / «не указано») — не печатаем такой суффикс.
            let by_suffix = q
                .raised_by
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty() && !is_placeholder_name(b))
                .map(|b| format!(" ({b})"))
                .unwrap_or_default();
            out.push_str(&format!("- {}{}\n", q.text.trim(), by_suffix));
            if let Some(ev) = q.evidence.as_ref() {
                if !ev.quote.trim().is_empty() {
                    out.push_str(&format!("  > {}\n", ev.quote.trim()));
                }
            }
        }
        out.push('\n');
    }

    // [MoM cleanup] `mom` НЕ рендерим: слабая локальная модель эхо-копировала
    // сюда инструкции промпта (## Status by workstream + type_specific_block
    // schema + raw JSON) → мусор в рекапе. Промпты больше не просят mom/tsb.

    if !action_inputs.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.tasks));
        for (i, ai) in action_inputs.iter().enumerate() {
            let owner_label = ai
                .owner_contact_id
                .as_deref()
                .and_then(|id| contacts.iter().find(|c| c.id == id))
                .map(|c| c.display_name.clone())
                .or_else(|| {
                    summary
                        .action_items
                        .get(i)
                        .and_then(|r| r.owner_hint.clone())
                });
            let due_suffix = ai
                .due
                .as_deref()
                .map(|d| format!(" — {} {d}", labels.until))
                .unwrap_or_default();
            let category_prefix = ai
                .category
                .as_deref()
                .map(|c| match c {
                    "commitment" => "✅ ",
                    "proposal" => "💡 ",
                    "idea" => "📝 ",
                    _ => "",
                })
                .unwrap_or("");
            match owner_label {
                Some(label) if !label.trim().is_empty() => {
                    out.push_str(&format!(
                        "- [ ] {}**{}** — {}{}\n",
                        category_prefix,
                        label.trim(),
                        ai.text.trim(),
                        due_suffix
                    ));
                }
                _ => {
                    out.push_str(&format!(
                        "- [ ] {}{}{}\n",
                        category_prefix,
                        ai.text.trim(),
                        due_suffix
                    ));
                }
            }
            if let Some(ev) = ai.evidence_quote.as_deref() {
                if !ev.trim().is_empty() {
                    out.push_str(&format!("  > {}\n", ev.trim()));
                }
            }
        }
        out.push('\n');
    }

    if !summary.participants.is_empty() {
        out.push_str(&format!("## {}\n\n", labels.participants));
        for p in &summary.participants {
            let name = p
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            match name {
                Some(n) if n != p.speaker_tag => {
                    out.push_str(&format!("- {} (`{}`)\n", n, p.speaker_tag));
                }
                _ => {
                    out.push_str(&format!("- `{}`\n", p.speaker_tag));
                }
            }
        }
        out.push('\n');
    }

    out
}

/// Локализованные labels для секций recap.md. Lang detection из summary.language.
struct RecapLabels {
    title: &'static str,
    key_points: &'static str,
    topics: &'static str,
    decisions: &'static str,
    open_questions: &'static str,
    tasks: &'static str,
    participants: &'static str,
    until: &'static str,
}

impl RecapLabels {
    fn for_lang(lang: &str) -> Self {
        match lang {
            "en" => Self {
                title: "Recap",
                key_points: "Key points",
                topics: "Topics",
                decisions: "Decisions",
                open_questions: "Open questions",
                tasks: "Tasks",
                participants: "Participants",
                until: "by",
            },
            "kk" => Self {
                title: "Қорытынды",
                key_points: "Негізгі тармақтар",
                topics: "Тақырыптар",
                decisions: "Шешімдер",
                open_questions: "Ашық сұрақтар",
                tasks: "Тапсырмалар",
                participants: "Қатысушылар",
                until: "мерзім:",
            },
            _ => Self {
                title: "Рекап",
                key_points: "Ключевое",
                topics: "Темы",
                decisions: "Решения",
                open_questions: "Открытые вопросы",
                tasks: "Задачи",
                participants: "Участники",
                until: "до",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// [Phase 3] recap::run должен корректно отрабатывать happy-error path:
    /// если provider_path неизвестный — Err до любых I/O. Это покрывает
    /// контракт «recap не пробрасывает успех на мусорных настройках».
    #[tokio::test]
    async fn recap_run_unknown_provider_path_returns_error() {
        let db = crate::db::test_support::fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device: std::sync::Arc<str> = std::sync::Arc::from("dev-1");
        let ctx = RecapCtx {
            call_id: "c1",
            call_dir: tmpdir.path(),
            transcript_md: "# transcript\n\nspeaker: hello",
            lang_detected: Some("en"),
            proxy_base_url: "https://example.com",
            device_id: &device,
            provider_path: "ghost-path",
            model_override: None,
            engine_label: "test-engine",
            summary_v2_enabled: true,
        };
        let err = super::run(&db.pool, ctx).await.unwrap_err();
        assert!(
            err.to_string().contains("unknown provider_path"),
            "got: {err}"
        );
    }

    /// [Phase 3] recap::run в managed-режиме с пустым proxy_base_url должен
    /// падать с UX-readable ошибкой («Proxy URL не настроен»). Покрывает
    /// edge case настройки прокси.
    #[tokio::test]
    async fn recap_run_managed_empty_proxy_url_returns_error() {
        let db = crate::db::test_support::fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device: std::sync::Arc<str> = std::sync::Arc::from("dev-1");
        let ctx = RecapCtx {
            call_id: "c1",
            call_dir: tmpdir.path(),
            transcript_md: "stub",
            lang_detected: None,
            proxy_base_url: "",
            device_id: &device,
            provider_path: "managed",
            model_override: None,
            engine_label: "test-engine",
            summary_v2_enabled: true,
        };
        let err = super::run(&db.pool, ctx).await.unwrap_err();
        assert!(err.to_string().contains("Proxy URL"), "got: {err}");
    }

    /// [M14 T-14] Legacy prompt должен ссылаться на schema_version 1 и
    /// НЕ упоминать decisions/open_questions/evidence/call_type — иначе LLM
    /// продолжит выдавать v2 при выключенном флаге.
    #[test]
    fn legacy_system_prompt_targets_schema_v1_only() {
        let prompt = build_legacy_system_prompt(Some("ru"), None);
        assert!(prompt.contains("\"schema_version\": 1"));
        assert!(!prompt.contains("decisions"));
        assert!(!prompt.contains("open_questions"));
        assert!(!prompt.contains("evidence"));
        assert!(!prompt.contains("call_type"));
        assert!(prompt.contains("action_items"));
        assert!(prompt.contains("title"));
        assert!(prompt.contains("summary"));
        assert!(prompt.contains("key_points"));
    }

    /// [M14 T-14] V2 prompt — sanity: содержит ключи которых нет в legacy
    /// (call_type, decisions, open_questions, evidence). Защищает от
    /// случайной деградации v2 prompt при future edits.
    #[test]
    fn v2_system_prompt_includes_full_schema() {
        let prompt = build_v2_system_prompt(Some("ru"), None, None);
        assert!(prompt.contains("\"schema_version\": 2"));
        assert!(prompt.contains("call_type"));
        assert!(prompt.contains("decisions"));
        assert!(prompt.contains("open_questions"));
        assert!(prompt.contains("evidence"));
    }

    /// [M14 T-14] persist_recap_from_json с flag_state=Some(true) +
    /// LLM выдал v2 JSON → telemetry row appears с schema_version=2,
    /// flag_state=1.
    #[tokio::test]
    async fn persist_emits_telemetry_when_flag_present() {
        let db = crate::db::test_support::fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let call = crate::db::insert_recording(&db.pool, "managed")
            .await
            .unwrap();
        // Минимальный v2 JSON — schema_version=2, без decisions/open_questions.
        let json_value = serde_json::json!({
            "schema_version": 2,
            "title": "test call",
            "summary": "stub",
            "key_points": ["a"],
            "language": "en",
            "call_type": "other",
            "call_type_confidence": 0.5,
            "participants": [],
            "action_items": [],
            "decisions": [],
            "open_questions": [],
            "mom": "## stub",
        });
        persist_recap_from_json(
            &db.pool,
            &call.id,
            tmpdir.path(),
            json_value,
            "cloud-managed",
            "transcript stub",
            Some(1234),
            Some(true),
        )
        .await
        .unwrap();
        let (v1, v2) = crate::db::telemetry::count_by_schema_version(&db.pool)
            .await
            .unwrap();
        assert_eq!(v1, 0);
        assert_eq!(v2, 1);
    }

    /// [M14 T-14] flag_state=None (local path до T-04..T-10) → telemetry
    /// НЕ emit'ится, persist всё равно успешен.
    #[tokio::test]
    async fn persist_skips_telemetry_when_flag_none() {
        let db = crate::db::test_support::fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let call = crate::db::insert_recording(&db.pool, "managed")
            .await
            .unwrap();
        let json_value = serde_json::json!({
            "schema_version": 2,
            "title": "no telemetry",
            "summary": "stub",
            "key_points": [],
            "language": "en",
            "call_type": "other",
            "call_type_confidence": 0.0,
            "participants": [],
            "action_items": [],
            "decisions": [],
            "open_questions": [],
            "mom": "",
        });
        persist_recap_from_json(
            &db.pool,
            &call.id,
            tmpdir.path(),
            json_value,
            "local-qwen-1.5b",
            "stub",
            None,
            None,
        )
        .await
        .unwrap();
        let (v1, v2) = crate::db::telemetry::count_by_schema_version(&db.pool)
            .await
            .unwrap();
        assert_eq!(v1, 0);
        assert_eq!(v2, 0);
    }

    /// [Phase 3] regenerate_recap при отсутствии transcript.md → AppError.
    /// Хотя `recap::run` сам не читает файл (caller это делает), мы покрываем
    /// контракт через pipeline-level wrapper, чтобы поймать regression если
    /// recap внезапно станет читать transcript сам.
    #[tokio::test]
    async fn regenerate_recap_missing_transcript_md_returns_error() {
        let db = crate::db::test_support::fresh_db().await;
        let tmpdir = tempfile::tempdir().unwrap();
        let device: std::sync::Arc<str> = std::sync::Arc::from("dev-1");
        // Создаём call row, но НЕ создаём transcript.md.
        let call = crate::db::insert_recording(&db.pool, "managed")
            .await
            .unwrap();
        let err =
            crate::pipeline::regenerate_recap(&db.pool, tmpdir.path(), &device, &call.id, None)
                .await
                .unwrap_err();
        assert!(
            err.to_string().contains("transcript.md"),
            "expected transcript.md error, got: {err}"
        );
    }

    fn contact(id: &str, name: &str) -> db::Contact {
        db::Contact {
            id: id.to_string(),
            display_name: name.to_string(),
            is_owner: false,
            org: None,
            role: None,
            attributes: Value::Object(serde_json::Map::new()),
            notes: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            identifiers: vec![],
        }
    }

    #[test]
    fn match_contact_id_exact_wins_over_partial() {
        let contacts = vec![contact("a", "Alice"), contact("b", "Alice Smith")];
        assert_eq!(match_contact_id(&contacts, "alice").as_deref(), Some("a"));
    }

    #[test]
    fn match_contact_id_partial_handles_substring() {
        let contacts = vec![contact("b", "Bob Johnson")];
        assert_eq!(match_contact_id(&contacts, "bob").as_deref(), Some("b"));
        assert_eq!(
            match_contact_id(&contacts, "Bob Johnson Jr").as_deref(),
            Some("b")
        );
        assert_eq!(match_contact_id(&contacts, "Carol"), None);
    }

    /// [M14 T-02] Helper для построения minimal CallSummaryV2 в tests.
    fn empty_summary_v2(lang: &str) -> CallSummaryV2 {
        CallSummaryV2 {
            schema_version: 2,
            title: String::new(),
            summary: String::new(),
            key_points: vec![],
            mom: String::new(),
            language: lang.into(),
            call_type: CallType::Other,
            call_type_confidence: 0.0,
            participants: vec![],
            action_items: vec![],
            decisions: vec![],
            open_questions: vec![],
            topics: Vec::new(),
            narrative: String::new(),
            type_specific_block: None,
        }
    }

    #[test]
    fn render_recap_md_v2_skips_empty_sections() {
        let mut s = empty_summary_v2("ru");
        s.summary = "Brief".into();
        let md = render_recap_md_v2(&s, &[], &[]);
        assert!(md.contains("# Рекап"));
        assert!(md.contains("Brief"));
        assert!(!md.contains("## Ключевое"));
        assert!(!md.contains("## Решения"));
        assert!(!md.contains("## Открытые вопросы"));
        assert!(!md.contains("## Задачи"));
    }

    #[test]
    fn recap_md_is_blank_detects_header_only() {
        // Старый до-фиксный пустой рекап = «# Рекап\n\n».
        assert!(recap_md_is_blank("# Рекап\n\n"));
        assert!(recap_md_is_blank("# Рекап"));
        assert!(recap_md_is_blank("  \n## A\n\n"));
        assert!(recap_md_is_blank(""));
        // С телом — не blank.
        assert!(!recap_md_is_blank("# Рекап\n\nКоманда обсудила релиз."));
        assert!(!recap_md_is_blank("# Рекап\n\n## Ключевое\n- пункт"));
        // render пустого summary с одним полем → не blank.
        let mut s = empty_summary_v2("ru");
        s.summary = "Brief".into();
        assert!(!recap_md_is_blank(&render_recap_md_v2(&s, &[], &[])));
        // render полностью пустого summary → blank (как старые звонки).
        let empty = empty_summary_v2("ru");
        assert!(recap_md_is_blank(&render_recap_md_v2(&empty, &[], &[])));
    }

    #[test]
    fn render_recap_md_v2_does_not_render_mom() {
        // [MoM cleanup] Даже если модель положила мусор в mom (эхо схемы) —
        // он НЕ попадает в recap.md.
        let mut s = empty_summary_v2("ru");
        s.summary = "Краткое содержание.".into();
        s.mom =
            "## Status by workstream / ## Risks / ## Asks\n\n### type_specific_block schema\n{\"workstreams\":[]}"
                .into();
        let md = render_recap_md_v2(&s, &[], &[]);
        assert!(md.contains("Краткое содержание."));
        assert!(!md.contains("Status by workstream"));
        assert!(!md.contains("type_specific_block"));
        assert!(!md.contains("workstreams"));
    }

    #[test]
    fn render_recap_md_v2_renders_action_items_with_owner_label() {
        let contacts = vec![contact("a", "Alice")];
        let mut s = empty_summary_v2("ru");
        s.title = "Q3 plan review".into();
        s.summary = "Discussed Q3.".into();
        s.key_points = vec!["plan reviewed".into()];
        s.action_items = vec![crate::pipeline::summary_v2::ActionItemV2 {
            id: "ai-1".into(),
            text: "send draft".into(),
            owner_hint: Some("Alice".into()),
            owner_confidence: Some(0.95),
            due: Some("2026-06-01".into()),
            due_confidence: Some(0.8),
            category: ActionItemCategory::Commitment,
            evidence: None,
        }];
        s.participants = vec![crate::pipeline::summary_v2::ParticipantV2 {
            speaker_tag: "Speaker 0".into(),
            display_name: Some("Alice".into()),
            role_hint: None,
        }];
        let action_inputs = vec![ActionItemInput {
            text: "send draft".into(),
            owner_contact_id: Some("a".into()),
            due: Some("2026-06-01".into()),
            category: Some("commitment".into()),
            ..Default::default()
        }];
        let md = render_recap_md_v2(&s, &contacts, &action_inputs);
        assert!(md.contains("## Задачи"));
        // [M14 T-02] category prefix "✅" prefix'ит owner label.
        assert!(md.contains("✅ **Alice** — send draft — до 2026-06-01"));
        assert!(md.contains("## Участники"));
        assert!(md.contains("Alice (`Speaker 0`)"));
    }

    #[test]
    fn render_recap_md_v2_includes_decisions_and_open_questions_sections() {
        let mut s = empty_summary_v2("ru");
        s.summary = "Brief".into();
        s.decisions = vec![crate::pipeline::summary_v2::Decision {
            id: "d1".into(),
            text: "Lock enterprise tier at $499".into(),
            evidence: Some(crate::pipeline::summary_v2::EvidenceAnchor {
                quote: "we agreed on 499 dollars".into(),
                speaker: Some("Alice".into()),
                start_ms: None,
                end_ms: None,
            }),
            confidence: Some(0.9),
        }];
        s.open_questions = vec![crate::pipeline::summary_v2::OpenQuestion {
            id: "q1".into(),
            text: "Should we offer a trial?".into(),
            raised_by: Some("Bob".into()),
            evidence: None,
        }];
        let md = render_recap_md_v2(&s, &[], &[]);
        assert!(md.contains("## Решения"));
        assert!(md.contains("Lock enterprise tier at $499"));
        assert!(md.contains("> we agreed on 499 dollars"));
        assert!(md.contains("## Открытые вопросы"));
        assert!(md.contains("Should we offer a trial? (Bob)"));
    }

    #[test]
    fn render_recap_md_v2_language_en_uses_english_labels() {
        let mut s = empty_summary_v2("en");
        s.summary = "Brief".into();
        s.key_points = vec!["a".into(), "b".into(), "c".into()];
        let md = render_recap_md_v2(&s, &[], &[]);
        assert!(md.contains("# Recap"));
        assert!(md.contains("## Key points"));
    }

    #[test]
    fn promote_legacy_to_v2_produces_other_call_type() {
        let legacy = RecapJson {
            version: Some(1),
            title: "Sync".into(),
            summary: "x".into(),
            key_points: vec!["a".into()],
            mom: "## A".into(),
            action_items: vec![RecapActionItem {
                text: "do thing".into(),
                owner_hint: Some("Alice".into()),
                due: None,
            }],
            participants: vec![RecapParticipant {
                speaker_tag: "Speaker 0".into(),
                display_name: Some("Alice".into()),
                contact_id: None,
            }],
        };
        let v2 = promote_legacy_to_v2(legacy);
        assert_eq!(v2.schema_version, 2);
        assert_eq!(v2.call_type, CallType::Other);
        assert_eq!(v2.action_items.len(), 1);
        assert_eq!(v2.action_items[0].category, ActionItemCategory::Commitment);
        assert!(v2.decisions.is_empty());
        assert!(v2.open_questions.is_empty());
        // Title/summary/key_points/mom преобразуются 1-в-1.
        assert_eq!(v2.title, "Sync");
    }

    #[tokio::test]
    async fn parse_summary_v2_succeeds_on_v2_json() {
        let json = serde_json::json!({
            "schema_version": 2,
            "title": "Q3 sync",
            "summary": "x",
            "key_points": ["a", "b", "c"],
            "mom": "## A",
            "language": "en",
            "call_type": "product_sync",
            "call_type_confidence": 0.9,
            "participants": [],
            "action_items": [],
            "decisions": [],
            "open_questions": []
        });
        let parsed = parse_summary_v2_or_promote_legacy(json, "c1").unwrap();
        assert_eq!(parsed.schema_version, 2);
        assert_eq!(parsed.call_type, CallType::ProductSync);
    }

    #[tokio::test]
    async fn parse_summary_v2_falls_back_to_legacy_on_v1_json() {
        // V1 не имеет schema_version=2 → fallback на legacy promote.
        let json = serde_json::json!({
            "version": 1,
            "title": "Old call",
            "summary": "Brief",
            "key_points": ["x"],
            "mom": "## A",
            "action_items": [{"text": "do thing", "owner_hint": null, "due": null}],
            "participants": []
        });
        let parsed = parse_summary_v2_or_promote_legacy(json, "c1").unwrap();
        assert_eq!(parsed.schema_version, 2);
        assert_eq!(parsed.call_type, CallType::Other);
        assert_eq!(parsed.title, "Old call");
        assert_eq!(parsed.action_items.len(), 1);
    }
}
