// [M14 T-14 + T-16 P2] LabsSection vitest — load defaults, toggle persist for
// summary_v2 (default ON) и speculative decoding (default OFF).

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

  test('renders summary v2 ON by default + speculative OFF by default', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_setting') return null; // missing → defaults apply
      return null;
    });
    render(<LabsSection />);
    await flush();
    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[];
    expect(boxes).toHaveLength(2);
    const summaryV2 = boxes[0]!;
    const speculative = boxes[1]!;
    await waitFor(() => expect(summaryV2.checked).toBe(true));
    expect(speculative.checked).toBe(false);
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
    const summaryV2 = (screen.getAllByRole('checkbox') as HTMLInputElement[])[0]!;
    await waitFor(() => expect(summaryV2.checked).toBe(false));
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
    const summaryV2 = (screen.getAllByRole('checkbox') as HTMLInputElement[])[0]!;
    await waitFor(() => expect(summaryV2.checked).toBe(true));
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
    const select = await waitFor(() =>
      screen.getByRole('combobox') as HTMLSelectElement,
    );
    expect(select.value).toBe('auto');
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
    const select = await waitFor(() => {
      const s = screen.getByRole('combobox') as HTMLSelectElement;
      if (s.value !== '3') throw new Error('not yet');
      return s;
    });
    expect(select.value).toBe('3');
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
    const select = screen.getByRole('combobox') as HTMLSelectElement;
    await act(async () => {
      select.value = '2';
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });
    expect(mockInvoke).toHaveBeenCalledWith('set_setting', {
      key: 'mic_diarization_num_speakers',
      value: '2',
    });
  });

  test('speculative decoding toggle persists "1" when enabled', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_setting') return null;
      if (cmd === 'set_setting') {
        const a = args as { key: string; value: string };
        if (a.key === 'summary_speculative_decoding') {
          expect(a.value).toBe('1');
        }
        return null;
      }
      return null;
    });
    render(<LabsSection />);
    await flush();
    const speculative = (screen.getAllByRole('checkbox') as HTMLInputElement[])[1]!;
    await waitFor(() => expect(speculative.checked).toBe(false));
    await act(async () => {
      speculative.click();
    });
    expect(mockInvoke).toHaveBeenCalledWith('set_setting', {
      key: 'summary_speculative_decoding',
      value: '1',
    });
  });
});
