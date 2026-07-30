export type FailureKind =
  | 'parked'
  | 'model_missing'
  | 'timeout'
  | 'low_resources'
  | 'broken_recording'
  | 'generic';

/**
 * Причины, по которым звонок припаркован из-за нехватки модулей движка.
 *
 * Список обязан совпадать с SQL-условием `db::list_parked_calls` на бэкенде:
 * там решают, какие звонки поднять после докачки, здесь — что показать
 * пользователю. Разойдутся — либо звонок молча висит «сломанным», хотя
 * поднимется сам, либо мы обещаем автоматику там, где её нет.
 */
const PARKED_MARKERS = [
  'local_engine_not_ready',
  'local_engine_model_missing',
  'local_engine_model_tampered',
  'local_engine_preset_not_set',
];

/** Ждёт ли звонок докачки модулей (а не сломан). */
export function isParkedFailure(reason: string | null): boolean {
  if (!reason) return false;
  return PARKED_MARKERS.some((m) => reason.includes(m));
}

export function mapFailureToUxKind(reason: string | null): FailureKind {
  if (!reason) return 'generic';
  // Парковка проверяется первой: `local_engine_model_missing` иначе уходил бы
  // в ветку «модель не установлена» с ручной установкой, хотя звонок
  // обработается сам после докачки.
  if (isParkedFailure(reason)) return 'parked';
  if (reason.includes('model_missing')) return 'model_missing';
  if (reason.includes('timeout')) return 'timeout';
  if (reason.includes('oom') || reason.includes('low_resources')) return 'low_resources';
  if (reason.includes('local_audio_decode_failed') || reason.includes('ERR_INVALID_FORMAT'))
    return 'broken_recording';
  return 'generic';
}
