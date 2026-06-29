// [recording] Живая аудио-дорожка: useAudioLevel + RecEq. Инкапсулирует
// подписку на audio:level и изолирует high-freq audio-перерендер В СЕБЕ —
// родительский контрол (навбар/рейл/минирейл) не перерисовывается на каждый
// аудио-сэмпл. (Родитель всё равно перерисовывается ~4×/сек от timer-тика
// `elapsed` в RecordingContext — но не от потока аудио-уровней.)
//
// Источник тот же, что у виджета: Swift-сайдкар шлёт audio:level каждые 100ms;
// RecEq берёт последние 3 сэмпла → высоты баров.

import { useAudioLevel } from '../hooks/useAudioLevel';
import { RecEq } from './RecEq';

interface LiveRecEqProps {
  /** На паузе бары застывают (подписка отключается). */
  paused?: boolean;
  /** Бары currentColor (белые на красных danger-кнопках). */
  inherit?: boolean;
}

export function LiveRecEq({ paused = false, inherit = false }: LiveRecEqProps) {
  const audio = useAudioLevel(!paused);
  // Без useMemo: audio.mic/system — новые ссылки на каждый тик (slice в хуке),
  // мемо не кэшировало бы; считаем напрямую.
  const levels = audio.mic.map((m, i) => Math.max(m, audio.system[i] ?? 0));
  return <RecEq paused={paused} levels={levels} inherit={inherit} />;
}
