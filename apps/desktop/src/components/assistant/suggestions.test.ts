// [B27.4] Подсказки: пул 50×3 локали без дублей, детерминированная выборка.

import { describe, expect, it } from 'vitest';

import { SUGGESTIONS, SUGGEST_COUNT, pickSuggestions } from './suggestions';

describe('SUGGESTIONS', () => {
  it('в каждой локали ровно 50 уникальных вопросов', () => {
    for (const [locale, pool] of Object.entries(SUGGESTIONS)) {
      expect(pool.length, locale).toBe(50);
      expect(new Set(pool).size, locale).toBe(50);
    }
  });
});

describe('pickSuggestions', () => {
  it('возвращает n уникальных элементов из пула', () => {
    const picked = pickSuggestions(SUGGESTIONS.ru, SUGGEST_COUNT);
    expect(picked).toHaveLength(4);
    expect(new Set(picked).size).toBe(4);
    for (const s of picked) expect(SUGGESTIONS.ru).toContain(s);
  });

  it('детерминирован при фиксированном rand', () => {
    const rand = () => 0; // всегда j === i — порядок пула
    expect(pickSuggestions(['a', 'b', 'c', 'd', 'e'], 3, rand)).toEqual(['a', 'b', 'c']);
  });

  it('n больше пула — возвращает весь пул', () => {
    expect(pickSuggestions(['a', 'b'], 5)).toHaveLength(2);
  });
});
