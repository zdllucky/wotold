// Шаг онбординга «настройка движка».
//
// Главное, что проверяется: размер на кнопке приходит из каталога, а не из
// захардкоженных констант, — они занижали все три варианта, потому что не
// учитывали обязательные базовые модули.

import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

import { invoke } from '@tauri-apps/api/core';
import { OnboardingEngineStep } from './OnboardingEngineStep';
import { ReadinessProvider } from '../components/readiness/ReadinessProvider';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

const HW = {
  os: 'macos',
  arch: 'arm64',
  cpu_model: 'Apple M2 Pro',
  ram_gb: 16,
  metal_supported: true,
  recommendation: 'balanced',
};

// Реальные значения каталога: модели размера + обязательная база.
const SPECS = [
  {
    preset: 'light',
    whisper_model_id: 'whisper-small',
    llm_model_id: 'qwen25-1_5b',
    preset_bytes: 1_176_134_255,
    base_bytes: 566_646_307,
    total_bytes: 1_742_780_562,
  },
  {
    preset: 'balanced',
    whisper_model_id: 'whisper-medium',
    llm_model_id: 'qwen25-3b',
    preset_bytes: 2_469_115_731,
    base_bytes: 566_646_307,
    total_bytes: 3_035_762_038,
  },
  {
    preset: 'quality',
    whisper_model_id: 'whisper-large-v3',
    llm_model_id: 'qwen25-7b',
    preset_bytes: 5_764_214_443,
    base_bytes: 566_646_307,
    total_bytes: 6_330_860_750,
  },
];

async function flush() {
  await act(async () => {
    for (let i = 0; i < 8; i += 1) await Promise.resolve();
  });
}

function renderStep(onAdvance = vi.fn()) {
  render(
    <ReadinessProvider>
      <OnboardingEngineStep onAdvance={onAdvance} />
    </ReadinessProvider>,
  );
  return onAdvance;
}

describe('OnboardingEngineStep', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'local_engine_hw_probe') return HW;
      if (cmd === 'local_engine_preset_specs') return SPECS;
      if (cmd === 'local_engine_readiness')
        return { ready: false, preset: 'balanced', missing: [], missing_bytes_total: 0 };
      return null;
    });
  });
  afterEach(() => cleanup());

  test('размер на кнопке — полный, из каталога', async () => {
    renderStep();
    await flush();
    // Balanced: 2.83 ГБ с базой. Старая константа обещала 2.4 — меньше, чем
    // скачается.
    expect(screen.getByRole('button', { name: /~2\.8 GB/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /~2\.4 GB/ })).toBeNull();
  });

  test('другой размер меняет подпись кнопки', async () => {
    renderStep();
    await flush();
    act(() => screen.getByRole('button', { name: /Выбрать другой/ }).click());
    act(() => screen.getByRole('radio', { name: /Максимальный|Quality|Качество/ }).click());
    expect(screen.getByRole('button', { name: /~5\.9 GB/ })).toBeInTheDocument();
  });

  test('старт скачивания фиксирует размер и запускает докачку одной командой', async () => {
    renderStep();
    await flush();
    await act(async () => {
      screen.getByRole('button', { name: /Скачать и продолжить/ }).click();
      await Promise.resolve();
      await Promise.resolve();
    });
    await flush();

    expect(mockInvoke).toHaveBeenCalledWith('local_engine_set_active_preset', {
      preset: 'balanced',
    });
    expect(mockInvoke).toHaveBeenCalledWith('local_engine_ensure_required');
    // Своей очереди по моделям больше нет — фронт не перебирает id сам.
    expect(mockInvoke).not.toHaveBeenCalledWith(
      'local_engine_model_download',
      expect.anything(),
    );
  });

  test('«свернуть» во время докачки продвигает шаг и ничего не удаляет', async () => {
    const onAdvance = renderStep();
    await flush();
    await act(async () => {
      screen.getByRole('button', { name: /Скачать и продолжить/ }).click();
      await Promise.resolve();
    });
    await flush();

    act(() => screen.getByRole('button', { name: /докачается в фоне/ }).click());
    expect(onAdvance).toHaveBeenCalled();
    // Прежний «Отменить» удалял модель — причём не тот путь, что писала
    // качалка, так что полускачанный файл всё равно оставался на диске.
    expect(mockInvoke).not.toHaveBeenCalledWith(
      'local_engine_model_delete',
      expect.anything(),
    );
  });

  test('движок уже готов — шаг проматывается без лишнего клика', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'local_engine_hw_probe') return HW;
      if (cmd === 'local_engine_preset_specs') return SPECS;
      return { ready: true, preset: 'balanced', missing: [], missing_bytes_total: 0 };
    });
    const onAdvance = renderStep();
    await flush();
    expect(onAdvance).toHaveBeenCalled();
  });

  test('не-macOS: шаг проматывается сам', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'local_engine_hw_probe') return { ...HW, os: 'linux', recommendation: null };
      if (cmd === 'local_engine_preset_specs') return SPECS;
      return { ready: true, preset: null, missing: [], missing_bytes_total: 0 };
    });
    const onAdvance = renderStep();
    await flush();
    expect(onAdvance).toHaveBeenCalled();
  });
});
