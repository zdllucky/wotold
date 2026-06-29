// [B18.7b] Unit tests for the Inbox person facet predicate in matchesFacets.

import { describe, expect, test } from 'vitest';
import type { Call } from '../api/recording';
import { FACETS_EMPTY, matchesFacets } from './inboxData';

function mkCall(id: string, overrides: Partial<Call> = {}): Call {
  return {
    id,
    title: `Call ${id}`,
    started_at: new Date(2026, 0, 1).toISOString(),
    ended_at: null,
    duration_sec: 600,
    status: 'ready',
    provider: null,
    path_label: '',
    lang_detected: null,
    failed_reason: null,
    recap_failed_reason: null,
    pipeline_step: null,
    pipeline_pct: null,
    pipeline_eta_sec: null,
    upload_bytes: null,
    paused_at: null,
    paused_total_ms: 0,
    processing_via: null,
    call_type: null,
    call_type_confidence: null,
    summary_schema_version: null,
    summary_engine: null,
    summary_pipeline_mode: null,
    created_at: '',
    updated_at: '',
    ...overrides,
  };
}

describe('matchesFacets — person facet', () => {
  const persons = new Map<string, string[]>([
    ['c1', ['Алиса Иванова']],
    ['c2', ['Боб Петров']],
    ['c3', ['Алиса Иванова', 'Боб Петров']],
  ]);

  test('empty person facet is inactive — every call passes', () => {
    expect(matchesFacets(mkCall('c1'), FACETS_EMPTY, '', persons)).toBe(true);
    expect(matchesFacets(mkCall('zzz'), FACETS_EMPTY, '', persons)).toBe(true);
  });

  test('filters to calls whose confirmed participants include the value', () => {
    const f = { ...FACETS_EMPTY, person: ['Алиса Иванова'] };
    expect(matchesFacets(mkCall('c1'), f, '', persons)).toBe(true);
    expect(matchesFacets(mkCall('c2'), f, '', persons)).toBe(false);
    expect(matchesFacets(mkCall('c3'), f, '', persons)).toBe(true);
  });

  test('OR semantics across multiple selected people', () => {
    const f = { ...FACETS_EMPTY, person: ['Алиса Иванова', 'Боб Петров'] };
    expect(matchesFacets(mkCall('c1'), f, '', persons)).toBe(true);
    expect(matchesFacets(mkCall('c2'), f, '', persons)).toBe(true);
  });

  test('a call with no confirmed participants (or no map) fails an active person filter', () => {
    const f = { ...FACETS_EMPTY, person: ['Алиса Иванова'] };
    expect(matchesFacets(mkCall('unknown'), f, '', persons)).toBe(false);
    expect(matchesFacets(mkCall('c1'), f, '', undefined)).toBe(false);
  });
});
