// [M14 T-14] LabsSection vitest — load default ON, toggle persist, label render.

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

  test('renders toggle with default ON when setting missing', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_setting') return null; // missing → default ON
      return null;
    });
    render(<LabsSection />);
    await flush();
    const checkbox = screen.getByRole('checkbox') as HTMLInputElement;
    await waitFor(() => expect(checkbox.checked).toBe(true));
  });

  test('renders OFF when setting is "0"', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_setting') return '0';
      return null;
    });
    render(<LabsSection />);
    await flush();
    const checkbox = screen.getByRole('checkbox') as HTMLInputElement;
    await waitFor(() => expect(checkbox.checked).toBe(false));
  });

  test('toggle click persists via set_setting', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_setting') return '1';
      if (cmd === 'set_setting') {
        const a = args as { key: string; value: string };
        expect(a.key).toBe('summary_v2_enabled');
        // Toggle from ON → OFF persists "0".
        expect(a.value).toBe('0');
        return null;
      }
      return null;
    });
    render(<LabsSection />);
    await flush();
    const checkbox = screen.getByRole('checkbox') as HTMLInputElement;
    await waitFor(() => expect(checkbox.checked).toBe(true));
    await act(async () => {
      checkbox.click();
    });
    expect(mockInvoke).toHaveBeenCalledWith('set_setting', {
      key: 'summary_v2_enabled',
      value: '0',
    });
  });
});
