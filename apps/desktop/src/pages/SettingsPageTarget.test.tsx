// [B34.4] Переход из палитры ⌘K внутрь Настроек: открыть нужный раздел и
// подсветить конкретную строку.
//
// Отдельный файл, а не дополнение к `SettingsPage.test.tsx`: тот падает на
// `localStorage.clear is not a function` ещё до наших правок, и новые проверки
// утонули бы вместе с ним.

import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';

import { ThemeProvider } from '../theme/useTheme';
import { SettingsPage } from './SettingsPage';
import { settingDomId } from './settingsIndex';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(() => Promise.resolve(null));
});

afterEach(() => cleanup());

describe('SettingsPage target', () => {
  test('opens the section the palette asked for', async () => {
    render(<SettingsPage target={{ section: 'recording' }} />);
    // Строка из раздела «Запись» — значит открылась именно она, а не дефолтный
    // «Внешний вид».
    expect(await screen.findByText('Останавливать автоматически')).toBeTruthy();
  });

  test('flashes the requested row, then lets the highlight go', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      render(
        <SettingsPage target={{ section: 'recording', highlight: 'silence-auto-stop' }} />,
      );
      await screen.findByText('Останавливать автоматически');

      const row = () => document.getElementById(settingDomId('silence-auto-stop'));
      await waitFor(() => expect(row()?.getAttribute('data-flash')).toBe('true'));

      // Подсветка — ориентир на пару секунд, а не постоянное состояние.
      vi.advanceTimersByTime(3_000);
      await waitFor(() => expect(row()?.getAttribute('data-flash')).toBeNull());
    } finally {
      vi.useRealTimers();
    }
  });

  test('does not steal focus when highlighting', async () => {
    render(<SettingsPage target={{ section: 'recording', highlight: 'silence-auto-stop' }} />);
    await screen.findByText('Останавливать автоматически');
    await waitFor(() =>
      expect(document.getElementById(settingDomId('silence-auto-stop'))).toBeTruthy(),
    );
    // Подсветка не должна перехватывать клавиатуру у того, кто печатал.
    expect(document.activeElement).toBe(document.body);
  });

  test('without a target the default section opens', async () => {
    // «Внешний вид» тянет useTheme — без провайдера раздел падает, и с ним
    // всё дерево. Остальным тестам провайдер не нужен: они про «Запись».
    render(
      <ThemeProvider>
        <SettingsPage />
      </ThemeProvider>,
    );
    expect(await screen.findByText('Тема')).toBeTruthy();
  });
});
