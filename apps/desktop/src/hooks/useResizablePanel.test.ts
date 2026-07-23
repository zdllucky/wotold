// [B29.5a] useResizablePanel: clamp, persist, чтение, drag→авто-collapse.

import { beforeEach, describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';

import { clampPanelWidth, useResizablePanel } from './useResizablePanel';

const OPTS = {
  min: 180,
  max: 400,
  defaultWidth: 232,
  collapseAt: 150,
  widthKey: 'test-w',
  collapsedKey: 'test-collapsed',
};

describe('clampPanelWidth', () => {
  it('держит границы (наследник clampChatsWidth 180-400)', () => {
    expect(clampPanelWidth(100, 180, 400)).toBe(180);
    expect(clampPanelWidth(500, 180, 400)).toBe(400);
    expect(clampPanelWidth(250, 180, 400)).toBe(250);
  });
});

describe('useResizablePanel', () => {
  beforeEach(() => localStorage.clear());

  it('дефолт при пустом/мусорном localStorage, чтение валидного', () => {
    const a = renderHook(() => useResizablePanel(OPTS));
    expect(a.result.current.width).toBe(232);
    expect(a.result.current.collapsed).toBe(false);
    a.unmount();

    localStorage.setItem('test-w', 'мусор');
    localStorage.setItem('test-collapsed', '1');
    const b = renderHook(() => useResizablePanel(OPTS));
    expect(b.result.current.width).toBe(232);
    expect(b.result.current.collapsed).toBe(true);
    b.unmount();

    localStorage.setItem('test-w', '300');
    const c = renderHook(() => useResizablePanel(OPTS));
    expect(c.result.current.width).toBe(300);
    // Вне диапазона → дефолт.
    localStorage.setItem('test-w', '9999');
    const d = renderHook(() => useResizablePanel(OPTS));
    expect(d.result.current.width).toBe(232);
  });

  it('persist пишет ширину и collapse', () => {
    const { result } = renderHook(() => useResizablePanel(OPTS));
    act(() => result.current.setCollapsed(true));
    expect(localStorage.getItem('test-collapsed')).toBe('1');
    expect(localStorage.getItem('test-w')).toBe('232');
  });

  it('drag ниже collapseAt → авто-collapse и снятие листенеров', () => {
    const { result } = renderHook(() => useResizablePanel(OPTS));
    act(() => {
      result.current.onResizeStart({
        preventDefault: () => {},
        clientX: 232,
      } as unknown as React.MouseEvent);
    });
    act(() => {
      document.dispatchEvent(new MouseEvent('mousemove', { clientX: 60 })); // w = 60 < 150
    });
    expect(result.current.collapsed).toBe(true);
    expect(document.body.style.cursor).toBe('');
  });

  it('drag в диапазоне меняет ширину с clamp', () => {
    const { result } = renderHook(() => useResizablePanel(OPTS));
    act(() => {
      result.current.onResizeStart({
        preventDefault: () => {},
        clientX: 232,
      } as unknown as React.MouseEvent);
    });
    act(() => {
      document.dispatchEvent(new MouseEvent('mousemove', { clientX: 532 })); // w = 532 → clamp 400
    });
    expect(result.current.width).toBe(400);
    act(() => {
      document.dispatchEvent(new MouseEvent('mouseup'));
    });
    expect(document.body.style.cursor).toBe('');
  });
});

describe('unmount-cleanup', () => {
  it('unmount посреди драга снимает листенеры и body-стили', () => {
    localStorage.clear();
    const { result, unmount } = renderHook(() => useResizablePanel(OPTS));
    act(() => {
      result.current.onResizeStart({
        preventDefault: () => {},
        clientX: 232,
      } as unknown as React.MouseEvent);
    });
    expect(document.body.style.cursor).toBe('col-resize');
    unmount(); // смена view посреди драга (ревью B29 HIGH)
    expect(document.body.style.cursor).toBe('');
    expect(document.body.style.userSelect).toBe('');
    // Поздний mousemove не падает и ничего не делает.
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 60 }));
  });
});

describe('drag-expand из свёрнутого состояния', () => {
  it('вытягивание за collapseAt разворачивает; недотяг оставляет свёрнутой', () => {
    localStorage.clear();
    localStorage.setItem('test-collapsed', '1');
    const { result } = renderHook(() => useResizablePanel(OPTS));
    expect(result.current.collapsed).toBe(true);

    act(() => {
      result.current.onResizeStart({
        preventDefault: () => {},
        clientX: 48,
      } as unknown as React.MouseEvent);
    });
    // Недотяг: 48 + 50 = 98 < 150 → остаёмся свёрнутыми.
    act(() => {
      document.dispatchEvent(new MouseEvent('mousemove', { clientX: 98 }));
    });
    expect(result.current.collapsed).toBe(true);
    // Дотянули: 48 + 200 = 248 > 150 → развернулись на 248.
    act(() => {
      document.dispatchEvent(new MouseEvent('mousemove', { clientX: 248 }));
    });
    expect(result.current.collapsed).toBe(false);
    expect(result.current.width).toBe(248);
    // Пока мышь зажата — можно утащить обратно в collapse.
    act(() => {
      document.dispatchEvent(new MouseEvent('mousemove', { clientX: 90 }));
    });
    expect(result.current.collapsed).toBe(true);
    act(() => {
      document.dispatchEvent(new MouseEvent('mouseup'));
    });
    expect(document.body.style.cursor).toBe('');
  });
});
