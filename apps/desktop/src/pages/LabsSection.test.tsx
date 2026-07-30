// [M14 T-14] LabsSection vitest — дефолт и запись тумблера summary_v2.
//
// Тумблеры ускорения генерации и «число собеседников» удалены вместе с их
// настройками: черновая модель обязательна, а число кластеров определяет
// диаризатор — прежний потолок в три спикера снят.
//
// [B18.5b] Wotold v2: checkbox → role=switch button (aria-checked).

import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { LabsSection } from './LabsSection';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe('LabsSection', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });
  afterEach(() => cleanup());

  test('в разделе остался ровно один тумблер', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_setting') return null; // нет значения → дефолты
      return null;
    });
    render(<LabsSection />);
    await flush();
    const switches = screen.getAllByRole('switch');
    expect(switches).toHaveLength(1);
    await waitFor(() => expect(switches[0]!).toHaveAttribute('aria-checked', 'true'));
    // Селектора «сколько собеседников» больше нет — число кластеров
    // определяет диаризатор.
    expect(screen.queryByRole('combobox')).toBeNull();
  });

  test('summary v2 выключен, когда в настройке "0"', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_setting') {
        const a = args as { key: string };
        if (a.key === 'summary_v2_enabled') return '0';
        return null;
      }
      return null;
    });
    render(<LabsSection />);
    await flush();
    const summaryV2 = screen.getAllByRole('switch')[0]!;
    await waitFor(() => expect(summaryV2).toHaveAttribute('aria-checked', 'false'));
  });

  test('клик по тумблеру пишет настройку', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_setting') {
        const a = args as { key: string };
        if (a.key === 'summary_v2_enabled') return '1';
        return null;
      }
      return null;
    });
    render(<LabsSection />);
    await flush();
    const summaryV2 = screen.getAllByRole('switch')[0]!;
    await waitFor(() => expect(summaryV2).toHaveAttribute('aria-checked', 'true'));
    await act(async () => {
      summaryV2.click();
    });
    expect(mockInvoke).toHaveBeenCalledWith('set_setting', {
      key: 'summary_v2_enabled',
      value: '0',
    });
  });
});
