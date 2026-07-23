// [B27.8] parseFragmentRefs: валидные ссылки → refs-сегменты, мусор → текст.

import { describe, expect, it } from 'vitest';

import { parseFragmentRefs } from './fragmentRefs';

describe('parseFragmentRefs', () => {
  it('одиночная ссылка [2] режет текст на три сегмента', () => {
    expect(parseFragmentRefs('см. [2] выше', 3)).toEqual([
      { kind: 'text', text: 'см. ' },
      { kind: 'refs', indices: [2], raw: '[2]' },
      { kind: 'text', text: ' выше' },
    ]);
  });

  it('список [2, 4] — один refs-сегмент с двумя индексами', () => {
    const segs = parseFragmentRefs('(фрагменты [2, 4])', 5);
    expect(segs).toEqual([
      { kind: 'text', text: '(фрагменты ' },
      { kind: 'refs', indices: [2, 4], raw: '[2, 4]' },
      { kind: 'text', text: ')' },
    ]);
  });

  it('номер вне диапазона и [0] остаются текстом', () => {
    expect(parseFragmentRefs('см. [9]', 3)).toEqual([{ kind: 'text', text: 'см. [9]' }]);
    expect(parseFragmentRefs('пункт [0]', 3)).toEqual([{ kind: 'text', text: 'пункт [0]' }]);
    // Смешанный список с невалидным номером — весь матч в текст.
    expect(parseFragmentRefs('[1, 9]', 3)).toEqual([{ kind: 'text', text: '[1, 9]' }]);
  });

  it('fragmentCount=0 выключает парсинг, пустой текст — пустой список', () => {
    expect(parseFragmentRefs('см. [1]', 0)).toEqual([{ kind: 'text', text: 'см. [1]' }]);
    expect(parseFragmentRefs('', 3)).toEqual([]);
  });

  it('несколько ссылок вперемешку с текстом', () => {
    const segs = parseFragmentRefs('a [1] b [3] c', 3);
    expect(segs.filter((s) => s.kind === 'refs')).toHaveLength(2);
    expect(segs.map((s) => (s.kind === 'text' ? s.text : s.raw)).join('')).toBe('a [1] b [3] c');
  });
});
