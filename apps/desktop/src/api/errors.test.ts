// @vitest-environment node
import { describe, expect, test } from 'vitest';
import { humanError } from './errors';

describe('humanError', () => {
  test('network refused', () => {
    expect(humanError(new Error('ECONNREFUSED'))).toMatch(/нет соединения/i);
  });
  test('quota exceeded', () => {
    expect(humanError({ code: 'quota_exceeded', message: 'too many requests' })).toMatch(
      /превышен дневной лимит/i,
    );
  });
  test('proxy 502 → сервис временно занят', () => {
    expect(humanError('llm: provider: proxy 502 Bad Gateway')).toMatch(/временно занят/i);
  });
  // [Bug-fix #1] Cloudflare proxy wrapping Anthropic 429 как provider_error.
  // Не должно матчить "Превышен дневной лимит" — это transient, не quota cap.
  test('upstream 429 → провайдер занят (не quota)', () => {
    expect(
      humanError(
        'llm: provider: proxy 502 Bad Gateway: {"ok":false,"code":"provider_error","message":"LLM upstream error (429)"}',
      ),
    ).toMatch(/сервис временно занят/i);
  });
  test('upstream rate limit text', () => {
    expect(humanError(new Error('rate limit reached upstream'))).toMatch(
      /сервис временно занят/i,
    );
  });
  // [Bug-fix follow-up] failed_reason из DB — plain string, not wrapped.
  // Это путь recap_failed_reason → humanError на CallDetailPage.
  test('plain string failed_reason maps к friendly message', () => {
    expect(
      humanError(
        'llm: provider: proxy 502 Bad Gateway: {"ok":false,"code":"provider_error","message":"LLM upstream error (429)"}',
      ),
    ).not.toContain('Bad Gateway');
  });
  test('mic permission', () => {
    expect(humanError(new Error('Failed: NSMicrophoneUsageDescription'))).toMatch(
      /микрофон/i,
    );
  });
  test('disk full', () => {
    expect(humanError(new Error('ENOSPC: no space left'))).toMatch(/места на диске/i);
  });
  test('sqlite busy', () => {
    expect(humanError(new Error('database is locked'))).toMatch(/база данных занята/i);
  });
  test('cancelled', () => {
    expect(humanError(new Error('aborted by user'))).toMatch(/отменена/i);
  });
  test('unknown passes through truncated', () => {
    const long = 'X'.repeat(200);
    const out = humanError(new Error(long));
    expect(out.length).toBeLessThanOrEqual(165);
    expect(out).toContain('XXXXX');
  });
  test('null/undefined safe', () => {
    expect(humanError(null)).toBe('Неизвестная ошибка');
    expect(humanError(undefined)).toBe('Неизвестная ошибка');
  });
});
