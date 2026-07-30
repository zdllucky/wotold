// @vitest-environment node
import { describe, expect, test } from 'vitest';
import { isParkedFailure, mapFailureToUxKind } from './failureKind';

describe('парковка звонка', () => {
  test('маркеры нехватки модулей — парковка, а не поломка', () => {
    for (const reason of [
      'local_engine_not_ready: не хватает модулей: silero-vad-v5, voice-embedder',
      'local_engine_model_missing: модель whisper-small не установлена',
      'local_engine_model_tampered: файл не прошёл проверку SHA256',
      'local_engine_preset_not_set: выберите размер движка',
    ]) {
      expect(isParkedFailure(reason)).toBe(true);
      expect(mapFailureToUxKind(reason)).toBe('parked');
    }
  });

  test('парковка проверяется раньше «модель не установлена»', () => {
    // Иначе `local_engine_model_missing` уводил бы в ветку с ручной
    // установкой, хотя звонок обработается сам после докачки.
    expect(mapFailureToUxKind('local_engine_model_missing: qwen25-3b')).not.toBe(
      'model_missing',
    );
  });

  test('настоящие сбои парковкой не считаются', () => {
    expect(isParkedFailure('local_engine_stt_failed (mic): sherpa panic')).toBe(false);
    expect(isParkedFailure('local_llm_timeout')).toBe(false);
    expect(isParkedFailure(null)).toBe(false);
    expect(mapFailureToUxKind('local_llm_timeout')).toBe('timeout');
    expect(mapFailureToUxKind('local_audio_decode_failed')).toBe('broken_recording');
    expect(mapFailureToUxKind('что-то своё')).toBe('generic');
  });
});
