import type { DiarizedTranscript, TranscriptSegment } from '@wotold/contracts';

// Soniox async transcription API:
//   POST /v1/transcriptions  body {audio_url, model, enable_speaker_diarization, ...}
//     → {id}
//   GET  /v1/transcriptions/{id}            → {status, ...}
//   GET  /v1/transcriptions/{id}/transcript → {tokens, language, duration_ms, ...}
// Auth: Authorization: Bearer <apiKey>

export const SONIOX_BASE_URL = 'https://api.soniox.com/v1';
const DEFAULT_MODEL = 'stt-async-preview';
const POLL_INTERVAL_MS = 1500;

export interface SonioxOpts {
  apiKey: string;
  audioUrl: string;
  lang: 'auto' | string;
  /** Дедлайн (epoch ms) до которого ждём результат. Workers free CPU ≈ 30s. */
  pollDeadlineMs: number;
  /** Test override. */
  baseUrl?: string;
  model?: string;
  /** Test injection of polling delay. */
  sleep?: (ms: number) => Promise<void>;
}

interface SonioxToken {
  text: string;
  start_ms?: number;
  end_ms?: number;
  confidence?: number;
  speaker?: number;
  language?: string;
}

interface SonioxTranscript {
  text?: string;
  language?: string;
  duration_ms?: number;
  tokens?: SonioxToken[];
}

export async function transcribeSoniox(opts: SonioxOpts): Promise<DiarizedTranscript> {
  const base = opts.baseUrl ?? SONIOX_BASE_URL;
  const sleep = opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
  const headers = {
    authorization: `Bearer ${opts.apiKey}`,
    'content-type': 'application/json',
  } as const;

  // 1. Создаём transcription job.
  const createBody: Record<string, unknown> = {
    audio_url: opts.audioUrl,
    model: opts.model ?? DEFAULT_MODEL,
    enable_speaker_diarization: true,
    enable_language_identification: opts.lang === 'auto',
  };
  if (opts.lang !== 'auto') {
    createBody.language_hints = [opts.lang];
    createBody.language_hints_strict = true;
  }

  const createResp = await fetch(`${base}/transcriptions`, {
    method: 'POST',
    headers,
    body: JSON.stringify(createBody),
  });
  if (!createResp.ok) {
    throw new Error(`soniox create ${createResp.status}: ${await safeText(createResp)}`);
  }
  const { id } = (await createResp.json()) as { id: string };

  // 2. Polling до completed/failed/deadline.
  while (Date.now() < opts.pollDeadlineMs) {
    await sleep(POLL_INTERVAL_MS);
    const statusResp = await fetch(`${base}/transcriptions/${id}`, {
      headers: { authorization: headers.authorization },
    });
    if (!statusResp.ok) {
      throw new Error(`soniox status ${statusResp.status}: ${await safeText(statusResp)}`);
    }
    const status = (await statusResp.json()) as { status: string };
    if (status.status === 'completed') break;
    if (status.status === 'failed') {
      throw new Error('soniox transcription failed');
    }
  }

  // 3. Забираем результат.
  const transcriptResp = await fetch(`${base}/transcriptions/${id}/transcript`, {
    headers: { authorization: headers.authorization },
  });
  if (!transcriptResp.ok) {
    throw new Error(`soniox transcript ${transcriptResp.status}: ${await safeText(transcriptResp)}`);
  }
  const transcript = (await transcriptResp.json()) as SonioxTranscript;

  return normalizeSoniox(transcript);
}

/**
 * Группирует подряд идущие токены одного спикера в TranscriptSegment.
 * Если speaker не задан на токенах — всё уходит в один сегмент Speaker 0.
 */
export function normalizeSoniox(t: SonioxTranscript): DiarizedTranscript {
  const segments: TranscriptSegment[] = [];
  let current: TranscriptSegment | null = null;

  for (const tok of t.tokens ?? []) {
    const speakerTag = tok.speaker != null ? `Speaker ${tok.speaker}` : 'Speaker 0';
    const startMs = tok.start_ms ?? 0;
    const endMs = tok.end_ms ?? startMs;

    if (current && current.speakerTag === speakerTag) {
      current.text += tok.text;
      current.end = endMs / 1000;
    } else {
      current = {
        start: startMs / 1000,
        end: endMs / 1000,
        text: tok.text,
        speakerTag,
      };
      segments.push(current);
    }
  }

  return {
    version: 1,
    provider: 'soniox',
    langDetected: t.language ?? null,
    durationSec: (t.duration_ms ?? 0) / 1000,
    segments,
  };
}

async function safeText(resp: Response): Promise<string> {
  try {
    return (await resp.text()).slice(0, 500);
  } catch {
    return '<unreadable>';
  }
}
