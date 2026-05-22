export type FailureKind =
  | 'model_missing'
  | 'timeout'
  | 'low_resources'
  | 'broken_recording'
  | 'generic';

export function mapFailureToUxKind(reason: string | null): FailureKind {
  if (!reason) return 'generic';
  if (reason.includes('model_missing')) return 'model_missing';
  if (reason.includes('timeout')) return 'timeout';
  if (reason.includes('oom') || reason.includes('low_resources')) return 'low_resources';
  if (reason.includes('local_audio_decode_failed') || reason.includes('ERR_INVALID_FORMAT'))
    return 'broken_recording';
  return 'generic';
}
