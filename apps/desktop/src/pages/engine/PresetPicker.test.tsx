// Выбор размера движка.
//
// Главное: во время докачки размер не меняется. Очередь на бэкенде одна, и
// второй запрос дожидался бы первого — со стороны это выглядело бы как «нажал,
// ничего не произошло», а после окончания скачалось бы не то, что выбрали.

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

import { PresetPicker } from './PresetPicker';

afterEach(() => cleanup());

const SPECS = [
  {
    preset: 'light' as const,
    whisper_model_id: 'whisper-small',
    llm_model_id: 'qwen25-1_5b',
    preset_bytes: 1_176_134_255,
    base_bytes: 566_646_307,
    total_bytes: 1_742_780_562,
  },
  {
    preset: 'balanced' as const,
    whisper_model_id: 'whisper-medium',
    llm_model_id: 'qwen25-3b',
    preset_bytes: 2_469_115_731,
    base_bytes: 566_646_307,
    total_bytes: 3_035_762_038,
  },
  {
    preset: 'quality' as const,
    whisper_model_id: 'whisper-large-v3',
    llm_model_id: 'qwen25-7b',
    preset_bytes: 5_764_214_443,
    base_bytes: 566_646_307,
    total_bytes: 6_330_860_750,
  },
];

const ACTIVE = {
  preset: 'light' as const,
  whisper_model_id: 'whisper-small',
  llm_model_id: 'qwen25-1_5b',
};

function renderPicker(overrides: Partial<Parameters<typeof PresetPicker>[0]> = {}) {
  const onPick = vi.fn();
  render(
    <PresetPicker
      preset={ACTIVE}
      specs={SPECS}
      statuses={{}}
      downloadingIds={new Set()}
      busy={false}
      recommendation="balanced"
      onPick={onPick}
      {...overrides}
    />,
  );
  return onPick;
}

describe('PresetPicker', () => {
  test('размеры полные, из каталога', () => {
    renderPicker();
    const radios = screen.getAllByRole('radio');
    expect(radios).toHaveLength(3);
    expect(radios[0]).toHaveTextContent('1.6 GB');
    expect(radios[1]).toHaveTextContent('2.8 GB');
    expect(radios[2]).toHaveTextContent('5.9 GB');
  });

  test('во время докачки размер не меняется', () => {
    const onPick = renderPicker({ busy: true });
    const quality = screen.getAllByRole('radio')[2]!;
    expect(quality).toBeDisabled();
    act(() => quality.click());
    expect(onPick).not.toHaveBeenCalled();
    // И об этом сказано словами, а не только серым цветом кнопки.
    expect(screen.getByText(/размер не меняем/i)).toBeInTheDocument();
  });

  test('вне докачки выбор работает', () => {
    const onPick = renderPicker();
    act(() => screen.getAllByRole('radio')[2]!.click());
    expect(onPick).toHaveBeenCalledWith('quality');
  });

  test('статус берётся по моделям своего размера', () => {
    renderPicker({
      statuses: {
        'whisper-small': { state: 'present', id: 'whisper-small', bytes_total: 1 },
        'qwen25-1_5b': { state: 'present', id: 'qwen25-1_5b', bytes_total: 1 },
      },
      downloadingIds: new Set(['whisper-medium']),
    });
    const radios = screen.getAllByRole('radio');
    expect(radios[0]).toHaveTextContent('установлено');
    expect(radios[1]).toHaveTextContent('качаем…');
    expect(radios[2]).toHaveTextContent('не установлено');
  });
});
