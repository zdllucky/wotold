import type { DiarizedTranscript, TranscriptSegment } from '@wotold/contracts';

// Gladia v2 pre-recorded:
//   POST /v2/pre-recorded {audio_url, diarization, diarization_config?, language_config?}
//     → {id, result_url}
//   GET  result_url (absolute) → {status: queued|processing|done|error, result?, error_code?}
// Auth: x-gladia-key: <apiKey>

export const GLADIA_BASE_URL = 'https://api.gladia.io/v2';
const POLL_INTERVAL_MS = 1500;

export interface GladiaOpts {
  apiKey: string;
  audioUrl: string;
  lang: 'auto' | string;
  pollDeadlineMs: number;
  baseUrl?: string;
  sleep?: (ms: number) => Promise<void>;
  /**
   * [B3]: resume из KV-кэша. Gladia create отдаёт {id, result_url} —
   * для resume нужен result_url (id справочный).
   */
  existingJobId?: string;
  existingResultUrl?: string;
}

export interface TranscribeGladiaResult {
  transcript: DiarizedTranscript;
  jobId: string;
  resultUrl: string;
  jobCreated: boolean;
}

interface GladiaUtterance {
  start: number;
  end: number;
  speaker?: number;
  text: string;
  confidence?: number;
}

interface GladiaResult {
  metadata?: { audio_duration?: number };
  transcription?: {
    languages?: string[];
    full_transcript?: string;
    utterances?: GladiaUtterance[];
  };
}

interface GladiaResponse {
  status: 'queued' | 'processing' | 'done' | 'error';
  result?: GladiaResult;
  error_code?: string;
}

export async function transcribeGladia(opts: GladiaOpts): Promise<TranscribeGladiaResult> {
  const base = opts.baseUrl ?? GLADIA_BASE_URL;
  const sleep = opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
  const headers = {
    'x-gladia-key': opts.apiKey,
    'content-type': 'application/json',
  } as const;

  let id: string;
  let result_url: string;
  let jobCreated = false;
  if (opts.existingJobId && opts.existingResultUrl) {
    id = opts.existingJobId;
    result_url = opts.existingResultUrl;
  } else {
    const createBody: Record<string, unknown> = {
      audio_url: opts.audioUrl,
      diarization: true,
    };
    if (opts.lang !== 'auto') {
      createBody.language_config = {
        languages: [opts.lang],
        code_switching: true,
      };
    } else {
      // [Lang-tuning] auto-detect biased к ru/en/kk через code-switching.
      // Gladia может выбрать любой из списка для каждого сегмента — без strict
      // ограничения, что покрывает случаи фоновой музыки или невнятного звука
      // лучше чем full automatic.
      createBody.language_config = {
        languages: ['ru', 'en', 'kk'],
        code_switching: true,
      };
    }

    const createResp = await fetch(`${base}/pre-recorded`, {
      method: 'POST',
      headers,
      body: JSON.stringify(createBody),
    });
    if (!createResp.ok) {
      throw new Error(`gladia create ${createResp.status}: ${await safeText(createResp)}`);
    }
    ({ id, result_url } = (await createResp.json()) as {
      id: string;
      result_url: string;
    });
    jobCreated = true;
  }

  while (Date.now() < opts.pollDeadlineMs) {
    await sleep(POLL_INTERVAL_MS);
    const resp = await fetch(result_url, {
      headers: { 'x-gladia-key': opts.apiKey },
    });
    if (!resp.ok) {
      throw new Error(`gladia poll ${resp.status}: ${await safeText(resp)}`);
    }
    const data = (await resp.json()) as GladiaResponse;
    if (data.status === 'done') {
      return {
        transcript: normalizeGladia(data),
        jobId: id,
        resultUrl: result_url,
        jobCreated,
      };
    }
    if (data.status === 'error') {
      throw new Error(`gladia error_code: ${data.error_code ?? 'unknown'}`);
    }
  }

  throw new Error(`gladia poll timeout (job ${id})`);
}

export function normalizeGladia(r: GladiaResponse): DiarizedTranscript {
  const utterances = r.result?.transcription?.utterances ?? [];
  const segments: TranscriptSegment[] = utterances.map((u) => ({
    start: u.start,
    end: u.end,
    text: u.text,
    speakerTag: u.speaker != null ? `Speaker ${u.speaker}` : 'Speaker 0',
    ...(u.confidence != null ? { confidence: u.confidence } : {}),
  }));

  return {
    version: 1,
    provider: 'gladia',
    langDetected: r.result?.transcription?.languages?.[0] ?? null,
    durationSec: r.result?.metadata?.audio_duration ?? 0,
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
