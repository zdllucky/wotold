# PRD: Wotold Call Summary Generation System (v2)

**Версия:** 1.0 | **Дата:** 22 мая 2026 | **Owner:** Prompt Engineering | **Target executor:** Claude Code

---

## TL;DR

- **Что строим:** type-driven, evidence-grounded multi-step pipeline (`classify → segment → map → reduce → expert → action-item post-pass → validate`) для саммари корпоративных звонков, local-first на Qwen 2.5 1.5B/3B/7B Instruct GGUF через llama.cpp, cloud fallback на **Grok-4.1-Fast-Reasoning** через xAI API (Anthropic Claude Sonnet 4 path удаляется).
- **Главные риски:** (1) Qwen 2.5 7B критический long-context threshold — F1 0.55→0.30 на 40-50% от заявленного 128K (arXiv:2601.15300), поэтому effective context = 16-24K, не 128K; (2) Grok-4.1-Fast-Reasoning имеет 19.2% hallucination rate на Vectara HHEM-2.3 New Dataset (один из худших frontier — нужны substring-anchored evidence quotes); (3) 2-часовые звонки требуют map-reduce.
- **Производительность таргет:** 45-минутный звонок на M2 Pro 16GB / Qwen 7B Q4_K_M end-to-end ≤ 3 минуты; 2-часовой ≤ 12 минут; cloud Grok ≤ 60 секунд.

---

## 1. Executive Summary

Wotold переходит от single-shot LLM-саммари к **type-driven, evidence-grounded, multi-step pipeline**. Главный приоритет — **local-first** (Qwen 2.5 1.5B / 3B / 7B Instruct GGUF через llama.cpp), cloud (Grok через xAI API) остаётся fallback для Pro-пользователей и звонков, которые не помещаются в локальный effective context.

Ключевые причины редизайна:
1. **Single-shot на small LLM деградирует на длинных входах.** arXiv:2601.15300 показывает: Qwen2.5-7B имеет критический threshold на 40-50% от max context (128K), где F1 падает с 0.55-0.56 до 0.30 — 45.5% деградация. Реальный safe ctx для extraction = 16-24K, не 128K.
2. **Универсальный шаблон проигрывает type-specific.** Avoma / MeetGeek / Granola / Fireflies / Fathom все используют type-driven templates. Fireflies (с ~75% Fortune 500 adoption по их homepage) поддерживает 60+ языков и обрабатывает meetings как collaborative data, Fathom рекламирует ~95% transcription accuracy с 30-секундным post-call processing.
3. **Claude Sonnet 4 path больше не доступен.** Мигрируем на Grok-4.1-Fast-Reasoning ($0.20/M input, $0.50/M output, 2M ctx, cached $0.05/M — официальная xAI pricing на 2026-05-20) с нативным `response_format: json_schema` strict mode.

Архитектура pipeline: `classify → segment → map (если нужно) → reduce → expert-prompt → action-item post-pass → optional GBNF sealing`, с substring-anchored evidence quotes (fuzzy match ≥ 90%) на каждом decision и action item. Это критично, так как Grok-4.1-Fast-Reasoning на Vectara HHEM-2.3 New Dataset показывает 19.2% hallucination rate (answer rate 99.7%) — он любит выдумывать, и без post-hoc verification это пробьётся в выход.

Документ описывает 18 задач (12 P0, 4 P1, 2 P2) для Claude Code, полные production-ready промпты на ChatML и Grok system/user формате, SQL миграции (tauri-plugin-sql), TypeScript/Rust типы с serde aliases, llama.cpp параметры, eval setup с G-Eval (0.514 Spearman correlation с human judgments per Liu et al. arXiv:2303.16634), acceptance criteria.

---

## 2. Goals и Non-Goals

### Goals (in scope)
1. **Local-first** саммари для 45-минутных звонков на Qwen 2.5 7B Q4_K_M на M2 Pro 16GB за ≤ 3 минуты со структурированным JSON-выходом.
2. Поддержка 2-часовых звонков через map-reduce без потери > 15% качества (G-Eval).
3. **Type-driven templates** для 8 типов: sales_discovery, sales_demo, product_sync, standup, customer_interview, one_on_one, strategy_brainstorm, status_update.
4. **Evidence-grounded** action items и decisions: каждый item содержит `evidence_quote` (verbatim substring ≥ 90% fuzzy match).
5. Cloud fallback через **Grok-4.1-Fast-Reasoning** с `response_format: json_schema` strict mode.
6. ru / en / kk + code-switching.
7. Backward compatibility со старыми саммари.

### Non-Goals
- Realtime / streaming summarization во время звонка.
- Cross-meeting agentic search (отдельный MCP epic).
- Fine-tuning Qwen.
- Voice cloning, TTS, iOS/Android port.
- Замена STT (Soniox / Gladia остаются).

---

## 3. Целевая архитектура

### 3.1 Pipeline

```
STT diarized transcript
        │
   Tokenizer + length estimator (Qwen BPE proxy)
        │
   ┌────┴────┐
   │ token ≤ │ ──── yes ──► ONE-SHOT: classify → expert prompt
   │ budget? │                                      ↓
   └────┬────┘                                action-item post-pass
        │ no                                        ↓
        ▼                                      validator (substring,
   MAP-REDUCE: segment (topic-tile) →           dedup, schema)
   per-chunk map → reduce → classify →               ↓
   expert prompt → action-item post-pass →     optional GBNF re-seal
   validator → optional GBNF re-seal                 ↓
                                              persist + render
```

### 3.2 Engine matrix

| Preset | Engine | Model | Quant | KV cache | Effective ctx | Use when |
|---|---|---|---|---|---|---|
| **Light** | local | Qwen 2.5 1.5B Instruct | Q4_K_M | f16 | 8K | < 30 min calls, low-spec hw |
| **Balanced** (default) | local | Qwen 2.5 3B Instruct | Q4_K_M | q8_0 | 12K | 30–60 min, M2 Pro 16GB |
| **Quality** | local | Qwen 2.5 7B Instruct | Q4_K_M | q8_0 | 16–24K | 45 min–2h, M2 Pro 16GB |
| **Pro / Cloud** | cloud | grok-4-1-fast-reasoning | — | — | 2M nominal, use ≤200K | user opt-in или transcript > local ctx |

Effective context установлен консервативно: arXiv:2601.15300 показал критический threshold для Qwen2.5-7B на 40-50% max context с F1 collapse 0.55→0.30; берём 16-24K для 7B как safe-зону. Для cloud (Grok-4.1-Fast) — nominal 2M, но используем ≤ 200K input, так как Grok 4.1 Fast tier меняется при >200K и hallucination rate растёт.

### 3.3 Стратегия для длинных звонков

| Транскрипт | Light (1.5B / 8K) | Balanced (3B / 12K) | Quality (7B / 24K) | Cloud |
|---|---|---|---|---|
| ≤ 6K (~25 min) | one-shot | one-shot | one-shot | one-shot |
| 6–12K (~25–50 min) | map-reduce 3.2K/0.32K | one-shot | one-shot | one-shot |
| 12–24K (~50–100 min) | map-reduce 3.2K | map-reduce 4.8K/0.48K | one-shot | one-shot |
| 24–48K (~100–200 min) | hierarchical (3 levels) | map-reduce 4.8K | map-reduce 9.6K/0.96K | one-shot |
| > 48K | force cloud / refuse | force cloud | hierarchical | one-shot (warn) |

Chunk size = effective_ctx × 0.4 (оставляем 60% на prompt + output), overlap = 10% (≈ 2 минуты речи). LangChain практика для подобных пайплайнов использует chunk 1000-2000 / overlap 100-200, что мы масштабируем под наши effective contexts.

---

## 4. Data model changes

### 4.1 SQL миграция

Файл: `apps/desktop/src-tauri/migrations/0007_call_summary_v2.sql`

```sql
ALTER TABLE summaries ADD COLUMN call_type TEXT DEFAULT NULL;
ALTER TABLE summaries ADD COLUMN call_type_confidence REAL DEFAULT NULL;
ALTER TABLE summaries ADD COLUMN language TEXT DEFAULT NULL;
ALTER TABLE summaries ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 2;
ALTER TABLE summaries ADD COLUMN engine TEXT DEFAULT NULL;
ALTER TABLE summaries ADD COLUMN pipeline_mode TEXT DEFAULT NULL;
ALTER TABLE summaries ADD COLUMN generation_ms INTEGER DEFAULT NULL;
ALTER TABLE summaries ADD COLUMN input_tokens INTEGER DEFAULT NULL;
ALTER TABLE summaries ADD COLUMN output_tokens INTEGER DEFAULT NULL;
ALTER TABLE summaries ADD COLUMN type_specific_block TEXT;

CREATE TABLE IF NOT EXISTS decisions (
  id TEXT PRIMARY KEY,
  summary_id TEXT NOT NULL REFERENCES summaries(id) ON DELETE CASCADE,
  text TEXT NOT NULL,
  evidence_quote TEXT,
  evidence_speaker TEXT,
  evidence_start_ms INTEGER,
  evidence_end_ms INTEGER,
  confidence REAL,
  order_idx INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX IF NOT EXISTS idx_decisions_summary ON decisions(summary_id);

CREATE TABLE IF NOT EXISTS open_questions (
  id TEXT PRIMARY KEY,
  summary_id TEXT NOT NULL REFERENCES summaries(id) ON DELETE CASCADE,
  text TEXT NOT NULL,
  raised_by TEXT,
  evidence_quote TEXT,
  order_idx INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_open_questions_summary ON open_questions(summary_id);

ALTER TABLE action_items ADD COLUMN owner_confidence REAL DEFAULT NULL;
ALTER TABLE action_items ADD COLUMN due_confidence REAL DEFAULT NULL;
ALTER TABLE action_items ADD COLUMN category TEXT DEFAULT 'commitment';
ALTER TABLE action_items ADD COLUMN evidence_quote TEXT;
ALTER TABLE action_items ADD COLUMN evidence_speaker TEXT;
ALTER TABLE action_items ADD COLUMN evidence_start_ms INTEGER;

UPDATE summaries SET schema_version = 1 WHERE schema_version IS NULL;
```

Файл: `apps/desktop/src-tauri/migrations/0008_call_summary_indices.sql`

```sql
CREATE INDEX IF NOT EXISTS idx_summaries_call_type ON summaries(call_type);
CREATE INDEX IF NOT EXISTS idx_summaries_engine ON summaries(engine);
CREATE INDEX IF NOT EXISTS idx_action_items_category ON action_items(category);
```

Регистрируется через tauri-plugin-sql `add_migrations` в `lib.rs`. Non-destructive — все ALTER ADD COLUMN.

### 4.2 TypeScript types

Файл: `packages/contracts/src/summary.ts`

```typescript
export type CallType =
  | "sales_discovery" | "sales_demo" | "product_sync" | "standup"
  | "customer_interview" | "one_on_one" | "strategy_brainstorm" | "status_update" | "other";

export type ActionItemCategory = "commitment" | "proposal" | "idea";
export type Engine = "local-qwen-1.5b" | "local-qwen-3b" | "local-qwen-7b" | "cloud-grok-4.1-fast";
export type PipelineMode = "one_shot" | "map_reduce" | "hierarchical";

export interface EvidenceAnchor {
  quote: string;
  speaker?: string;
  start_ms?: number;
  end_ms?: number;
}

export interface ActionItem {
  id: string;
  text: string;
  owner_hint?: string;
  owner_confidence?: number;
  due?: string;
  due_confidence?: number;
  category: ActionItemCategory;
  evidence?: EvidenceAnchor;
}

export interface Decision {
  id: string;
  text: string;
  evidence?: EvidenceAnchor;
  confidence?: number;
}

export interface OpenQuestion {
  id: string;
  text: string;
  raised_by?: string;
  evidence?: EvidenceAnchor;
}

export interface Participant {
  speaker_tag: string;
  display_name?: string;
  role_hint?: string;
}

export interface CallSummary {
  id: string;
  schema_version: 2;
  title: string;
  summary: string;
  key_points: string[];
  mom: string;
  language: "ru" | "en" | "kk" | "mixed";
  call_type: CallType;
  call_type_confidence: number;
  participants: Participant[];
  action_items: ActionItem[];
  decisions: Decision[];
  open_questions: OpenQuestion[];
  type_specific_block?: Record<string, unknown>;
  meta: {
    engine: Engine;
    pipeline_mode: PipelineMode;
    generation_ms: number;
    input_tokens: number;
    output_tokens: number;
  };
}

export interface CallSummaryV1 {
  schema_version?: 1;
  title: string;
  summary: string;
  key_points: string[];
  mom: string;
  action_items: Array<{ text: string; ownerHint?: string; due?: string }>;
  participants: Array<{ speakerTag: string; displayName?: string }>;
}
```

### 4.3 Rust types

Файл: `apps/desktop/src-tauri/src/pipeline/types.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CallType {
    SalesDiscovery, SalesDemo, ProductSync, Standup,
    CustomerInterview, OneOnOne, StrategyBrainstorm, StatusUpdate, Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionItemCategory { Commitment, Proposal, Idea }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvidenceAnchor {
    pub quote: String,
    #[serde(default)] pub speaker: Option<String>,
    #[serde(default, alias = "startMs")] pub start_ms: Option<u64>,
    #[serde(default, alias = "endMs")]   pub end_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: String,
    pub text: String,
    #[serde(default, alias = "ownerHint")] pub owner_hint: Option<String>,
    #[serde(default, alias = "ownerConfidence")] pub owner_confidence: Option<f32>,
    #[serde(default)] pub due: Option<String>,
    #[serde(default, alias = "dueConfidence")] pub due_confidence: Option<f32>,
    #[serde(default = "default_commitment")] pub category: ActionItemCategory,
    #[serde(default)] pub evidence: Option<EvidenceAnchor>,
}
fn default_commitment() -> ActionItemCategory { ActionItemCategory::Commitment }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String, pub text: String,
    #[serde(default)] pub evidence: Option<EvidenceAnchor>,
    #[serde(default)] pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenQuestion {
    pub id: String, pub text: String,
    #[serde(default, alias = "raisedBy")] pub raised_by: Option<String>,
    #[serde(default)] pub evidence: Option<EvidenceAnchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    #[serde(alias = "speakerTag")] pub speaker_tag: String,
    #[serde(default, alias = "displayName")] pub display_name: Option<String>,
    #[serde(default, alias = "roleHint")] pub role_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSummary {
    pub schema_version: u8,
    pub title: String, pub summary: String,
    pub key_points: Vec<String>, pub mom: String,
    pub language: String,
    pub call_type: CallType,
    #[serde(alias = "callTypeConfidence")] pub call_type_confidence: f32,
    pub participants: Vec<Participant>,
    pub action_items: Vec<ActionItem>,
    pub decisions: Vec<Decision>,
    pub open_questions: Vec<OpenQuestion>,
    #[serde(default)] pub type_specific_block: Option<serde_json::Value>,
}
```

---

## 5. Промпты (production-ready)

**Общие принципы:**
- Инструкция на английском (Qwen 2.5 IFEval сильнее на en), output language задаётся через явную инструкцию.
- **БЕЗ CoT** — BRIDGE benchmark (arXiv:2509.21933) показал деградацию на 82 из 95 LLM при добавлении CoT.
- **Describe-then-output**: схема → JSON. Прецифилл `{` для local.
- Per llama.cpp grammars README: "The JSON schema is only used to constrain the model output and is not injected into the prompt. The model has no visibility into the schema, so if you want it to understand the expected structure, describe it explicitly in your prompt." — поэтому GBNF не заменяет describe.

### 5.1 Универсальный cloud-промпт (Grok 4.1 Fast)

Файл: `apps/desktop/src-tauri/src/pipeline/prompts/cloud_universal.txt`

**System message:**
```
You are a senior meeting analyst at Wotold, a corporate call recording tool. Your job: produce a faithful, evidence-grounded JSON summary of a meeting transcript.

ABSOLUTE RULES (violations are bugs):
1. NEVER invent facts, names, dates, numbers, or commitments not present in the transcript.
2. Every action_item, decision, and open_question MUST include `evidence.quote` — a verbatim substring (10–200 chars) copied from the transcript. If you cannot find a verbatim anchor, OMIT the item.
3. Owner attribution: only assign an owner if the transcript shows them explicitly accepting the task ("I'll do it", "I will take that"). Mere mention of a name is NOT enough. Set `owner_confidence` accordingly (≥ 0.8 only for explicit acceptance).
4. Categorize each action item: "commitment" (explicit accept), "proposal" (suggested but not accepted), "idea" (raised but no clear action).
5. Output ONLY one JSON object matching the schema. No prose, no markdown fences, no explanation.
6. Output language MUST be the dominant language of the transcript, except `call_type` and `category` enum values which stay English.

OUTPUT SCHEMA:
{
  "schema_version": 2,
  "title": string (≤ 80 chars, specific — NOT "Team Meeting"),
  "summary": string (3–6 sentences, no greeting, no closing),
  "key_points": string[] (3–7 items),
  "language": "ru" | "en" | "kk" | "mixed",
  "call_type": "sales_discovery" | "sales_demo" | "product_sync" | "standup" | "customer_interview" | "one_on_one" | "strategy_brainstorm" | "status_update" | "other",
  "call_type_confidence": number (0..1),
  "participants": [ { "speaker_tag": string, "display_name": string|null, "role_hint": string|null } ],
  "action_items": [ {
    "text": string, "owner_hint": string|null, "owner_confidence": number,
    "due": string|null, "due_confidence": number,
    "category": "commitment"|"proposal"|"idea",
    "evidence": { "quote": string, "speaker": string|null }
  } ],
  "decisions": [ { "text": string, "evidence": {"quote": string, "speaker": string|null}, "confidence": number } ],
  "open_questions": [ { "text": string, "raised_by": string|null, "evidence": {"quote": string, "speaker": string|null} } ],
  "mom": string (markdown; structure depends on call_type — see TYPE GUIDE),
  "type_specific_block": object|null
}

TYPE GUIDE:
- sales_discovery: sections "## Customer pain", "## Stakeholders", "## Budget signals", "## Next steps". type_specific_block: { "pain_points": string[], "current_solution": string|null, "budget_signal": string|null, "decision_makers": [{"name":string,"role":string|null,"stance":"champion"|"neutral"|"blocker"|"unknown"}], "timeline_hint": string|null }
- sales_demo: "## Demo flow", "## Objections", "## Buying signals", "## Follow-up commitments". type_specific_block: { "objections": [{"raised":string,"resolved":bool}], "buying_signals": string[] }
- product_sync: "## Progress since last sync", "## Blockers", "## Decisions", "## Next milestones". type_specific_block: { "blockers": string[], "milestones": [{"name":string,"target":string}] }
- standup: "## Yesterday", "## Today", "## Blockers". type_specific_block: { "per_person": [{"speaker":string,"yesterday":string,"today":string,"blockers":string|null}] }
- customer_interview: "## Job to be done", "## Current workflow", "## Pain quotes", "## Feature requests". type_specific_block: { "jtbd": string|null, "pain_quotes": [{"quote":string,"speaker":string}] }
- one_on_one: "## Wins", "## Challenges", "## Feedback", "## Career". PRIVACY: minimal type_specific_block { "topics_discussed": string[], "follow_ups_committed": string[] }. NO verbatim personal feedback quotes — paraphrase only (evidence.quote = null for personal items).
- strategy_brainstorm: "## Ideas generated", "## Top picks", "## Open questions", "## Owners assigned". type_specific_block: { "ideas": [{"text":string,"votes":number|null}] }
- status_update: "## Status by workstream", "## Risks", "## Asks". type_specific_block: { "workstreams": [{"name":string,"status":"green"|"yellow"|"red","note":string}] }

EVIDENCE QUOTE RULES:
- Verbatim substring of transcript. Preserve original language and casing.
- 10–200 characters.
- Set `speaker` field separately rather than duplicating speaker label inside the quote when possible.

LANGUAGE: detect dominant language. Mixed ru/en → respond in ru with English technical terms as-is.
```

**User message:**
```
TRANSCRIPT:
<<<
{transcript}
>>>

Produce the JSON summary now. Output ONLY the JSON object.
```

**xAI API call** (Rust, replacing existing Anthropic call):
```rust
// apps/desktop/src-tauri/src/pipeline/cloud.rs
let body = serde_json::json!({
    "model": "grok-4-1-fast-reasoning",
    "messages": [
        { "role": "system", "content": SYSTEM_PROMPT },
        { "role": "user", "content": format!("TRANSCRIPT:\n<<<\n{}\n>>>\n\nProduce the JSON summary now. Output ONLY the JSON object.", transcript) }
    ],
    "response_format": {
        "type": "json_schema",
        "json_schema": {
            "name": "call_summary_v2",
            "schema": CALL_SUMMARY_JSON_SCHEMA.clone(),
            "strict": true
        }
    },
    "temperature": 0.2,
    "max_tokens": 6000,
    "reasoning_effort": "low"
});

let resp = client
    .post("https://api.x.ai/v1/chat/completions")
    .bearer_auth(api_key)
    .json(&body)
    .send().await?;
```

Notes:
- xAI pricing for Grok-4.1-Fast (per docs.x.ai, synced 2026-05-20 via aipricing.guru): **$0.20/M input tokens, $0.50/M output tokens, 2,000,000-token context window, cached input $0.05/M**. No hedging — published list prices.
- Hallucination risk: Vectara HHEM-2.3 leaderboard scores `xai-org/grok-4-1-fast-reasoning` at **19.2%** hallucination rate (answer rate 99.7%); the related `grok-4-fast-reasoning` variant scores 20.2%. Both are among the worst-performing frontier models on the new 7,700-article enterprise dataset. **MUST** verify evidence quotes post-hoc.
- `reasoning_effort: "low"` — default for reasoning models; saves cost. Use `"medium"` for transcripts > 50K tokens.
- Prompt caching: keep system message byte-identical between calls to qualify for $0.05/M cached rate.

### 5.2 Type classifier (Step 1 of local pipeline)

Файл: `apps/desktop/src-tauri/src/local_engine/prompts/classify.txt`

**System:**
```
You classify corporate call transcripts into one of 9 types. Output ONLY valid JSON, no prose.

TYPES (with telltale signals):
- sales_discovery: rep + prospect; questions about pain, budget, timeline, decision-makers; no product demo.
- sales_demo: rep walks through product features; objections; pricing discussion.
- product_sync: internal team; engineering/design/PM updates; tickets, sprints, blockers.
- standup: short rotating updates per person; yesterday/today/blockers.
- customer_interview: user research; open-ended questions; no selling.
- one_on_one: 2 speakers; manager-report; career, feedback.
- strategy_brainstorm: open-ended ideation; multiple ideas raised; voting.
- status_update: workstream/project reporting upward; risks, asks, escalations.
- other: doesn't fit cleanly.

OUTPUT JSON:
{ "call_type": "<one of above>", "confidence": <0..1>, "language": "ru"|"en"|"kk"|"mixed" }
```

**User:**
```
Transcript excerpt (first 1500 chars + last 800 chars):
{excerpt}

Classify now. JSON only.
```

Префилл: `{"call_type":`

### 5.3 Map-pass prompt

Файл: `apps/desktop/src-tauri/src/local_engine/prompts/map_pass.txt`

**System:**
```
You are processing chunk {chunk_idx} of {total_chunks} from a meeting transcript. Extract structured facts from THIS CHUNK ONLY. Do not summarize the whole meeting.

OUTPUT JSON only:
{
  "chunk_idx": <int>,
  "facts": [ <≤25 words each, source language> ],
  "decisions_candidates": [ { "text": string, "evidence_quote": string (verbatim from chunk), "speaker": string|null } ],
  "action_candidates": [ { "text": string, "owner_hint": string|null, "due": string|null, "category": "commitment"|"proposal"|"idea", "evidence_quote": string (verbatim), "speaker": string|null } ],
  "open_questions_candidates": [ { "text": string, "raised_by": string|null, "evidence_quote": string (verbatim) } ],
  "topic_tags": [ <1-3 word tags> ],
  "participants_mentioned": [ string ]
}

RULES:
- evidence_quote MUST be a verbatim substring of the chunk text (10–200 chars).
- "category": commitment = explicit accept ("I'll do X"); proposal = suggested but not accepted; idea = raised, no action.
- If chunk has no decisions/actions/questions, return empty arrays. NEVER fabricate.
- Use source language. Do not translate.
```

**User:**
```
CHUNK {chunk_idx} OF {total_chunks}:
<<<
{chunk_text}
>>>

Extract now. JSON only.
```

Префилл: `{"chunk_idx":{chunk_idx},`

### 5.4 Reduce / synthesis prompt

Файл: `apps/desktop/src-tauri/src/local_engine/prompts/reduce.txt`

**System:**
```
You synthesize a coherent meeting summary from per-chunk extractions (already JSON). Your job:
1. Deduplicate decisions/action_items/open_questions across chunks (same intent → keep best evidence_quote).
2. Resolve cross-chunk references: if an action proposed in chunk K is accepted in chunk K+M, merge them; category becomes "commitment".
3. Order action_items by importance and recency.
4. Identify the dominant call_type from `topic_tags` distribution (consistent with classifier output: {classifier_call_type}).
5. Compose `title` (specific, ≤ 80 chars, NOT generic), `summary` (3–6 sentences), `key_points` (3–7).
6. Build `mom` markdown following the structure for {call_type}.

NEVER add facts not present in input extractions. NEVER invent participants, dates, numbers.

OUTPUT: single JSON object matching CallSummary v2 schema. JSON only.
```

**User:**
```
CLASSIFIER RESULT: { "call_type": "{classifier_call_type}", "confidence": {classifier_conf}, "language": "{language}" }

PER-CHUNK EXTRACTIONS (JSON array, ordered):
{chunk_extractions_json}

PARTICIPANTS (from STT diarization):
{participants_list}

Now synthesize the final CallSummary JSON. JSON only.
```

Префилл: `{"schema_version":2,"title":"`

### 5.5 Expert prompts (8 типов)

Все expert-prompts следуют схеме cloud_universal с `ABSOLUTE RULES` + фиксированным `type_specific_block` под конкретный тип. Файлы: `apps/desktop/src-tauri/src/local_engine/prompts/expert/{call_type}.txt`. Ниже три ключевых.

**expert/sales_discovery.txt:**
```
You are an expert sales analyst summarizing a DISCOVERY call (rep qualifying a prospect).

Focus areas (extract WITH evidence quotes):
- Customer pain (what hurts today?)
- Current solution & gaps
- Budget signals (numbers, "no budget", procurement timing)
- Decision makers (who decides? who blocks? who champions?)
- Timeline
- Next steps committed (NOT vague "let's stay in touch" — concrete only)

MoM structure (exact headers):
## Customer pain
- bullet with evidence as > blockquote
## Stakeholders
- name — role — stance (champion/neutral/blocker)
## Budget signals
## Next steps

type_specific_block:
{
  "pain_points": string[],
  "current_solution": string|null,
  "budget_signal": string|null,
  "decision_makers": [{ "name": string, "role": string|null, "stance": "champion"|"neutral"|"blocker"|"unknown" }],
  "timeline_hint": string|null
}

ABSOLUTE RULES (same as base): evidence.quote = verbatim substring 10-200 chars; owner_confidence ≥ 0.8 only for explicit accept; output language = transcript language; JSON only.
```

**expert/standup.txt:**
```
You summarize a daily STANDUP. Each speaker reports yesterday/today/blockers.

MoM:
## Yesterday
- speaker_name: brief
## Today
- speaker_name: brief
## Blockers
- speaker_name: blocker > evidence quote

type_specific_block:
{
  "per_person": [ { "speaker": string, "yesterday": string, "today": string, "blockers": string|null } ]
}

Action items are usually low — only extract where someone explicitly commits to unblock another or deliver by specific time.
```

**expert/one_on_one.txt (PRIVACY-sensitive):**
```
You summarize a 1:1 between manager and report. PRIVACY-FIRST.

PRIVACY RULES:
- DO NOT include verbatim quotes of personal feedback, criticism, salary, emotional content in evidence.quote. Use paraphrase + evidence = null.
- DO summarize topics at category level: "discussed career growth", "discussed workload concerns".
- Skip action items unless explicitly committed with clear deliverable.

MoM:
## Wins (general, no quotes)
## Challenges (paraphrased)
## Feedback exchanged (high-level)
## Career

type_specific_block:
{
  "topics_discussed": string[],     // ≤ 5 tags
  "follow_ups_committed": string[]
}

decisions, open_questions: KEEP EMPTY unless work-relevant.
action_items: only work commitments.
```

Остальные expert (`sales_demo`, `product_sync`, `customer_interview`, `strategy_brainstorm`, `status_update`) — по тому же шаблону с TYPE GUIDE из §5.1.

### 5.6 Title regen (отдельный мини-pass)

Триггер: regex `^(meeting|call|sync|discussion|team|встреча|звонок)\b` OR длина < 15.

```
You generate a SPECIFIC, SCANNABLE meeting title (≤ 80 chars).

Bad: "Team Meeting", "Sync", "Call with John"
Good: "Q3 pricing decision: enterprise tier locked at $499", "Onboarding flow redesign — Anna picks variant B"

Rules:
- Mention topic, not format
- Include outcome/decision if any
- Use transcript language

INPUT: { "summary": "...", "key_points": [...], "decisions": [...] }
OUTPUT JSON: { "title": string }
```

### 5.7 Action items extraction post-pass

Файл: `apps/desktop/src-tauri/src/pipeline/prompts/action_items_postpass.txt`. Применяется ВСЕГДА для local. Обоснование: Action-Item-Driven Summarization (Golia & Kalita, NLPIR 2023, arXiv:2312.17581) даёт +4.98% BERTScore на AMI corpus за счёт изоляции extraction.

```
You extract ACTION ITEMS from a meeting transcript. Focused task: only commitments to act, with owners and timing.

DEFINITIONS (Purver/Ehlen/Niekrasz 4-class):
- task_description: verb + object
- owner: who will do it (must be explicit acceptance, not just mention)
- timeframe: when (explicit OR clear horizon)
- agreement: was it accepted? (commitment vs proposal vs idea)

PERSONAL DEIXIS WARNING: pronouns ("I'll", "we'll", "you should") are the #1 source of owner-attribution errors. Only assign owner if the speaker tag of the speaker who said "I" is identifiable. If "we" or "someone" — leave owner_hint = null.

OUTPUT JSON array:
[ {
  "text": string,
  "owner_hint": string|null,
  "owner_confidence": number,   // 0.9+ only when speaker explicitly says "I'll/I will"
  "due": string|null,            // ISO date OR human ("end of Q2", "next Tuesday")
  "due_confidence": number,
  "category": "commitment"|"proposal"|"idea",
  "evidence": { "quote": string (verbatim, 10-200 chars), "speaker": string|null }
} ]

Return empty array if no action items. NEVER invent. JSON array only.
```

### 5.8 Qwen 2.5 ChatML wrapper

```
<|im_start|>system
{system_prompt}<|im_end|>
<|im_start|>user
{user_prompt}<|im_end|>
<|im_start|>assistant
{prefill}
```

Префиллы:
- classify → `{"call_type":`
- map_pass → `{"chunk_idx":{idx},`
- reduce / expert → `{"schema_version":2,"title":"`
- action_items_postpass → `[`
- title_regen → `{"title":"`

llama.cpp invocation (Quality preset):
```bash
llama-cli -m qwen2.5-7b-instruct-q4_k_m.gguf \
  --jinja \
  -fa \
  -ngl 99 \
  -c 24576 \
  -n 4096 \
  --temp 0.2 \
  --top-p 0.9 \
  --min-p 0.05 \
  --repeat-penalty 1.05 \
  --no-context-shift \
  -ctk q8_0 -ctv q8_0 \
  -p "<ChatML+prefill>"
```

Параметры:
- `-fa` (flash attention) — обязательно на Metal.
- `-ctk q8_0 -ctv q8_0` — KV cache q8_0 cuts context VRAM roughly in half with negligible perplexity impact on Qwen 2.5 (under 0.05 PPL).
- `--no-context-shift` — НЕ резать prompt, лучше падать и пересегментировать.
- `--temp 0.2` — низкая для extraction.

YaRN/rope scaling: НЕ нужно (max ctx 24K < 32K natively trained).

### 5.9 GBNF grammar (sealing fallback для 1.5B)

Файл: `apps/desktop/src-tauri/src/local_engine/grammars/call_summary_v2.gbnf`

```gbnf
root        ::= summary
summary     ::= "{" ws "\"schema_version\":" ws "2" ws ","
                ws "\"title\":" ws str ws ","
                ws "\"summary\":" ws str ws ","
                ws "\"key_points\":" ws strArr ws ","
                ws "\"language\":" ws lang ws ","
                ws "\"call_type\":" ws callType ws ","
                ws "\"call_type_confidence\":" ws num ws ","
                ws "\"participants\":" ws partArr ws ","
                ws "\"action_items\":" ws aiArr ws ","
                ws "\"decisions\":" ws decArr ws ","
                ws "\"open_questions\":" ws oqArr ws ","
                ws "\"mom\":" ws str ws "}"

lang        ::= "\"ru\"" | "\"en\"" | "\"kk\"" | "\"mixed\""
callType    ::= "\"sales_discovery\"" | "\"sales_demo\"" | "\"product_sync\""
              | "\"standup\"" | "\"customer_interview\"" | "\"one_on_one\""
              | "\"strategy_brainstorm\"" | "\"status_update\"" | "\"other\""

partArr     ::= "[" ws (part (ws "," ws part)*)? ws "]"
part        ::= "{" ws "\"speaker_tag\":" ws str ws "," ws "\"display_name\":" ws (str | "null") ws "," ws "\"role_hint\":" ws (str | "null") ws "}"

aiArr       ::= "[" ws (ai (ws "," ws ai)*)? ws "]"
ai          ::= "{" ws "\"text\":" ws str ws ","
                ws "\"owner_hint\":" ws (str | "null") ws ","
                ws "\"owner_confidence\":" ws num ws ","
                ws "\"due\":" ws (str | "null") ws ","
                ws "\"due_confidence\":" ws num ws ","
                ws "\"category\":" ws ("\"commitment\"" | "\"proposal\"" | "\"idea\"") ws ","
                ws "\"evidence\":" ws evidence ws "}"

decArr      ::= "[" ws (dec (ws "," ws dec)*)? ws "]"
dec         ::= "{" ws "\"text\":" ws str ws "," ws "\"evidence\":" ws evidence ws "," ws "\"confidence\":" ws num ws "}"

oqArr       ::= "[" ws (oq (ws "," ws oq)*)? ws "]"
oq          ::= "{" ws "\"text\":" ws str ws "," ws "\"raised_by\":" ws (str | "null") ws "," ws "\"evidence\":" ws evidence ws "}"

evidence    ::= "{" ws "\"quote\":" ws str ws "," ws "\"speaker\":" ws (str | "null") ws "}"

strArr      ::= "[" ws (str (ws "," ws str)*)? ws "]"
str         ::= "\"" ([^"\\] | "\\" ["\\/bfnrt])* "\""
num         ::= "-"? [0-9]+ ("." [0-9]+)?
ws          ::= [ \t\n]*
```

Передаётся через `--grammar-file` только при fallback (после JSON.parse failure). Не для default runs из-за ~15-25% slowdown.

---

## 6. Pipeline implementation

### 6.1 Файлы

```
apps/desktop/src-tauri/
├── migrations/
│   ├── 0007_call_summary_v2.sql               [NEW]
│   └── 0008_call_summary_indices.sql          [NEW]
├── src/
│   ├── pipeline/
│   │   ├── mod.rs                              [MOD]
│   │   ├── recap.rs                            [REWRITE — cloud Grok path]
│   │   ├── cloud.rs                            [NEW — xAI client]
│   │   ├── orchestrator.rs                     [NEW — router]
│   │   ├── chunker.rs                          [NEW]
│   │   ├── types.rs                            [NEW]
│   │   ├── schema.rs                           [NEW — JSON schema for Grok strict]
│   │   ├── validator.rs                        [NEW — substring/dedup/quote verify]
│   │   └── prompts/
│   │       ├── cloud_universal.txt             [NEW]
│   │       ├── action_items_postpass.txt       [NEW]
│   │       └── title_regen.txt                 [NEW]
│   ├── local_engine/
│   │   ├── llm.rs                              [MOD — multi-step]
│   │   ├── llama_runner.rs                     [NEW]
│   │   ├── prompts/
│   │   │   ├── classify.txt                    [NEW]
│   │   │   ├── map_pass.txt                    [NEW]
│   │   │   ├── reduce.txt                      [NEW]
│   │   │   └── expert/
│   │   │       ├── sales_discovery.txt         [NEW]
│   │   │       ├── sales_demo.txt              [NEW]
│   │   │       ├── product_sync.txt            [NEW]
│   │   │       ├── standup.txt                 [NEW]
│   │   │       ├── customer_interview.txt      [NEW]
│   │   │       ├── one_on_one.txt              [NEW]
│   │   │       ├── strategy_brainstorm.txt     [NEW]
│   │   │       └── status_update.txt           [NEW]
│   │   └── grammars/call_summary_v2.gbnf       [NEW]
│   └── db/summary_repo.rs                      [MOD]
└── tauri.conf.json                              [MOD — register migrations]

apps/desktop/src/components/
├── SummaryView.tsx                              [MOD]
├── CallTypeBadge.tsx                            [NEW]
├── DecisionsBlock.tsx                           [NEW]
├── OpenQuestionsBlock.tsx                       [NEW]
├── ActionItemRow.tsx                            [MOD — confidence badges]
├── EvidenceTooltip.tsx                          [NEW]
├── PrivacyDisclaimer.tsx                        [NEW]
├── TypeSpecificBlock.tsx                        [NEW]
└── ProgressIndicator.tsx                        [MOD]

packages/contracts/src/summary.ts                [REWRITE]

tests/
├── golden/                                      [NEW]
│   ├── sales_discovery_ru_45min.json
│   ├── standup_en_15min.json
│   ├── product_sync_ru_60min.json
│   ├── one_on_one_ru_30min.json
│   ├── customer_interview_en_50min.json
│   ├── strategy_brainstorm_mixed_90min.json
│   └── transcripts/*.txt
└── pipeline_e2e.rs                              [NEW]
```

### 6.2 Orchestrator (псевдокод)

```rust
// apps/desktop/src-tauri/src/pipeline/orchestrator.rs

pub async fn summarize_call(
    transcript: &DiarizedTranscript,
    preset: Preset,
    cloud_pref: CloudPreference,
    progress: &ProgressEmitter,
) -> Result<CallSummary> {
    let token_count = estimate_tokens(&transcript.text());
    let ctx_budget = preset.effective_context();
    let engine = select_engine(preset, cloud_pref, token_count, ctx_budget);
    progress.emit(Phase::Routing, 0);

    match engine {
        Engine::CloudGrok => {
            let raw = cloud::call_grok(transcript, &progress).await?;
            let summary = validator::parse_and_verify(raw, transcript)?;
            Ok(summary)
        }
        Engine::Local(model) => local_pipeline(transcript, model, ctx_budget, &progress).await
    }
}

async fn local_pipeline(
    transcript: &DiarizedTranscript,
    model: LocalModel,
    ctx_budget: usize,
    progress: &ProgressEmitter,
) -> Result<CallSummary> {
    let token_count = estimate_tokens(&transcript.text());
    let pipeline_mode = if token_count <= (ctx_budget as f32 * 0.4) as usize {
        PipelineMode::OneShot
    } else if token_count <= ctx_budget * 3 {
        PipelineMode::MapReduce
    } else {
        PipelineMode::Hierarchical
    };

    // 1) Classify
    progress.emit(Phase::Classifying, 5);
    let classified = run_classifier(transcript, &model).await?;

    let final_summary = match pipeline_mode {
        PipelineMode::OneShot => {
            progress.emit(Phase::ExpertPrompt, 15);
            run_expert(transcript, &classified, &model).await?
        }
        PipelineMode::MapReduce => {
            let chunks = chunker::chunk_by_topic_then_size(
                transcript,
                (ctx_budget as f32 * 0.4) as usize,
                (ctx_budget as f32 * 0.04) as usize,
            )?;
            let total = chunks.len();
            let mut map_results = Vec::with_capacity(total);
            for (i, chunk) in chunks.iter().enumerate() {
                progress.emit(Phase::Mapping(i, total), 15 + (60 * i / total));
                map_results.push(run_map_pass(chunk, i, total, &model).await?);
            }
            progress.emit(Phase::Reducing, 80);
            let reduced = run_reduce(&classified, &map_results, &transcript.participants, &model).await?;
            progress.emit(Phase::ExpertPrompt, 90);
            run_expert_on_reduced(reduced, &classified, &model).await?
        }
        PipelineMode::Hierarchical => {
            let chunks = chunker::chunk_by_topic_then_size(transcript,
                (ctx_budget as f32 * 0.4) as usize, (ctx_budget as f32 * 0.04) as usize)?;
            // group of 8 chunks → mid-reduce → final reduce → expert
            run_hierarchical(chunks, &classified, &model, progress).await?
        }
    };

    // 2) Action items post-pass (ALWAYS for local)
    progress.emit(Phase::ActionItemPass, 92);
    let action_items = run_action_item_postpass(transcript, &model).await?;
    let mut merged = final_summary;
    merged.action_items = merge_and_dedup_actions(merged.action_items, action_items);

    // 3) Validate evidence
    progress.emit(Phase::Validating, 96);
    let validated = validator::verify_evidence_quotes(&merged, transcript, 0.90)?;

    // 4) Title regen if generic
    if title_is_generic(&validated.title) {
        progress.emit(Phase::TitleRegen, 98);
        validated.title = run_title_regen(&validated, &model).await?;
    }

    Ok(validated)
}
```

### 6.3 Chunking strategy

```rust
// apps/desktop/src-tauri/src/pipeline/chunker.rs

pub fn chunk_by_topic_then_size(
    transcript: &DiarizedTranscript,
    target_size: usize,
    overlap: usize,
) -> Result<Vec<Chunk>> {
    // Step 1: TextTiling-style topic segmentation on multilingual embeddings
    //   (paraphrase-multilingual-mpnet-base-v2 via candle if available,
    //    else fall back to lexical TextTiling on token n-grams per Hearst 1997).
    // Step 2: For each topic segment exceeding target_size, split at speaker
    //   turn boundaries closest to target.
    // Step 3: Add overlap: last `overlap` tokens of chunk K prepended to chunk K+1
    //   (sentence-aligned).
    //
    // Split priority (NEVER mid-utterance):
    //   1. Topic boundary (TextTiling)
    //   2. Speaker turn boundary
    //   3. Sentence boundary
    //   4. Last resort: token boundary
}
```

Конкретные значения:
- Light (Qwen 1.5B / ctx 8K): chunk=3200, overlap=320 → ~13 min/chunk
- Balanced (Qwen 3B / ctx 12K): chunk=4800, overlap=480 → ~20 min/chunk
- Quality (Qwen 7B / ctx 24K): chunk=9600, overlap=960 → ~40 min/chunk

### 6.4 Error handling / fallback chain

```
1. Local fails (JSON parse, validation, timeout):
   1a. Retry once with GBNF grammar.
   1b. Degrade schema: drop type_specific_block, drop evidence on action items.
   1c. If cloud available + user consent: route to cloud.
   1d. Return partial summary with error flag.

2. Cloud fails:
   2a. Exponential backoff retry (3 attempts).
   2b. 429 → queue + notify.
   2c. 401 → prompt API key update.
   2d. Fallback to local with notice.

3. Validation fails (substring < 90%):
   3a. Drop failing item, keep rest.
   3b. Log to telemetry for prompt iteration.
```

### 6.5 llama.cpp параметры по preset

| Preset | Model | -c | -n | -ctk/-ctv | -ngl | --temp | --top-p | -fa |
|---|---|---|---|---|---|---|---|---|
| Light | qwen2.5-1.5b-instruct-q4_k_m.gguf | 8192 | 2048 | f16/f16 | 99 | 0.2 | 0.9 | on |
| Balanced | qwen2.5-3b-instruct-q4_k_m.gguf | 12288 | 3072 | q8_0/q8_0 | 99 | 0.2 | 0.9 | on |
| Quality | qwen2.5-7b-instruct-q4_k_m.gguf | 24576 | 4096 | q8_0/q8_0 | 99 | 0.2 | 0.9 | on |

**Memory math для Quality на M2 Pro 16GB:** Qwen 2.5 7B Q4_K_M weights ~4.7 GB, KV cache q8_0 при 24K ctx ≈ 0.66 GB, общий резидент ~6.0 GB; macOS GPU wired limit ~12 GB (75% от 16 GB по дефолту); остаётся ~6 GB headroom — безопасно. При 32K ctx q8_0 KV = 0.88 GB total ~6.1 GB; всё ещё safe. f16 KV при 32K = 1.75 GB, total ~7.0 GB — тоже OK с overhead.

**Performance на M2 Pro 16-core GPU (200 GB/s)** для Qwen 2.5 7B Q4_K_M:
- TG @ 8K: ~30-33 t/s; @ 32K: ~25-28 t/s
- PP @ 8K: ~270-290 t/s; @ 16K: ~240-270 t/s; @ 32K: ~200-240 t/s
- Speculative decoding (0.5B drafting 7B на Metal): ~1.3-1.5× speedup на extraction-heavy workloads, существенно меньше чем 2.5-4× на CUDA. Опция, не дефолт.

---

## 7. UI changes

### 7.1 Progress indicator

Tauri events `summary-progress` → frontend `ProgressIndicator.tsx`:

```typescript
type Phase =
  | "routing" | "classifying" | "chunking"
  | { type: "mapping"; chunk: number; total: number }
  | "reducing" | "expert_prompt" | "action_item_pass"
  | "validating" | "title_regen" | "done";

// Rendering:
//   "Обработано {mapping.chunk}/{mapping.total} фрагментов" for map phase
//   "Извлекаю задачи" for action pass
//   "Проверяю цитаты" for validating
```

One-shot: spinner. Map-reduce: progress bar + ETA по per-chunk timing.

### 7.2 New fields rendering

```tsx
<header>
  <CallTypeBadge type={summary.call_type} confidence={summary.call_type_confidence} />
  <h1>{summary.title}</h1>
  {summary.call_type === "one_on_one" && <PrivacyDisclaimer />}
</header>
<section>{summary.summary}</section>
<KeyPoints items={summary.key_points} />
<DecisionsBlock decisions={summary.decisions} />
<OpenQuestionsBlock questions={summary.open_questions} />
<ActionItemsList>
  {summary.action_items.map(ai => <ActionItemRow item={ai} />)}
</ActionItemsList>
<MoMSection markdown={summary.mom} />
<TypeSpecificBlock data={summary.type_specific_block} type={summary.call_type} />
```

`<ActionItemRow>` badges:
- `owner_confidence >= 0.8`: solid blue, owner name shown
- `0.4 <= owner_confidence < 0.8`: gray "?" badge, tooltip "Owner inferred — verify"
- `owner_confidence < 0.4` or null: no owner, "Unassigned"
- `category === "proposal"`: amber "Proposal" badge
- `category === "idea"`: gray "Idea" badge
- `category === "commitment"`: green check

`<EvidenceTooltip>`: hover на evidence_quote → popover с цитатой + кнопка "Jump to {start_ms}".

### 7.3 Privacy disclaimer

For `call_type === "one_on_one"`:
```
🔒 Это была встреча 1:1 — содержит личную обратную связь.
Саммари показывает только темы (без цитат). Не делитесь без необходимости.
```

For `call_type === "customer_interview"` с feature flag sensitive:
```
⚠️ Содержит чувствительные данные. Проверьте политику хранения.
```

---

## 8. Evaluation и quality gates

### 8.1 Golden set

10-15 калибровочных звонков:
- 5 на русском (sales_discovery, standup, product_sync, one_on_one, brainstorm)
- 5 на английском (sales_demo, customer_interview, status_update, sync, brainstorm)
- 2-3 mixed ru/en
- 1 на казахском

Разметка в `tests/golden/{name}.json`:
```json
{
  "transcript_path": "transcripts/sales_discovery_ru_45min.txt",
  "expected_call_type": "sales_discovery",
  "expected_action_items_min": 4,
  "expected_action_items_max": 8,
  "must_contain_decisions": [
    { "intent": "scheduled follow-up on Tuesday", "must_have_evidence": true }
  ],
  "must_contain_owners": ["Анна", "Михаил"],
  "must_NOT_contain": ["John Smith", "Q5 2027"],
  "min_quote_substring_match_rate": 0.90
}
```

### 8.2 Automated checks (CI)

Файл: `apps/desktop/src-tauri/tests/pipeline_e2e.rs`

Для каждого golden:
1. **Schema validation**: JSON-Schema compile + validate → 100%.
2. **Substring match**: каждая `evidence.quote` fuzzy substring ≥ 90% (rapidfuzz crate partial_ratio).
3. **No generic titles**: regex blacklist `^(meeting|call|sync|discussion|team|встреча|звонок)\b` → fail.
4. **Call type accuracy**: predicted == expected для >= 80% golden.
5. **Action item count**: min <= len <= max.
6. **No hallucinated entities**: ни один `must_NOT_contain` не встречается.
7. **Owner attribution**: каждый `must_contain_owners` найден в action_items.owner_hint или participants.

CI gate: все 7 зелёные на 90% golden → merge OK.

### 8.3 LLM-as-judge (G-Eval) — offline, не блокирует merge

Per Liu et al., "G-Eval: NLG Evaluation using GPT-4 with Better Human Alignment" (arXiv:2303.16634, EMNLP 2023, pp. 2511-2522): "G-Eval with GPT-4 as the backbone model achieves a Spearman correlation of 0.514 with human on summarization task, outperforming all previous methods by a large margin." Используем reference-based (с golden ground truth) — чисто reference-free рискованнее с Grok 4.1 Fast в качестве judge.

Judge candidate: **Gemini 3 Pro** (neutral, низкий hallucination) или Grok-4-1-Fast-Reasoning (тот же, что cloud production — дешевле, но conflict-of-interest).

G-Eval dimensions (0-5 each):
- **Faithfulness**: "Все утверждения поддержаны транскриптом?"
- **Coverage**: "Покрыто важное, не пропущены key decisions?"
- **Specificity**: "Title/summary конкретны или generic?"
- **Action item quality**: "Owners/timing верно атрибутированы?"

KPI thresholds для merge:
- Faithfulness ≥ 4.0 avg
- Coverage ≥ 3.5 avg
- Specificity ≥ 3.5 avg
- No item с Faithfulness < 3

Per Belem et al., "From Single to Multi: How LLMs Hallucinate in Multi-Document Summarization" (arXiv:2410.13961, Megagon Labs): "when summarizing non-existent topic-related information, gpt-3.5-turbo and GPT-4o still generate summaries about 79.35% and 44% of the time" — поэтому судью тоже нельзя слепо доверять; используем reference-based + golden.

### 8.4 Regression snapshots

После каждого изменения промпта → all-golden pipeline → diff old vs new. Snapshots в `tests/snapshots/{commit_sha}/`.

---

## 9. Migration plan

### 9.1 Backward compatibility

Legacy adapter в `SummaryView.tsx`:
```typescript
function renderSummary(s: CallSummary | CallSummaryV1) {
  if (!s.schema_version || s.schema_version === 1) {
    return <LegacySummaryView summary={s as CallSummaryV1} />;
  }
  return <SummaryViewV2 summary={s as CallSummary} />;
}
```

`LegacySummaryView` рендерит только title/summary/key_points/mom/action_items (как раньше), без decisions/open_questions/type_specific.

### 9.2 Re-process button

На любом саммари schema_version=1 → кнопка "Обновить до v2". Триггер: повторно прогоняем pipeline на сохранённом transcript, новая запись (старая сохраняется как snapshot для отката).

### 9.3 Feature flag rollout

```typescript
// stores/featureFlags.ts
export const FLAGS = {
  pipeline_v2: localStorage.getItem("flag.pipeline_v2") === "1",
  pipeline_v2_default_quality: localStorage.getItem("flag.pipeline_v2_default_quality") === "1",
};
```

Этапы:
1. Week 1: OFF by default, opt-in via Settings → Labs.
2. Week 2: ON for Quality preset users.
3. Week 3: ON for Balanced.
4. Week 4: ON for Light + everyone; legacy path удаляется в next major.

### 9.4 Data migration

Миграции 0007/0008 — non-destructive ALTER ADD COLUMN через tauri-plugin-sql `add_migrations`. Existing rows получают NULL в новых полях. `UPDATE summaries SET schema_version=1 WHERE schema_version IS NULL` запускается одним statement.

---

## 10. Acceptance Criteria

**AC-1 (Cloud Grok migration)**
- `cloud.rs` использует endpoint `https://api.x.ai/v1/chat/completions`, модель `grok-4-1-fast-reasoning`.
- `response_format: { type: "json_schema", strict: true, json_schema: { schema: ... } }`.
- 401/429/5xx → exponential backoff retry (3 попытки, base 1s).
- 429 → Tauri event `cloud-rate-limited` с retry_after из header.
- CI test: mock xAI server, request body shape matches schema.

**AC-2 (Local pipeline)**
- `orchestrator.rs` выбирает one_shot / map_reduce / hierarchical по §3.3.
- Все 8 expert prompts загружаются по call_type.
- Action item post-pass всегда запускается для local.
- Validation failure (substring < 90%) → item dropped, rest сохраняется.
- Progress events emit каждые ≤ 2 сек.

**AC-3 (Data model)**
- Миграции 0007, 0008 применяются на свежей БД и БД с существующими записями.
- `summary_repo.rs` пишет/читает все новые поля.
- Старая запись (schema_version=1) → Rust struct с пустыми массивами decisions/open_questions.

**AC-4 (UI)**
- CallTypeBadge для всех schema_version=2.
- PrivacyDisclaimer для call_type=one_on_one.
- EvidenceTooltip — hover quote, click → scroll к timestamp в STT view.
- ActionItemRow рендерит confidence badges per §7.2.
- LegacySummaryView не падает на старых саммари.

**AC-5 (Quality gates)**
- Golden set 10+ в `tests/golden/`.
- CI запускает `pipeline_e2e.rs` на каждый PR.
- 7 automated checks (§8.2) зелёные на 90% golden set.
- Schema validation 100%.

**AC-6 (Performance)**
- 45-минутный звонок, Qwen 7B Q4_K_M, M2 Pro 16GB, ctx 16K → end-to-end ≤ 3 мин (включая model load). Math: ~8,800 input tokens @ ~250 PP t/s ≈ 35s ingest + ~3,000 output @ 28 TG t/s ≈ 107s gen + ~10s load.
- 2-часовой звонок, Qwen 7B, map-reduce → ≤ 12 мин.
- Cloud Grok-4.1-Fast, 45-мин → ≤ 60 сек.

**AC-7 (Multilingual)**
- ru/en/kk транскрипты классифицируются и саммируются в исходном языке.
- Code-switching ru/en не ломает JSON.

**AC-8 (Privacy)**
- one_on_one: evidence.quote = null или omitted.
- one_on_one: action_items только work commitments.

**AC-9 (Backward compat)**
- Re-process button на старых саммари.
- Старые рендерятся без потерь.

**AC-10 (Feature flag)**
- `flag.pipeline_v2` корректно gate'ит новый pipeline.
- OFF → старый recap.rs identical pre-PRD.

---

## 11. Tasks breakdown for Claude Code

| ID | Priority | Title | Files | Complexity | Depends on |
|---|---|---|---|---|---|
| T-01 | P0 | DB migrations + types Rust/TS | migrations/, types.rs, summary.ts | S | — |
| T-02 | P0 | xAI Grok cloud client + universal prompt | cloud.rs, recap.rs, prompts/cloud_universal.txt | M | T-01 |
| T-03 | P0 | Validator (substring, schema, dedup) | validator.rs | M | T-01 |
| T-04 | P0 | Local classifier + ChatML runner | local_engine/llm.rs, llama_runner.rs, prompts/classify.txt | M | T-01 |
| T-05 | P0 | Chunker (topic-tile + speaker-turn) | chunker.rs | L | T-01 |
| T-06 | P0 | Map / Reduce prompts + runtime | prompts/map_pass.txt, reduce.txt, orchestrator.rs | L | T-04, T-05 |
| T-07 | P0 | 8 expert prompts + selection | prompts/expert/*.txt, orchestrator.rs | M | T-04 |
| T-08 | P0 | Action-item post-pass | prompts/action_items_postpass.txt, orchestrator.rs | M | T-04 |
| T-09 | P1 | GBNF grammar + fallback | grammars/call_summary_v2.gbnf, llama_runner.rs | M | T-04 |
| T-10 | P0 | Orchestrator e2e + progress events | orchestrator.rs, pipeline/mod.rs | L | T-02..T-08 |
| T-11 | P0 | UI v2 components + legacy adapter | SummaryView.tsx, CallTypeBadge, DecisionsBlock, etc | L | T-01 |
| T-12 | P0 | Golden set + CI eval harness | tests/golden/*, pipeline_e2e.rs | M | T-10 |
| T-13 | P1 | LLM-as-judge offline eval | scripts/eval_g_eval.ts | M | T-12 |
| T-14 | P1 | Feature flag + rollout | featureFlags.ts, settings UI | S | T-10, T-11 |
| T-15 | P1 | Re-process button | SummaryActions.tsx, orchestrator.rs | S | T-10, T-11 |
| T-16 | P2 | Speculative decoding (0.5B draft) | llama_runner.rs | S | T-04 |
| T-17 | P2 | Title regen trigger | prompts/title_regen.txt, orchestrator.rs | S | T-10 |
| T-18 | P2 | Hierarchical pipeline (3-level) | orchestrator.rs, chunker.rs | M | T-06 |

Implementation order:
```
T-01 → (T-02, T-03, T-04) → T-05 → T-06 → T-07 → T-08 → T-10 → T-12 → AC verification
T-11 parallel with T-06..T-08
T-09, T-13..T-18 — backlog after P0 ships
```

Total estimate: 3-4 weeks P0 (1 senior engineer + Claude Code), +1 week P1, P2 backlog.

---

## 12. Open questions / decisions to make during implementation

1. **Topic segmentation source.** Lexical TextTiling (Hearst 1997, быстро, no deps) vs embedding-based (paraphrase-multilingual-mpnet-base-v2 через candle, +200MB dep). Solbiati et al. 2021 показывает BERT-embeddings лучше TextTiling на AMI/ICSI. **Decision:** start lexical → измерить на golden → переключить на embeddings если segmentation accuracy < 70%.

2. **GBNF для всех runs?** Тест: GBNF vs no-GBNF на Qwen 7B — overhead в tokens/sec и parse error reduction. Если grammar даёт < 5% reduction при > 15% slowdown — оставить только как fallback.

3. **Streaming partial JSON?** llama.cpp поддерживает stream, но при prefill `{"schema_version":2,"title":"` пользователь видит сырой JSON. **Decision:** НЕ streaming JSON; streaming сделаем для финального markdown MoM в отдельной фазе.

4. **Cloud cost cap.** Hard limit `max_input_tokens=200K` для cloud → если больше, refuse with explainer "transcript too long, use Quality preset locally with hierarchical".

5. **Embedding-based dedup для cross-chunk action items.** Сейчас полагаемся на LLM в reduce. Альтернатива — sentence embeddings + cosine threshold 0.85. **Backlog.**

6. **MCP integration для поиска по архиву.** Вне scope, но schema_version=2 поля (call_type, evidence_quote) — это и есть индекс для будущего MCP. Готовим почву.

7. **Speaker diarization confidence.** STT (Soniox/Gladia) даёт speaker tags без uncertainty. Per Lanzendörfer et al., "Benchmarking Diarization Models" (arXiv:2509.26177, 2025) — best models достигают 11.2% DER (PyannoteAI) и 13.3% DER (DiariZen open-source) на 196.6 часов multilingual audio. Для one_on_one и interviews критично — рассмотреть отдельный confidence-pass где LLM проверяет attribution на критических цитатах.

8. **Casing/punctuation для kk.** Qwen 2.5 слабее на kk чем на ru/en. Если quality для kk провалится в golden — добавить few-shot kk examples в expert prompts.

9. **Кэширование cloud-промптов.** xAI prompt caching — static system prompt должен идти первым. Verify: наш system_prompt стабильный (single-pass cloud, static system → 85% input cache hit expected → input cost эффективно $0.05/M cached).

10. **MoM markdown rendering.** Markdown vs structured blocks в DB? **Decision:** хранить markdown как есть в `mom` поле; рендерить через `react-markdown` с custom block-quote styling для evidence quotes.

---

## Caveats

- **M2 Pro 16GB benchmark numbers — extrapolated, не measured.** Token generation 30-35 t/s, prompt processing 250-290 t/s — bracketed из llama.cpp Discussion #4167 (LLaMA-7B v2 baseline) + KV/bandwidth math. Перед production claims в маркетинге — измерить на target hw через `llama-bench -m qwen2.5-7b-instruct-q4_k_m.gguf -p 8192,16384 -n 128 -ngl 99 -fa 1 -ctk q8_0 -ctv q8_0`. 19-core GPU variant даёт +10-15% PP, ~0% TG (TG bandwidth-bound).
- **Grok-4.1-Fast hallucination — 19.2% на Vectara HHEM-2.3 New Dataset (один из худших frontier).** Substring verification обязательна. Если в production окажется выше — рассмотреть Grok-4.3 ($1.25/$2.50 per M, hallucination ниже но цена в 6× выше) или Claude Opus 4.5 для critical paths.
- **Annotator agreement на action items низкий** (Kappa ~0.36 per Liu et al.). Категоризация "commitment / proposal / idea" вместо binary — компромисс с этим human disagreement.
- **CoT для small LLM**: per BRIDGE benchmark arXiv:2509.21933, 82/95 моделей деградируют с CoT. Соблюдаем describe-then-output strictly, без CoT.
- **G-Eval correlation с human = 0.514 Spearman** (Liu et al. EMNLP 2023, GPT-4 backbone) — не идеально. KPI thresholds в §8.3 калибруем после первого реального прогона на 30+ звонках с human review.
- **Hallucination даже на empty input**: per Belem et al. arXiv:2410.13961 (Megagon Labs) — gpt-3.5-turbo 79.35% и GPT-4o 44% генерируют summary content для non-existent topic. Эта же тенденция есть у Qwen — поэтому substring verification mandatory.
- **Lost-in-the-middle** (Liu et al. TACL 2024): не используем монолитный prompt для long transcripts даже если formally влезает в window — map-reduce даёт более ровное покрытие.

---

*Конец PRD. Готово к копи-пасту в Claude Code.*