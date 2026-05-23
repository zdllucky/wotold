// [M14 foundation] Type-driven evidence-grounded summary schema v2.
//
// Mirror Rust types из `apps/desktop/src-tauri/src/pipeline/summary_v2.rs`.
// Backbone для будущих фаз M14 (T-02 cloud schema-v2 prompts, T-11 UI v2
// components). Legacy `recap.ts` остаётся для existing single-pass recap.

/** 8 типов корпоративных звонков + `other` fallback. См. PRD §5.1. */
export type CallType =
  | 'sales_discovery'
  | 'sales_demo'
  | 'product_sync'
  | 'standup'
  | 'customer_interview'
  | 'one_on_one'
  | 'strategy_brainstorm'
  | 'status_update'
  | 'other';

/**
 * - `commitment` — explicit accept («I'll do X»).
 * - `proposal` — suggested но не accepted.
 * - `idea` — raised, no clear action assignment.
 */
export type ActionItemCategory = 'commitment' | 'proposal' | 'idea';

/** Engine label сохранённый в DB (calls.summary_engine). */
export type SummaryEngine =
  | 'local-qwen-1.5b'
  | 'local-qwen-3b'
  | 'local-qwen-7b'
  | 'cloud-groq'
  | 'cloud-anthropic'
  | 'cloud-xai-grok';

/** Pipeline mode исполнения (calls.summary_pipeline_mode). */
export type SummaryPipelineMode = 'one_shot' | 'map_reduce' | 'hierarchical';

/**
 * Substring-anchored evidence quote. `quote` обязательно verbatim substring
 * transcript'а (≥ 90% fuzzy match per Rust `summary_validator`). Если evidence
 * не найдётся — item drop'ается (degraded ok).
 */
export interface EvidenceAnchor {
  quote: string;
  speaker?: string;
  start_ms?: number;
  end_ms?: number;
}

export interface ActionItemV2 {
  id: string;
  text: string;
  owner_hint?: string;
  /** 0..1. ≥ 0.8 only при explicit accept (см. PRD §5.7 personal deixis warning). */
  owner_confidence?: number;
  /** ISO date OR human ("end of Q2"). Free-form. */
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

export interface ParticipantV2 {
  speaker_tag: string;
  display_name?: string;
  role_hint?: string;
}

/**
 * Полная V2 summary — то, что выдаёт future pipeline.
 *
 * Backward-compat: existing recaps имеют `schema_version=1` в DB и
 * рендерятся через legacy adapter (T-11). Новые pipeline runs пишут v2.
 */
export interface CallSummaryV2 {
  schema_version: 2;
  title: string;
  summary: string;
  key_points: string[];
  /** Markdown — type-specific structure (см. PRD §5.1 TYPE GUIDE). */
  mom: string;
  /** 'ru' | 'en' | 'kk' | 'mixed' — дублируется в `calls.lang_detected`. */
  language: string;
  call_type: CallType;
  call_type_confidence: number;
  participants: ParticipantV2[];
  action_items: ActionItemV2[];
  decisions: Decision[];
  open_questions: OpenQuestion[];
  /** JSON object с per-type structured data; null если call_type='other'. */
  type_specific_block?: Record<string, unknown> | null;
}

/** Constant export: список всех call types для UI selectors. */
export const CALL_TYPES: CallType[] = [
  'sales_discovery',
  'sales_demo',
  'product_sync',
  'standup',
  'customer_interview',
  'one_on_one',
  'strategy_brainstorm',
  'status_update',
  'other',
];

export const ACTION_ITEM_CATEGORIES: ActionItemCategory[] = [
  'commitment',
  'proposal',
  'idea',
];
