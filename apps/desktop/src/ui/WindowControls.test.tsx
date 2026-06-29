// Smoke tests for WindowControls — macOS-светофор (close/min/fullscreen).

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, test, vi } from 'vitest';

const close = vi.fn(() => Promise.resolve());
const minimize = vi.fn(() => Promise.resolve());
const setFullscreen = vi.fn(() => Promise.resolve());
const isFullscreen = vi.fn(() => Promise.resolve(false));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ close, minimize, setFullscreen, isFullscreen }),
}));

import { WindowControls } from './WindowControls';

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const noop = () => {};

describe('WindowControls', () => {
  test('renders 3 traffic-light buttons with aria-labels', () => {
    render(<WindowControls open onOpen={noop} onClose={noop} />);
    expect(document.querySelectorAll('.wc-btn').length).toBe(3);
    expect(screen.getByLabelText(/закрыть|close|жабу/i)).toBeTruthy();
    expect(screen.getByLabelText(/свернуть|minimize|жию/i)).toBeTruthy();
    expect(screen.getByLabelText(/весь экран|fullscreen|толық/i)).toBeTruthy();
  });

  test('close/minimize call the matching window APIs', () => {
    render(<WindowControls open onOpen={noop} onClose={noop} />);
    fireEvent.click(document.querySelector('.wc-btn--close')!);
    fireEvent.click(document.querySelector('.wc-btn--min')!);
    expect(close).toHaveBeenCalledTimes(1);
    expect(minimize).toHaveBeenCalledTimes(1);
  });

  test('max toggles fullscreen (isFullscreen → setFullscreen)', async () => {
    render(<WindowControls open onOpen={noop} onClose={noop} />);
    fireEvent.click(document.querySelector('.wc-btn--max')!);
    await waitFor(() => expect(setFullscreen).toHaveBeenCalledWith(true));
    expect(isFullscreen).toHaveBeenCalled();
  });

  test('onOpen/onClose fire on hover of the wrapper', () => {
    const onOpen = vi.fn();
    const onClose = vi.fn();
    render(<WindowControls open={false} onOpen={onOpen} onClose={onClose} />);
    const wrap = document.querySelector('.win-controls')!;
    fireEvent.mouseEnter(wrap);
    expect(onOpen).toHaveBeenCalled();
    fireEvent.mouseLeave(wrap);
    expect(onClose).toHaveBeenCalled();
  });
});
