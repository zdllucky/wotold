// [B29.5b] SettingsPage: панель разделов — collapse до полосы иконок,
// навигация из mini, persist. Мок Tauri до импорта (канон CallDetailPage.test).

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => null),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { ToastProvider } from '../ui';
import { ThemeProvider } from '../theme/useTheme';
import { SettingsPage } from './SettingsPage';

function renderPage() {
  return render(
    <ThemeProvider>
      <ToastProvider>
        <SettingsPage />
      </ToastProvider>
    </ThemeProvider>,
  );
}

describe('SettingsPage панель разделов', () => {
  beforeEach(() => localStorage.clear());

  it('collapse → полоса иконок, навигация работает из mini, persist', async () => {
    const { container } = renderPage();
    await waitFor(() =>
      expect(container.querySelector('aside.side-list')).not.toBeNull(),
    );
    const aside = container.querySelector('aside.side-list') as HTMLElement;

    fireEvent.click(screen.getByRole('button', { name: 'Свернуть разделы' }));
    expect(localStorage.getItem('wk-set-collapsed')).toBe('1');
    expect(aside.dataset.collapsed).toBeDefined();
    // NavItem-лейблов нет, mini-иконки разделов есть.
    expect(container.querySelector('.navitem')).toBeNull();
    const mini = container.querySelectorAll('.side-list-mini .iconbtn');
    expect(mini.length).toBeGreaterThan(3);

    // Навигация из mini: клик по иконке раздела «Запись» меняет секцию
    // (кнопка становится active), панель НЕ разворачивается.
    const recBtn = screen.getByRole('button', { name: /Запись/ });
    fireEvent.click(recBtn);
    expect(recBtn.getAttribute('data-active')).toBe('true');
    expect(localStorage.getItem('wk-set-collapsed')).toBe('1');

    fireEvent.click(screen.getByRole('button', { name: 'Развернуть разделы' }));
    await waitFor(() => expect(container.querySelector('.navitem')).not.toBeNull());
  });
});
