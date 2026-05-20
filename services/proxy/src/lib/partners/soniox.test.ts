import { afterEach, describe, expect, test, vi } from 'vitest';
import { normalizeSoniox, transcribeSoniox, SONIOX_BASE_URL } from './soniox.js';

afterEach(() => {
  vi.unstubAllGlobals();
});

function mockFetchSequence(responses: Array<{ status?: number; json?: unknown; text?: string }>) {
  let i = 0;
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input.toString();
    calls.push({ url, init });
    const r = responses[i++];
    if (!r) throw new Error(`unexpected extra fetch to ${url}`);
    return new Response(r.text ?? (r.json !== undefined ? JSON.stringify(r.json) : ''), {
      status: r.status ?? 200,
      headers: { 'content-type': 'application/json' },
    });
  });
  vi.stubGlobal('fetch', fetchMock);
  return { fetchMock, calls };
}

describe('normalizeSoniox', () => {
  test('empty tokens → empty segments', () => {
    const t = normalizeSoniox({ tokens: [], duration_ms: 0, language: null as unknown as string });
    expect(t.segments).toEqual([]);
    expect(t.provider).toBe('soniox');
    expect(t.version).toBe(1);
  });

  test('groups consecutive tokens by speaker', () => {
    const t = normalizeSoniox({
      duration_ms: 4000,
      language: 'ru',
      tokens: [
        { text: 'Hello ', start_ms: 0, end_ms: 500, speaker: 0 },
        { text: 'world', start_ms: 500, end_ms: 1000, speaker: 0 },
        { text: 'Hi', start_ms: 2000, end_ms: 2500, speaker: 1 },
      ],
    });
    expect(t.segments).toHaveLength(2);
    expect(t.segments[0]).toMatchObject({
      speakerTag: 'Speaker 0',
      text: 'Hello world',
      start: 0,
      end: 1,
    });
    expect(t.segments[1]).toMatchObject({ speakerTag: 'Speaker 1', text: 'Hi' });
    expect(t.langDetected).toBe('ru');
    expect(t.durationSec).toBe(4);
  });

  test('falls back to Speaker 0 when speaker missing', () => {
    const t = normalizeSoniox({
      duration_ms: 1000,
      tokens: [{ text: 'x', start_ms: 0, end_ms: 100 }],
    });
    expect(t.segments[0]!.speakerTag).toBe('Speaker 0');
  });
});

describe('transcribeSoniox', () => {
  const opts = {
    apiKey: 'sk-test',
    audioUrl: 'https://r2.example/audio.wav',
    lang: 'auto' as const,
    pollDeadlineMs: Date.now() + 5000,
    sleep: async () => {},
  };

  test('uses default base URL when not overridden', () => {
    expect(SONIOX_BASE_URL).toBe('https://api.soniox.com/v1');
  });

  test('happy path: create → poll → fetch transcript', async () => {
    const { fetchMock, calls } = mockFetchSequence([
      { status: 200, json: { id: 'job-123' } }, // POST /transcriptions
      { status: 200, json: { status: 'completed' } }, // GET status
      {
        status: 200,
        json: {
          language: 'en',
          duration_ms: 2000,
          tokens: [{ text: 'hi', start_ms: 0, end_ms: 500, speaker: 0 }],
        },
      },
    ]);

    const result = await transcribeSoniox(opts);
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(calls[0]!.url).toBe('https://api.soniox.com/v1/transcriptions');
    expect(calls[0]!.init?.method).toBe('POST');
    expect(calls[1]!.url).toBe('https://api.soniox.com/v1/transcriptions/job-123');
    expect(calls[2]!.url).toBe('https://api.soniox.com/v1/transcriptions/job-123/transcript');
    expect(result.transcript.provider).toBe('soniox');
    expect(result.transcript.segments).toHaveLength(1);
    expect(result.transcript.langDetected).toBe('en');
    expect(result.jobId).toBe('job-123');
    expect(result.jobCreated).toBe(true);
  });

  test('resume with existingJobId skips create step', async () => {
    const { fetchMock, calls } = mockFetchSequence([
      { status: 200, json: { status: 'completed' } }, // GET status
      { status: 200, json: { duration_ms: 1000, tokens: [] } },
    ]);
    const result = await transcribeSoniox({ ...opts, existingJobId: 'resumed-99' });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(calls[0]!.url).toBe('https://api.soniox.com/v1/transcriptions/resumed-99');
    expect(result.jobId).toBe('resumed-99');
    expect(result.jobCreated).toBe(false);
  });

  test('passes language_hints when lang is explicit', async () => {
    const { calls } = mockFetchSequence([
      { status: 200, json: { id: 'job' } },
      { status: 200, json: { status: 'completed' } },
      { status: 200, json: { duration_ms: 0, tokens: [] } },
    ]);

    await transcribeSoniox({ ...opts, lang: 'ru' });
    const body = JSON.parse(calls[0]!.init!.body as string);
    expect(body.language_hints).toEqual(['ru']);
    expect(body.language_hints_strict).toBe(true);
    expect(body.enable_language_identification).toBe(false);
  });

  test('throws on create non-2xx', async () => {
    mockFetchSequence([{ status: 401, text: 'unauthorized' }]);
    await expect(transcribeSoniox(opts)).rejects.toThrow(/soniox create 401/);
  });

  test('throws on status=failed', async () => {
    mockFetchSequence([
      { status: 200, json: { id: 'job' } },
      { status: 200, json: { status: 'failed' } },
    ]);
    await expect(transcribeSoniox(opts)).rejects.toThrow(/soniox transcription failed/);
  });

  test('throws on transcript fetch non-2xx', async () => {
    mockFetchSequence([
      { status: 200, json: { id: 'job' } },
      { status: 200, json: { status: 'completed' } },
      { status: 500, text: 'oops' },
    ]);
    await expect(transcribeSoniox(opts)).rejects.toThrow(/soniox transcript 500/);
  });
});
