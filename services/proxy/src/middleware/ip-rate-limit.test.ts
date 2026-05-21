import { describe, expect, test } from 'vitest';

import { ip16Prefix } from './ip-rate-limit.js';

describe('ip16Prefix', () => {
  test('IPv4 → v4:oct1.oct2', () => {
    expect(ip16Prefix('1.2.3.4')).toBe('v4:1.2');
    expect(ip16Prefix('192.168.10.255')).toBe('v4:192.168');
    expect(ip16Prefix('10.0.0.1')).toBe('v4:10.0');
  });

  test('IPv6 expanded → v6:first hex block', () => {
    expect(ip16Prefix('2001:db8:abcd::1')).toBe('v6:2001');
    expect(ip16Prefix('fe80::1')).toBe('v6:fe80');
  });

  test('IPv6 compressed leading :: → берёт первый non-empty', () => {
    expect(ip16Prefix('::1')).toBe('v6:1');
    expect(ip16Prefix('::ffff:192.168.1.1')).toBe('v6:ffff');
  });

  test('IPv6 case-insensitive normalize', () => {
    expect(ip16Prefix('FE80::1')).toBe('v6:fe80');
    expect(ip16Prefix('2001:DB8::1')).toBe('v6:2001');
  });

  test('IPv6 truncates >4 hex chars (защита от malformed)', () => {
    // Хотя реальный IPv6 hex block ≤ 4 chars, на корявом input не сегфолтим.
    expect(ip16Prefix('20010000:db8::1')).toBe('v6:2001');
  });

  test('Empty / null / undefined → unknown', () => {
    expect(ip16Prefix('')).toBe('unknown');
    expect(ip16Prefix(null)).toBe('unknown');
    expect(ip16Prefix(undefined)).toBe('unknown');
    expect(ip16Prefix('   ')).toBe('unknown');
  });

  test('Malformed IPv4 (без второго октета) → unknown', () => {
    expect(ip16Prefix('1')).toBe('unknown');
    expect(ip16Prefix('1.')).toBe('unknown');
    expect(ip16Prefix('abc.def')).toBe('unknown');
  });

  test('IPv4 с нечисловым октетом → unknown', () => {
    // [Sec] защита от injection в KV key — не пропускаем символы '/', ':', etc.
    expect(ip16Prefix('1.2:3:4')).toBe('unknown');
    expect(ip16Prefix('foo.bar.1.2')).toBe('unknown');
  });
});
