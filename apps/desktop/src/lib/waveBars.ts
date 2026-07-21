// [UI-fix A] Адаптивное число баров волновой дорожки плеера.
//
// Раньше AudioScrubber рендерил фикс. 130 баров `flex: 1 1 0` — на узком окне
// каждый бар получал суб-пиксельную ширину и «съезжал» (часть баров исчезала,
// зазоры плясали). Теперь count выводится из реальной ширины контейнера:
// ~4.5px на бар+зазор (3px бар + 1.5px gap из wk.css .player-wave) — бары
// держат стабильную визуальную плотность на любой ширине.

export const DEFAULT_WAVE_BARS = 130;

/** px на один бар + зазор (3px бар + 1.5px gap wk.css:352-353). */
const PX_PER_BAR = 4.5;
const MIN_BARS = 40;
const MAX_BARS = 180;

/** Число баров для ширины контейнера. width<=0 (нет замера) → DEFAULT. */
export function waveBarCount(width: number): number {
  if (!Number.isFinite(width) || width <= 0) return DEFAULT_WAVE_BARS;
  return Math.min(MAX_BARS, Math.max(MIN_BARS, Math.round(width / PX_PER_BAR)));
}
