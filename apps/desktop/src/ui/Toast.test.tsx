// Smoke tests for the lightweight toast system.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { ToastProvider, useToast } from './Toast';

afterEach(() => cleanup());

function Trigger() {
  const { show } = useToast();
  // duration:0 — без авто-dismiss таймера (чтобы не дёргать unmounted в тесте).
  return (
    <button onClick={() => show({ message: 'hello toast', tone: 'warn', duration: 0 })}>
      go
    </button>
  );
}

describe('Toast', () => {
  test('show renders a .toast with message + tone', () => {
    render(
      <ToastProvider>
        <Trigger />
      </ToastProvider>,
    );
    fireEvent.click(screen.getByText('go'));
    const toast = document.querySelector('.toast');
    expect(toast).toBeTruthy();
    expect(toast!.classList.contains('toast--warn')).toBe(true);
    expect(screen.getByText('hello toast')).toBeInTheDocument();
  });

  test('close button dismisses the toast', () => {
    render(
      <ToastProvider>
        <Trigger />
      </ToastProvider>,
    );
    fireEvent.click(screen.getByText('go'));
    expect(document.querySelector('.toast')).toBeTruthy();
    fireEvent.click(document.querySelector('.toast-close')!);
    expect(document.querySelector('.toast')).toBeNull();
  });

  test('useToast outside provider throws', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => render(<Trigger />)).toThrow(/ToastProvider/);
    spy.mockRestore();
  });
});
