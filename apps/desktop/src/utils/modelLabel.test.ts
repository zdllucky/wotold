import { describe, expect, it } from 'vitest';
import { modelLabel } from './modelLabel';
import { ru } from '../i18n/ru';
import type { TranslationKey } from '../i18n';

// Mock t() resolves dotted i18n keys against the ru dictionary (мок: тест
// инвариант что labels существуют в ru, не качество перевода).
function makeT() {
  return (key: TranslationKey): string => {
    const parts = key.split('.');
    let cursor: unknown = ru;
    for (const p of parts) {
      if (cursor && typeof cursor === 'object' && p in (cursor as Record<string, unknown>)) {
        cursor = (cursor as Record<string, unknown>)[p];
      } else {
        return `[missing:${key}]`;
      }
    }
    return typeof cursor === 'string' ? cursor : `[notstring:${key}]`;
  };
}

describe('modelLabel', () => {
  const t = makeT();

  it.each([
    ['whisper-small', 'Модуль речи · S'],
    ['whisper-medium', 'Модуль речи · M'],
    ['whisper-large-v3', 'Модуль речи · L'],
    ['qwen25-1_5b', 'Модуль саммари · S'],
    ['qwen25-3b', 'Модуль саммари · M'],
    ['qwen25-7b', 'Модуль саммари · L'],
    ['pyannote-segmentation', 'Модуль разделения · базовый'],
  ])('returns abstract label for known id %s', (id, expected) => {
    expect(modelLabel(id, t)).toBe(expected);
  });

  it('falls back to raw id for unknown model', () => {
    expect(modelLabel('llama-99b', t)).toBe('llama-99b');
  });

  it('never leaks the brand "Whisper" / "Qwen" / "Pyannote" in known labels', () => {
    const known = [
      'whisper-small',
      'whisper-medium',
      'whisper-large-v3',
      'qwen25-1_5b',
      'qwen25-3b',
      'qwen25-7b',
      'pyannote-segmentation',
    ];
    for (const id of known) {
      const label = modelLabel(id, t);
      expect(label).not.toMatch(/whisper|qwen|pyannote/i);
    }
  });
});
