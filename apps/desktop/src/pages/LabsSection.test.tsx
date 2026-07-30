// [M14 T-14] LabsSection vitest — load defaults, toggle persist for
// summary_v2 (default ON) и ограничитель «сколько голосов».
//
// Тумблер ускорения генерации удалён вместе с настройкой: черновая модель
// обязательна и применяется всегда, когда лежит на диске.
//
// [B18.5b] Wotold v2: checkboxes → role=switch buttons (aria-checked),
// native <select> → custom Select (combobox trigger + listbox options).

import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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

  test('renders summary v2 ON by default; лишних тумблеров нет', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_setting') return null; // missing → defaults apply
      return null;
    });
    render(<LabsSection />);
    await flush();
    const switches = screen.getAllByRole('switch');
    expect(switches).toHaveLength(1);
    await waitFor(() => expect(switches[0]!).toHaveAttribute('aria-checked', 'true'));
  });

  test('renders summary v2 OFF when setting is "0"', async () => {
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

  test('summary v2 toggle click persists via set_setting', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_setting') {
        const a = args as { key: string };
        if (a.key === 'summary_v2_enabled') return '1';
        return null;
      }
      if (cmd === 'set_setting') {
        const a = args as { key: string; value: string };
        expect(a.key).toBe('summary_v2_enabled');
        expect(a.value).toBe('0');
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

  test('force-N-speakers defaults to auto when no setting', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_setting') return null;
      return null;
    });
    render(<LabsSection />);
    await flush();
    // Trigger shows the selected option's label.
    await waitFor(() =>
      expect(screen.getByRole('combobox')).toHaveTextContent('Авто (рекомендовано)'),
    );
  });

  test('force-N-speakers reads "3" из DB и render"ит', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_setting') {
        const a = args as { key: string };
        if (a.key === 'mic_diarization_num_speakers') return '3';
        return null;
      }
      return null;
    });
    render(<LabsSection />);
    await flush();
    await waitFor(() =>
      expect(screen.getByRole('combobox')).toHaveTextContent('3 собеседника'),
    );
  });

  test('force-N-speakers change persists via set_setting', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_setting') return null;
      if (cmd === 'set_setting') {
        const a = args as { key: string; value: string };
        if (a.key === 'mic_diarization_num_speakers') {
          expect(a.value).toBe('2');
        }
        return null;
      }
      return null;
    });
    render(<LabsSection />);
    await flush();
    // Open the custom Select, pick the "2 собеседника" option (index 1).
    await userEvent.click(screen.getByRole('combobox'));
    const opts = screen.getAllByRole('option');
    fireEvent.mouseDown(opts[1]!);
    expect(mockInvoke).toHaveBeenCalledWith('set_setting', {
      key: 'mic_diarization_num_speakers',
      value: '2',
    });
  });

});
