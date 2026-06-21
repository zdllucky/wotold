import { describe, expect, test } from 'vitest';
import { isMarkdownBlank } from './markdown';

describe('isMarkdownBlank', () => {
  test('null / undefined / empty → blank', () => {
    expect(isMarkdownBlank(null)).toBe(true);
    expect(isMarkdownBlank(undefined)).toBe(true);
    expect(isMarkdownBlank('')).toBe(true);
    expect(isMarkdownBlank('   \n  \n')).toBe(true);
  });

  test('только заголовок (старый пустой рекап) → blank', () => {
    expect(isMarkdownBlank('# Рекап\n\n')).toBe(true);
    expect(isMarkdownBlank('# Рекап')).toBe(true);
    expect(isMarkdownBlank('## A\n\n### B\n')).toBe(true);
  });

  test('заголовок + тело → НЕ blank', () => {
    expect(isMarkdownBlank('# Рекап\n\nКоманда обсудила релиз.')).toBe(false);
    expect(isMarkdownBlank('# Рекап\n\n## Ключевое\n- пункт')).toBe(false);
  });

  test('тело без заголовка → НЕ blank', () => {
    expect(isMarkdownBlank('просто текст')).toBe(false);
  });

  test('строка вида #hashtag (не heading) → НЕ blank', () => {
    // `#тег` без пробела после # — не markdown-heading, считается контентом.
    expect(isMarkdownBlank('#тег')).toBe(false);
  });
});
