// DiarizedTranscript — общий формат вывода TranscriptionProvider.
// См. M2.1 паспорта. Изменения формата правятся здесь, потребляются
// и приложением, и прокси (S2).

export type SpeakerTag = string;

export interface TranscriptSegment {
  /** Start time within source audio, seconds (float). */
  start: number;
  /** End time within source audio, seconds (float). */
  end: number;
  /** Recognized text. */
  text: string;
  /** Diarization cluster tag. `owner` reserved for mic track (M2.4). */
  speakerTag: SpeakerTag;
  /** Provider confidence in [0, 1], optional. */
  confidence?: number;
}

export type TranscriptionProviderId = 'soniox' | 'gladia';

export interface DiarizedTranscript {
  /** Schema version. Bump on breaking changes. */
  version: 1;
  /** Detected language (BCP 47) or null. M2.6. */
  langDetected: string | null;
  /** Total source audio duration, seconds. */
  durationSec: number;
  /** Provider identifier. */
  provider: TranscriptionProviderId;
  /** Ordered speech segments across all speakers. */
  segments: TranscriptSegment[];
}
