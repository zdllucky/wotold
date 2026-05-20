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
  /**
   * [B3]: при resume из KV-кэша — пропускаем create step и сразу идём в polling
   * по существующему job id. Защита от двойной оплаты при client retry.
   */
  existingJobId?: string;
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

export interface TranscribeSonioxResult {
  transcript: DiarizedTranscript;
  /** Soniox job id — клиент-сторона прокси кэширует в KV для resume на retry. */
  jobId: string;
  /** true если job создан в этом вызове (не resumed). */
  jobCreated: boolean;
}

export async function transcribeSoniox(opts: SonioxOpts): Promise<TranscribeSonioxResult> {
  const base = opts.baseUrl ?? SONIOX_BASE_URL;
  const sleep = opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
  const headers = {
    authorization: `Bearer ${opts.apiKey}`,
    'content-type': 'application/json',
  } as const;

  // 1. Resume или create transcription job.
  let id: string;
  let jobCreated = false;
  if (opts.existingJobId) {
    id = opts.existingJobId;
  } else {
    const createBody: Record<string, unknown> = {
      audio_url: opts.audioUrl,
      model: opts.model ?? DEFAULT_MODEL,
      enable_speaker_diarization: true,
      enable_language_identification: opts.lang === 'auto',
    };
    if (opts.lang !== 'auto') {
      createBody.language_hints = [opts.lang];
      createBody.language_hints_strict = true;
    } else {
      // [Lang-tuning] auto-detect biased toward common languages — не strict,
      // Soniox всё ещё может выбрать что-то другое если уверен. Спасает от
      // ошибочного выбора JP/CN на коротких/тихих русских фразах из mic-канала.
      createBody.language_hints = ['ru', 'en', 'kk'];
      createBody.language_hints_strict = false;
    }

    const createResp = await fetch(`${base}/transcriptions`, {
      method: 'POST',
      headers,
      body: JSON.stringify(createBody),
    });
    if (!createResp.ok) {
      throw new Error(`soniox create ${createResp.status}: ${await safeText(createResp)}`);
    }
    ({ id } = (await createResp.json()) as { id: string });
    jobCreated = true;
  }

  // 2. Polling до completed/failed/deadline.
  let completed = false;
  while (Date.now() < opts.pollDeadlineMs) {
    await sleep(POLL_INTERVAL_MS);
    const statusResp = await fetch(`${base}/transcriptions/${id}`, {
      headers: { authorization: headers.authorization },
    });
    if (!statusResp.ok) {
      throw new Error(`soniox status ${statusResp.status}: ${await safeText(statusResp)}`);
    }
    const status = (await statusResp.json()) as { status: string };
    if (status.status === 'completed') {
      completed = true;
      break;
    }
    if (status.status === 'failed') {
      throw new Error('soniox transcription failed');
    }
  }
  // [B16 audit P1]: явный poll timeout вместо silent fall-through на fetch transcript
  // (который вернул бы partial/4xx и юзер видел бы непонятную ошибку).
  if (!completed) {
    throw new Error(`soniox poll timeout (job ${id})`);
  }

  // 3. Забираем результат.
  const transcriptResp = await fetch(`${base}/transcriptions/${id}/transcript`, {
    headers: { authorization: headers.authorization },
  });
  if (!transcriptResp.ok) {
    throw new Error(`soniox transcript ${transcriptResp.status}: ${await safeText(transcriptResp)}`);
  }
  const transcript = (await transcriptResp.json()) as SonioxTranscript;

  return { transcript: normalizeSoniox(transcript), jobId: id, jobCreated };
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
