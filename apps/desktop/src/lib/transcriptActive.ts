// [B20.9] Активная группа транскрипта по текущему времени плеера.
//
// Семантика границ: start inclusive, end exclusive — кроме последней группы
// (end inclusive, чтобы конец записи не «гас»). Общая граница смежных групп
// (g[i].end === g[i+1].start) резолвится в СЛЕДУЮЩУЮ группу: seek кликом по
// реплике ставит currentTime ровно в её start, и подсвечиваться должна она.

/** Компенсация float-clamp'а `<audio>.currentTime` после seek (12.3 → 12.299999). */
export const SEEK_EPS = 0.05;

export interface GroupRange {
  start: number;
  end: number;
}

/** Индекс группы, содержащей момент `t`, или -1 (gap / вне записи). */
export function findActiveGroupIdx(ranges: readonly GroupRange[], t: number): number {
  for (let i = ranges.length - 1; i >= 0; i--) {
    const g = ranges[i]!;
    if (g.start > t + SEEK_EPS) continue;
    const isLast = i === ranges.length - 1;
    const inGroup = isLast ? t <= g.end : t < g.end;
    return inGroup ? i : -1;
  }
  return -1;
}
