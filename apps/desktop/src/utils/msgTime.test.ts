// [B26.8] Формат времени облачка: сегодня → HH:MM, вчера → «вчера HH:MM»,
// раньше → дата; полный формат по клику.
import { describe, expect, test } from 'vitest';

import { formatMsgTime, formatMsgTimeFull } from './msgTime';

const NOW = new Date('2026-07-23T15:30:00');

describe('formatMsgTime', () => {
  test('сегодня — только время', () => {
    const s = formatMsgTime('2026-07-23T09:05:00', NOW, 'ru', 'вчера');
    expect(s).toMatch(/09:05/);
    expect(s).not.toMatch(/2026/);
    expect(s).not.toMatch(/вчера/);
  });

  test('вчера — метка + время', () => {
    const s = formatMsgTime('2026-07-22T21:40:00', NOW, 'ru', 'вчера');
    expect(s).toMatch(/^вчера /);
    expect(s).toMatch(/21:40/);
  });

  test('раньше — только дата', () => {
    const s = formatMsgTime('2026-07-01T10:00:00', NOW, 'ru', 'вчера');
    expect(s).toMatch(/01\.07\.2026/);
    expect(s).not.toMatch(/10:00/);
  });

  test('граница полуночи: вчера 23:59 против сегодня 00:01', () => {
    const now = new Date('2026-07-23T00:01:00');
    expect(formatMsgTime('2026-07-22T23:59:00', now, 'ru', 'вчера')).toMatch(/^вчера /);
    expect(formatMsgTime('2026-07-23T00:00:30', now, 'ru', 'вчера')).not.toMatch(/вчера/);
  });
});

describe('formatMsgTimeFull', () => {
  test('всегда дата + время', () => {
    const s = formatMsgTimeFull('2026-07-01T10:07:00', 'ru');
    expect(s).toMatch(/01\.07\.2026/);
    expect(s).toMatch(/10:07/);
  });
});
