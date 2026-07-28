// [B21.6] Маппинг статуса голосового эмбеддера в строку хранилища.

import { describe, expect, test } from 'vitest';
import { voiceEmbedderRow, VOICE_EMBEDDER_ROW_ID } from './voiceStorageRow';

const HINT = 26_530_550;

describe('voiceEmbedderRow', () => {
  test('файла нет — absent, размер из подсказки каталога', () => {
    const row = voiceEmbedderRow({ status: 'missing' }, HINT);
    expect(row.id).toBe(VOICE_EMBEDDER_ROW_ID);
    expect(row.status.state).toBe('absent');
    // Без подсказки строка показала бы «0 GB» и выглядела как пустая.
    expect(row.size_bytes).toBe(HINT);
  });

  test('файл на месте — present с реальным размером', () => {
    const row = voiceEmbedderRow({ status: 'valid', size: 26_000_000 }, HINT);
    expect(row.status.state).toBe('present');
    expect(row.size_bytes).toBe(26_000_000);
  });

  test('битый файл — corrupted с хэшами', () => {
    const row = voiceEmbedderRow(
      { status: 'corrupted', size: 12, expected: 'aa', got: 'bb' },
      HINT,
    );
    expect(row.status).toMatchObject({ state: 'corrupted', expected: 'aa', got: 'bb' });
  });

  test('строка не считается активной — удаление идёт обычным путём', () => {
    expect(voiceEmbedderRow({ status: 'valid', size: 1 }, HINT).is_active).toBe(false);
  });
});
