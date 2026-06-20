import { describe, expect, test } from 'vitest';
import { PIPELINE_STEP_KEYS, pipelineStepKey } from './callState';

describe('pipelineStepKey — единый источник лейбла шага', () => {
  test('NULL → step1 (только-что-стартовавшая обработка)', () => {
    expect(pipelineStepKey(null)).toBe('pipeline.step1');
  });

  test('undefined → step1', () => {
    expect(pipelineStepKey(undefined)).toBe('pipeline.step1');
  });

  test('0 → step1 (clamp снизу)', () => {
    expect(pipelineStepKey(0)).toBe('pipeline.step1');
  });

  test('NaN → step1 (defensive)', () => {
    expect(pipelineStepKey(Number.NaN)).toBe('pipeline.step1');
  });

  test('каждый валидный шаг 1..5 → соответствующий ключ', () => {
    expect(pipelineStepKey(1)).toBe('pipeline.step1');
    expect(pipelineStepKey(2)).toBe('pipeline.step2');
    expect(pipelineStepKey(3)).toBe('pipeline.step3');
    expect(pipelineStepKey(4)).toBe('pipeline.step4');
    expect(pipelineStepKey(5)).toBe('pipeline.step5');
  });

  test('переполнение сверху → последний шаг (clamp сверху)', () => {
    expect(pipelineStepKey(9)).toBe(
      PIPELINE_STEP_KEYS[PIPELINE_STEP_KEYS.length - 1],
    );
  });

  test('дробное значение усекается (3.9 → step3)', () => {
    expect(pipelineStepKey(3.9)).toBe('pipeline.step3');
  });
});
