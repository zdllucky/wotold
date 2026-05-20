import { afterEach, describe, expect, test, vi } from 'vitest';
import { GLADIA_BASE_URL, normalizeGladia, transcribeGladia } from './gladia.js';

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

describe('normalizeGladia', () => {
  test('empty utterances → empty segments', () => {
    const t = normalizeGladia({ status: 'done', result: { transcription: { utterances: [] } } });
    expect(t.segments).toEqual([]);
    expect(t.provider).toBe('gladia');
  });

  test('maps utterances to segments with speakerTag', () => {
    const t = normalizeGladia({
      status: 'done',
      result: {
        metadata: { audio_duration: 12.5 },
        transcription: {
          languages: ['en', 'ru'],
          utterances: [
            { start: 0, end: 2, text: 'hi', speaker: 0, confidence: 0.95 },
            { start: 2, end: 4, text: 'there', speaker: 1 },
          ],
        },
      },
    });
    expect(t.segments).toHaveLength(2);
    expect(t.segments[0]).toMatchObject({
      speakerTag: 'Speaker 0',
      text: 'hi',
      confidence: 0.95,
    });
    expect(t.segments[1]!.speakerTag).toBe('Speaker 1');
    expect(t.langDetected).toBe('en');
    expect(t.durationSec).toBe(12.5);
  });

  test('falls back to Speaker 0 when speaker absent', () => {
    const t = normalizeGladia({
      status: 'done',
      result: { transcription: { utterances: [{ start: 0, end: 1, text: 'x' }] } },
    });
    expect(t.segments[0]!.speakerTag).toBe('Speaker 0');
  });
});

describe('transcribeGladia', () => {
  const opts = {
    apiKey: 'gl-test',
    audioUrl: 'https://r2.example/audio.wav',
    lang: 'auto' as const,
    pollDeadlineMs: Date.now() + 5000,
    sleep: async () => {},
  };

  test('exports GLADIA_BASE_URL', () => {
    expect(GLADIA_BASE_URL).toBe('https://api.gladia.io/v2');
  });

  test('happy path: create → poll done → normalize', async () => {
    const { fetchMock, calls } = mockFetchSequence([
      { status: 200, json: { id: 'g-1', result_url: 'https://api.gladia.io/v2/result/g-1' } },
      {
        status: 200,
        json: {
          status: 'done',
          result: {
            metadata: { audio_duration: 3 },
            transcription: {
              languages: ['ru'],
              utterances: [{ start: 0, end: 1.5, text: 'go', speaker: 0 }],
            },
          },
        },
      },
    ]);

    const result = await transcribeGladia(opts);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(calls[0]!.url).toBe('https://api.gladia.io/v2/pre-recorded');
    expect(result.provider).toBe('gladia');
    expect(result.durationSec).toBe(3);
    expect(result.segments).toHaveLength(1);
  });

  test('throws on create non-2xx', async () => {
    mockFetchSequence([{ status: 401, text: 'no key' }]);
    await expect(transcribeGladia(opts)).rejects.toThrow(/gladia create 401/);
  });

  test('throws on status=error', async () => {
    mockFetchSequence([
      { status: 200, json: { id: 'g', result_url: 'https://x/r' } },
      { status: 200, json: { status: 'error', error_code: 'AUDIO_UNREADABLE' } },
    ]);
    await expect(transcribeGladia(opts)).rejects.toThrow(/AUDIO_UNREADABLE/);
  });

  test('throws on poll deadline expired before any poll succeeds', async () => {
    // Deadline уже прошёл — после create while-loop не выполнится ни разу.
    mockFetchSequence([{ status: 200, json: { id: 'g', result_url: 'https://x/r' } }]);
    await expect(
      transcribeGladia({ ...opts, pollDeadlineMs: Date.now() - 1 }),
    ).rejects.toThrow(/gladia poll timeout/);
  });

  test('throws on poll non-2xx', async () => {
    mockFetchSequence([
      { status: 200, json: { id: 'g', result_url: 'https://x/r' } },
      { status: 500, text: 'boom' },
    ]);
    await expect(transcribeGladia(opts)).rejects.toThrow(/gladia poll 500/);
  });

  test('passes language_config when lang is not auto', async () => {
    const { calls } = mockFetchSequence([
      { status: 200, json: { id: 'g', result_url: 'https://x/r' } },
      {
        status: 200,
        json: {
          status: 'done',
          result: { transcription: { utterances: [] } },
        },
      },
    ]);
    await transcribeGladia({ ...opts, lang: 'en' });
    const body = JSON.parse(calls[0]!.init!.body as string);
    expect(body.language_config).toEqual({ languages: ['en'], code_switching: true });
  });
});
