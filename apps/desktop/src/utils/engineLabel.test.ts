// @vitest-environment node
import { describe, expect, test } from 'vitest';
import { engineLabelHuman, type EngineLabelStrings } from './engineLabel';

const S: EngineLabelStrings = {
  cloud: 'облако (Wotold proxy)',
  localLight: 'локальный Qwen 1.5B (Light)',
  localBalanced: 'локальный Qwen 3B (Balanced)',
  localQuality: 'локальный Qwen 7B (Quality)',
  localGeneric: 'локальный Qwen',
};

describe('engineLabelHuman', () => {
  test('cloud-managed', () => {
    expect(engineLabelHuman('cloud-managed', S)).toBe('облако (Wotold proxy)');
  });
  test('local presets', () => {
    expect(engineLabelHuman('local-qwen-1.5b', S)).toBe('локальный Qwen 1.5B (Light)');
    expect(engineLabelHuman('local-qwen-3b', S)).toBe('локальный Qwen 3B (Balanced)');
    expect(engineLabelHuman('local-qwen-7b', S)).toBe('локальный Qwen 7B (Quality)');
    expect(engineLabelHuman('local-qwen', S)).toBe('локальный Qwen');
  });
  test('null/undefined → null (no badge)', () => {
    expect(engineLabelHuman(null, S)).toBeNull();
    expect(engineLabelHuman(undefined, S)).toBeNull();
  });
  test('unknown engine → raw passthrough', () => {
    expect(engineLabelHuman('local-qwen-experimental', S)).toBe('local-qwen-experimental');
  });
});
