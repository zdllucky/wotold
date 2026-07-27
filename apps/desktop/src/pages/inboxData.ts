// [B18.2a] Inbox data helpers — ported 1-to-1 from the interim CallsPage so the
// behaviour (month grouping, state derivation, virtualization threshold, speaker
// fallback) is unchanged. Pure functions only; JSX lives in InboxView.tsx.

import { bcp47 } from '../i18n';
import type { Call } from '../api/recording';
import type { CallState } from '../types/callState';
import type { IconName } from '../ui/Icon';

export const VIRTUALIZATION_THRESHOLD = 200;
export const ROW_HEIGHT = 72;
export const VIRTUAL_LIST_HEIGHT = 600;

// Speaker thread colors — canonical Wotold v2 token palette (--sp1..5).
export { SP_COLORS } from './CallDetailUtils';

export function declinePlural(n: number, forms: [string, string, string]): string {
  const abs = Math.abs(n) % 100;
  const tail = abs % 10;
  if (abs >= 11 && abs <= 14) return forms[2];
  if (tail === 1) return forms[0];
  if (tail >= 2 && tail <= 4) return forms[1];
  return forms[2];
}

function monthLabel(d: Date, locale: string): string {
  return d.toLocaleDateString(bcp47(locale as Parameters<typeof bcp47>[0]), {
    month: 'long',
    year: 'numeric',
  });
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export interface MonthGroup {
  label: string;
  calls: Call[];
}

export function groupByMonth(calls: Call[], locale: string): MonthGroup[] {
  const map = new Map<string, MonthGroup>();
  for (const c of calls) {
    const dt = new Date(c.started_at);
    if (!Number.isFinite(dt.getTime())) continue;
    const key = `${dt.getFullYear()}-${dt.getMonth()}`;
    let g = map.get(key);
    if (!g) {
      g = { label: capitalize(monthLabel(dt, locale)), calls: [] };
      map.set(key, g);
    }
    g.calls.push(c);
  }
  return Array.from(map.values());
}

export function matchesQuery(c: Call, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  const haystack = [
    c.title ?? '',
    c.provider ?? '',
    c.lang_detected ?? '',
    c.failed_reason ?? '',
    c.id.slice(0, 8),
  ]
    .join(' ')
    .toLowerCase();
  return haystack.includes(needle);
}

/** DB status → UI state. step=1 (upload) → 'uploading'. */
export function deriveCallState(call: Call): CallState {
  if (call.status === 'recording') return 'live';
  if (call.status === 'failed') return 'error';
  if (call.status === 'ready') return 'ready';
  if (call.pipeline_step === 1) return 'uploading';
  return 'processing';
}

// ── Facets (status / recap / period / person — B18.7b) ──

export type StatusFacet = 'ready' | 'processing' | 'error';
export type RecapFacet = 'yes' | 'no';
export type PeriodFacet = 'today' | 'week';
/** Person facet values are confirmed-contact display names (dynamic). */
export type PersonFacet = string;

/** Custom date range (inclusive). ISO `YYYY-MM-DD` (local calendar days) or null. */
export interface DateRange {
  from: string | null;
  to: string | null;
}

export interface Facets {
  status: StatusFacet[];
  recap: RecapFacet[];
  period: PeriodFacet[];
  person: PersonFacet[];
  range: DateRange;
}

export const FACETS_EMPTY: Facets = {
  status: [],
  recap: [],
  period: [],
  person: [],
  range: { from: null, to: null },
};

/** Facet keys backed by a string[] (everything except the `range` object). */
export type StrFacetKey = 'status' | 'recap' | 'period' | 'person';
export const STR_FACET_KEYS: StrFacetKey[] = ['status', 'recap', 'period', 'person'];

export function hasRange(f: Facets): boolean {
  return Boolean(f.range.from || f.range.to);
}

export function facetCount(f: Facets): number {
  return (
    f.status.length +
    f.recap.length +
    f.period.length +
    f.person.length +
    (hasRange(f) ? 1 : 0)
  );
}

/** Toggle a string[]-valued facet (status/recap/period/person). */
export function toggleFacet(
  f: Facets,
  k: 'status' | 'recap' | 'period' | 'person',
  v: string,
): Facets {
  const arr = f[k] as string[];
  const next = arr.includes(v) ? arr.filter((x) => x !== v) : [...arr, v];
  return { ...f, [k]: next } as Facets;
}

/** Set/clear one bound of the custom date range. */
export function setRange(f: Facets, patch: Partial<DateRange>): Facets {
  return { ...f, range: { ...f.range, ...patch } };
}

export function callStatusFacet(c: Call): StatusFacet {
  if (c.status === 'failed') return 'error';
  if (c.status === 'ready') return 'ready';
  return 'processing'; // recording | processing
}

export function callHasRecap(c: Call): boolean {
  return c.status === 'ready' && c.recap_failed_reason == null;
}

function withinPeriod(c: Call, periods: PeriodFacet[]): boolean {
  const t = new Date(c.started_at).getTime();
  if (!Number.isFinite(t)) return false;
  const now = Date.now();
  const startOfToday = new Date();
  startOfToday.setHours(0, 0, 0, 0);
  if (periods.includes('today') && t >= startOfToday.getTime()) return true;
  if (periods.includes('week') && t >= now - 7 * 24 * 3600 * 1000) return true;
  return false;
}

export function matchesFacets(
  c: Call,
  f: Facets,
  text: string,
  callPersons?: ReadonlyMap<string, string[]>,
): boolean {
  if (f.status.length && !f.status.includes(callStatusFacet(c))) return false;
  if (f.recap.length && !f.recap.includes(callHasRecap(c) ? 'yes' : 'no')) return false;
  if (f.period.length && !withinPeriod(c, f.period)) return false;
  if (hasRange(f) && !withinRange(c, f.range)) return false;
  if (f.person.length) {
    const people = callPersons?.get(c.id);
    if (!people || !f.person.some((p) => people.includes(p))) return false;
  }
  if (text && !matchesQuery(c, text)) return false;
  return true;
}

/** Inclusive custom range: local-day `from` 00:00 … `to` 23:59:59.999. */
function withinRange(c: Call, range: DateRange): boolean {
  const t = new Date(c.started_at).getTime();
  if (!Number.isFinite(t)) return false;
  if (range.from) {
    const from = new Date(`${range.from}T00:00:00`).getTime();
    // Fail-closed: an unparseable bound excludes rather than silently widening.
    if (!Number.isFinite(from) || t < from) return false;
  }
  if (range.to) {
    const to = new Date(`${range.to}T23:59:59.999`).getTime();
    if (!Number.isFinite(to) || t > to) return false;
  }
  return true;
}

// ── Formatters ──

export function formatDuration(sec: number | null): string {
  if (sec == null) return '—';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }
  return `${m}:${s.toString().padStart(2, '0')}`;
}

export function formatDay(iso: string): string {
  try {
    return new Date(iso).getDate().toString().padStart(2, '0');
  } catch {
    return iso;
  }
}

export function formatMegabytes(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb < 1) return `${(bytes / 1024).toFixed(0)} КБ`;
  return `${mb.toFixed(1)} МБ`;
}

// ── Speaker initials ──

/** Deterministic placeholder when confirmed speakers aren't loaded. */
export function inferSpeakers(call: Call): string[] {
  const sec = call.duration_sec ?? 0;
  const guess = sec < 300 ? 1 : sec < 1800 ? 2 : 3;
  const hash = [...call.id].reduce(
    (acc, ch) => (acc * 31 + ch.charCodeAt(0)) | 0,
    0,
  );
  const letters = 'АБВГДЕЖЗИКЛМНОПРСТУФХЦЧШЩЮЯ';
  const out: string[] = [];
  for (let i = 0; i < guess; i++) {
    const a = letters[Math.abs(hash + i * 7) % letters.length];
    const b = letters[Math.abs(hash + i * 13 + 5) % letters.length];
    out.push(`${a}${b}`);
  }
  return out;
}

/// [B29.1] Confirmed-участники звонка БЕЗ дублей: несколько speaker-тегов
/// одного контакта («Д Д») схлопываются по contact_id (fallback — имя).
/// Дедуп строго по ключу, НЕ по инициалам: два разных контакта с равными
/// инициалами остаются двумя аватарами.
export interface ConfirmedParticipants {
  initials: string[];
  names: string[];
}

export function confirmedParticipants(
  rows: ReadonlyArray<{
    confirmed: boolean;
    contact_id: string | null;
    contact_display_name: string | null;
  }>,
): ConfirmedParticipants {
  const seen = new Set<string>();
  const out: ConfirmedParticipants = { initials: [], names: [] };
  for (const s of rows) {
    if (!s.confirmed || !s.contact_display_name) continue;
    const key = s.contact_id ?? s.contact_display_name;
    if (seen.has(key)) continue;
    seen.add(key);
    out.initials.push(initials(s.contact_display_name));
    out.names.push(s.contact_display_name);
  }
  return out;
}

export function initials(name: string): string {
  return (
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('') || '·'
  );
}

/** [TD-49] Описание фасета для омни-бара. Жил в `InboxView`, поднят сюда,
 *  когда омни-бар переехал в свой модуль и стал вторым потребителем. */
export interface FacetDef {
  key: StrFacetKey;
  label: string;
  icon: IconName;
  values: { v: string; label: string }[];
}
