// [UI-fix C] weekGrid — hourRange / packDayEvents / callToDayEvent.

import { describe, expect, it } from 'vitest';
import type { Call } from '../api/recording';
import {
  callToDayEvent,
  hourRange,
  MIN_SLOT_MIN,
  packDayEvents,
  type DayEvent,
} from './weekGrid';

const ev = (id: string, startMin: number, endMin: number): DayEvent => ({
  id,
  startMin,
  endMin,
});

describe('hourRange', () => {
  it('defaults to 8..19 when no events', () => {
    expect(hourRange([])).toEqual({ startHour: 8, endHour: 19 });
  });

  it('extends start down to the earliest event floor hour', () => {
    // 7:15 → floor 7.
    expect(hourRange([ev('a', 7 * 60 + 15, 8 * 60)])).toEqual({
      startHour: 7,
      endHour: 19,
    });
  });

  it('extends end up to the latest event ceil hour', () => {
    // 21:10–22:05 → ceil(22:05) = 23.
    expect(hourRange([ev('a', 21 * 60 + 10, 22 * 60 + 5)])).toEqual({
      startHour: 8,
      endHour: 23,
    });
  });

  it('exact-hour end does not bump the ceiling', () => {
    // 20:00–22:00 → end 22 (без бампа до 23).
    expect(hourRange([ev('a', 20 * 60, 22 * 60)])).toEqual({
      startHour: 8,
      endHour: 22,
    });
  });

  it('clamps to [0, 24]', () => {
    expect(hourRange([ev('a', 0, 30), ev('b', 23 * 60 + 30, 1440)])).toEqual({
      startHour: 0,
      endHour: 24,
    });
  });

  it('accounts for MIN_SLOT visual extent of short late events', () => {
    // 18:50 + 5 мин звонок, но визуально 40 мин → до 19:30 → end 20.
    expect(hourRange([ev('a', 18 * 60 + 50, 18 * 60 + 55)])).toEqual({
      startHour: 8,
      endHour: 20,
    });
  });
});

describe('packDayEvents', () => {
  it('empty input → empty output', () => {
    expect(packDayEvents([])).toEqual([]);
  });

  it('single event gets lane 0 of 1', () => {
    const [p] = packDayEvents([ev('a', 540, 600)]);
    expect(p).toMatchObject({ laneIdx: 0, laneCount: 1 });
  });

  it('disjoint events form separate clusters, both full-width', () => {
    const out = packDayEvents([ev('a', 540, 600), ev('b', 700, 760)]);
    expect(out.map((p) => [p.laneIdx, p.laneCount])).toEqual([
      [0, 1],
      [0, 1],
    ]);
  });

  it('overlapping pair splits into 2 lanes', () => {
    const out = packDayEvents([ev('a', 540, 620), ev('b', 560, 640)]);
    expect(out.find((p) => p.id === 'a')).toMatchObject({ laneIdx: 0, laneCount: 2 });
    expect(out.find((p) => p.id === 'b')).toMatchObject({ laneIdx: 1, laneCount: 2 });
  });

  it('chain A(9-10) B(9:30-10:30) C(10-11): one cluster, C reuses lane 0', () => {
    const out = packDayEvents([
      ev('a', 540, 600),
      ev('b', 570, 630),
      ev('c', 600, 660),
    ]);
    const byId = Object.fromEntries(out.map((p) => [p.id, p]));
    expect(byId.a).toMatchObject({ laneIdx: 0, laneCount: 2 });
    expect(byId.b).toMatchObject({ laneIdx: 1, laneCount: 2 });
    expect(byId.c).toMatchObject({ laneIdx: 0, laneCount: 2 });
  });

  it('short events overlap via minSlot visual extent', () => {
    // Оба по 5 минут, второй стартует через 10 — фактически не пересекаются,
    // но визуальные слоты (40 мин) пересекаются → 2 lanes.
    const out = packDayEvents([ev('a', 540, 545), ev('b', 550, 555)]);
    expect(out.every((p) => p.laneCount === 2)).toBe(true);
  });

  it('handles unsorted input deterministically', () => {
    const out = packDayEvents([ev('b', 560, 640), ev('a', 540, 620)]);
    expect(out.map((p) => p.id)).toEqual(['a', 'b']);
    expect(out.find((p) => p.id === 'a')?.laneIdx).toBe(0);
  });
});

describe('callToDayEvent', () => {
  const call = (started_at: string, duration_sec: number | null): Call =>
    ({ id: 'c1', started_at, duration_sec }) as Call;

  it('maps local start time + duration to minutes', () => {
    const d = new Date(2026, 6, 1, 14, 29, 0);
    const e = callToDayEvent(call(d.toISOString(), 25 * 60));
    expect(e.startMin).toBe(14 * 60 + 29);
    expect(e.endMin).toBe(14 * 60 + 29 + 25);
  });

  it('clamps end at midnight', () => {
    const d = new Date(2026, 6, 1, 23, 50, 0);
    const e = callToDayEvent(call(d.toISOString(), 3600));
    expect(e.endMin).toBe(1440);
  });

  it('null duration yields minimal 1-minute event', () => {
    const d = new Date(2026, 6, 1, 9, 0, 0);
    const e = callToDayEvent(call(d.toISOString(), null));
    expect(e.endMin).toBe(e.startMin + 1);
  });

  it('MIN_SLOT_MIN constant sanity', () => {
    expect(MIN_SLOT_MIN).toBeGreaterThan(0);
  });
});
