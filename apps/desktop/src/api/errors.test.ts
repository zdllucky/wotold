// @vitest-environment node
import { describe, expect, test } from 'vitest';
import { humanError } from './errors';

describe('humanError', () => {
  test('network refused', () => {
    expect(humanError(new Error('ECONNREFUSED'))).toMatch(/нет соединения/i);
  });
  // [Bug-fix] Локальный engine error patterns — должны попадать в local-
  // specific human messages, а не в "Сервис временно занят".
  test('local_llm_timeout → не успела ответить', () => {
    expect(humanError('local_engine_llm_failed: provider: local_llm_timeout')).toMatch(
      /не успела ответить/i,
    );
  });
  test('local_engine_model_missing → понятный текст', () => {
    expect(
      humanError('local_engine_model_missing: модель qwen25-3b не установлена'),
    ).toMatch(/не установлена/i);
  });
  test('local_engine_preset_not_set → понятный текст', () => {
    expect(
      humanError('local_engine_preset_not_set: выберите Light/Balanced/Quality'),
    ).toMatch(/preset локального движка/i);
  });
  test('generic local_engine_llm_failed без timeout', () => {
    expect(humanError('local_engine_llm_failed: provider: gbnf parse error')).toMatch(
      /не справилась/i,
    );
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

  // [P2.2] Closing local_engine error coverage gaps. Без этих pattern'ов
  // юзер видел бы raw token «local_engine_recap_persist: ...» в баннере
  // failed_reason.

  test('local_engine_no_app_handle → внутренняя ошибка + перезапуск', () => {
    expect(
      humanError('local_engine_no_app_handle: pipeline requires Tauri runtime'),
    ).toMatch(/внутренняя ошибка/i);
    expect(
      humanError('local_engine_no_app_handle: pipeline requires Tauri runtime'),
    ).toMatch(/перезапусти/i);
  });

  test('local_engine_transcript_read → переобработать', () => {
    expect(humanError('local_engine_transcript_read: enoent')).toMatch(
      /прочитать транскрипт/i,
    );
    expect(humanError('local_engine_transcript_read: enoent')).toMatch(/переобработать/i);
  });

  test('local_engine_stt_failed (mic) → проверь модели', () => {
    expect(humanError('local_engine_stt_failed (mic): sherpa-onnx panic')).toMatch(
      /транскрипция не справилась/i,
    );
    expect(humanError('local_engine_stt_failed (mic): sherpa-onnx panic')).toMatch(
      /модели установлены/i,
    );
  });

  test('local_engine_recap_persist → сгенерировано но не сохранилось', () => {
    expect(humanError('local_engine_recap_persist: sqlite write')).toMatch(
      /сгенерировано, но не сохранилось/i,
    );
  });

  // Регрессия: новые pattern'ы должны идти ДО generic local_engine_llm_failed.
  // Verify что persist/stt_failed/transcript_read не cabbed под "не справилась
  // с задачей".
  test('local_engine_recap_persist НЕ catches как llm_failed', () => {
    const out = humanError('local_engine_recap_persist: sqlite');
    expect(out).not.toMatch(/не справилась с задачей/i);
  });
});
