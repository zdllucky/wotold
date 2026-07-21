// [UI-fix C] Чистая математика Outlook-style недельной сетки: динамический
// часовой диапазон + lane-packing перекрывающихся событий (кластеры).
// Рендер — InboxWeek (InboxCalendarViews.tsx); здесь только данные,
// unit-тестируется без DOM.

import type { Call } from '../api/recording';

/** Событие в минутах локального дня. endMin ≤ 1440 (клип на полночь). */
export interface DayEvent {
  id: string;
  startMin: number;
  endMin: number;
}

/** Событие с назначенной lane внутри своего кластера перекрытий. */
export interface PositionedEvent extends DayEvent {
  /** Визуальный конец: max(endMin, startMin + minSlot) — по нему и пакуем. */
  effEndMin: number;
  laneIdx: number;
  laneCount: number;
}

/** Часовой диапазон сетки. endHour — эксклюзивная граница (последний ряд). */
export interface HourRange {
  startHour: number;
  endHour: number;
}

/** Визуальный минимум слота, мин — чтобы время+тайтл влезали в чип. */
export const MIN_SLOT_MIN = 40;
/** Высота часа, px. Фикс — родительский .scroll скроллит высокую сетку. */
export const HOUR_PX = 48;

const DEFAULT_START_HOUR = 8;
const DEFAULT_END_HOUR = 19;
const DAY_MIN = 1440;

/** Диапазон часов: покрывает [8,19) по умолчанию + все события недели. */
export function hourRange(events: DayEvent[]): HourRange {
  let start = DEFAULT_START_HOUR;
  let end = DEFAULT_END_HOUR;
  for (const e of events) {
    start = Math.min(start, Math.floor(e.startMin / 60));
    end = Math.max(end, Math.ceil(Math.max(e.endMin, e.startMin + MIN_SLOT_MIN) / 60));
  }
  return {
    startHour: Math.max(0, start),
    endHour: Math.min(24, end),
  };
}

/**
 * Lane-packing (Outlook): события сортируются по началу, жадно раскладываются
 * по lanes; кластер = связная компонента перекрытий по ВИЗУАЛЬНОМУ extent
 * (effEnd = max(end, start + minSlot)); внутри кластера ширина делится
 * поровну на число lanes кластера.
 */
export function packDayEvents(
  events: DayEvent[],
  minSlot: number = MIN_SLOT_MIN,
): PositionedEvent[] {
  if (events.length === 0) return [];
  const evs = events
    .map((e) => ({
      ...e,
      effEndMin: Math.min(DAY_MIN, Math.max(e.endMin, e.startMin + minSlot)),
      laneIdx: 0,
      laneCount: 1,
    }))
    .sort(
      (a, b) =>
        a.startMin - b.startMin || b.effEndMin - a.effEndMin || a.id.localeCompare(b.id),
    );

  const out: PositionedEvent[] = [];
  let lanes: number[] = []; // effEnd последней записи каждой lane
  let cluster: PositionedEvent[] = [];
  let clusterEnd = -Infinity;

  const flush = () => {
    for (const ev of cluster) ev.laneCount = lanes.length;
    out.push(...cluster);
    lanes = [];
    cluster = [];
    clusterEnd = -Infinity;
  };

  for (const ev of evs) {
    if (cluster.length > 0 && ev.startMin >= clusterEnd) flush();
    let laneIdx = lanes.findIndex((end) => end <= ev.startMin);
    if (laneIdx === -1) {
      lanes.push(ev.effEndMin);
      laneIdx = lanes.length - 1;
    } else {
      lanes[laneIdx] = ev.effEndMin;
    }
    ev.laneIdx = laneIdx;
    cluster.push(ev);
    clusterEnd = Math.max(clusterEnd, ev.effEndMin);
  }
  flush();
  return out;
}

/** Call → DayEvent: локальное время старта + длительность, клип на полночь. */
export function callToDayEvent(c: Call): DayEvent {
  const d = new Date(c.started_at);
  const startMin = d.getHours() * 60 + d.getMinutes();
  const durMin = Math.max(1, Math.round((c.duration_sec ?? 0) / 60));
  return {
    id: c.id,
    startMin,
    endMin: Math.min(DAY_MIN, startMin + durMin),
  };
}
