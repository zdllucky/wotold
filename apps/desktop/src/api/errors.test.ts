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
  test('proxy 502', () => {
    expect(humanError('llm: provider: proxy 502 Bad Gateway')).toMatch(/временно недоступен/i);
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
