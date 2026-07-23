// [B18.7b] Unit tests for the Inbox person facet predicate in matchesFacets.

import { describe, expect, test } from 'vitest';
import type { Call } from '../api/recording';
import { FACETS_EMPTY, confirmedParticipants, matchesFacets } from './inboxData';

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

describe('matchesFacets — custom date range (B19.3)', () => {
  // Call started June 15 2026 (local noon).
  const onJun15 = mkCall('c', { started_at: new Date(2026, 5, 15, 12, 0, 0).toISOString() });
  const range = (from: string | null, to: string | null) => ({
    ...FACETS_EMPTY,
    range: { from, to },
  });

  test('empty range is a no-op', () => {
    expect(matchesFacets(onJun15, FACETS_EMPTY, '')).toBe(true);
    expect(matchesFacets(onJun15, range(null, null), '')).toBe(true);
  });

  test('inside an inclusive from..to range passes', () => {
    expect(matchesFacets(onJun15, range('2026-06-10', '2026-06-20'), '')).toBe(true);
    // Boundary days are inclusive (00:00 from … 23:59 to).
    expect(matchesFacets(onJun15, range('2026-06-15', '2026-06-15'), '')).toBe(true);
  });

  test('before `from` or after `to` is excluded', () => {
    expect(matchesFacets(onJun15, range('2026-06-16', null), '')).toBe(false);
    expect(matchesFacets(onJun15, range(null, '2026-06-14'), '')).toBe(false);
  });

  test('open-ended bounds (from-only / to-only)', () => {
    expect(matchesFacets(onJun15, range('2026-06-01', null), '')).toBe(true);
    expect(matchesFacets(onJun15, range(null, '2026-06-30'), '')).toBe(true);
  });

  test('an unparseable bound fails closed (excludes, not widens)', () => {
    expect(matchesFacets(onJun15, range('not-a-date', null), '')).toBe(false);
  });
});

// [B29.1] Дедуп аватаров участников по контакту.
describe('confirmedParticipants', () => {
  const row = (
    contact_id: string | null,
    name: string | null,
    confirmed = true,
  ) => ({ confirmed, contact_id, contact_display_name: name });

  test('несколько тегов одного контакта схлопываются в один аватар', () => {
    const r = confirmedParticipants([
      row('c1', 'Дамир Нуртазин'),
      row('c1', 'Дамир Нуртазин'),
      row('c2', 'Ренат Буланов'),
    ]);
    expect(r.initials).toEqual(['ДН', 'РБ']);
    expect(r.names).toEqual(['Дамир Нуртазин', 'Ренат Буланов']);
  });

  test('contact_id null — дедуп по имени', () => {
    const r = confirmedParticipants([row(null, 'Гость Гость'), row(null, 'Гость Гость')]);
    expect(r.initials).toEqual(['ГГ']);
  });

  test('разные контакты с одинаковыми инициалами остаются двумя', () => {
    const r = confirmedParticipants([row('a', 'Дана Дулатова'), row('b', 'Диас Досжан')]);
    expect(r.initials).toEqual(['ДД', 'ДД']);
  });

  test('неподтверждённые и безымянные пропускаются', () => {
    const r = confirmedParticipants([row('a', 'Имя Фамилия', false), row('b', null)]);
    expect(r.initials).toEqual([]);
  });
});
