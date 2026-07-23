// [B26.10] Fuzzy-матчер: субпоследовательность со скорингом (TDD-первым).
import { describe, expect, test } from 'vitest';

import { fuzzyFilter, fuzzyScore } from './fuzzy';

describe('fuzzyScore', () => {
  test('точное и префиксное совпадение матчатся', () => {
    expect(fuzzyScore('чат', 'чат')).not.toBeNull();
    expect(fuzzyScore('нов', 'Новый чат')).not.toBeNull();
  });

  test('субпоследовательность с разрывом матчится', () => {
    expect(fuzzyScore('нч', 'Новый чат')).not.toBeNull();
    expect(fuzzyScore('счзап', 'сколько чатов записано')).not.toBeNull();
  });

  test('нет подпоследовательности — null', () => {
    expect(fuzzyScore('xyz', 'Новый чат')).toBeNull();
    expect(fuzzyScore('чан', 'чат')).toBeNull();
  });

  test('регистр и ё нормализуются', () => {
    expect(fuzzyScore('ПЛАНЁРКА', 'решения планерки продукта')).not.toBeNull();
    expect(fuzzyScore('планерки', 'Решения ПЛАНЁРКИ')).not.toBeNull();
  });

  test('начало слова ранжируется выше разрыва в середине', () => {
    const wordStart = fuzzyScore('нч', 'Новый чат')!;
    const scattered = fuzzyScore('нч', 'конченый')!;
    expect(wordStart).toBeGreaterThan(scattered);
  });

  test('пустой запрос матчит всё с нулевым скором', () => {
    expect(fuzzyScore('', 'что угодно')).toBe(0);
  });
});

describe('fuzzyFilter', () => {
  const chats = [
    { title: 'Решения планёрки продукта' },
    { title: 'Сколько звонков было' },
    { title: 'Новый чат' },
  ];

  test('фильтрует и сортирует по score', () => {
    const out = fuzzyFilter(chats, 'нч', (c) => c.title);
    expect(out[0]?.title).toBe('Новый чат');
  });

  test('пустой запрос возвращает все в исходном порядке', () => {
    expect(fuzzyFilter(chats, '', (c) => c.title)).toEqual(chats);
  });

  test('нет совпадений — пусто', () => {
    expect(fuzzyFilter(chats, 'qqq', (c) => c.title)).toEqual([]);
  });
});
