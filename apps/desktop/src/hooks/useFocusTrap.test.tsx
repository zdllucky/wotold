// @vitest-environment jsdom
import { useRef } from 'react';
import { describe, expect, test, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/react';
import { useFocusTrap, type FocusTrapOptions } from './useFocusTrap';

function Modal({
  open,
  onClose,
  options,
}: {
  open: boolean;
  onClose?: () => void;
  options?: FocusTrapOptions;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, open, { onClose, ...options });
  if (!open) return null;
  return (
    <div ref={ref} data-testid="modal" role="dialog" aria-modal="true">
      <button>first</button>
      <button>middle</button>
      <button>last</button>
    </div>
  );
}

describe('useFocusTrap', () => {
  test('initial focus moves to first focusable inside root', () => {
    render(<Modal open />);
    expect(document.activeElement?.textContent).toBe('first');
  });

  test('ESC calls onClose when provided', () => {
    const onClose = vi.fn();
    const { getByTestId } = render(<Modal open onClose={onClose} />);
    fireEvent.keyDown(getByTestId('modal'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test('ESC is no-op when onClose omitted', () => {
    const { getByTestId } = render(<Modal open />);
    // Should not throw, should not move focus.
    fireEvent.keyDown(getByTestId('modal'), { key: 'Escape' });
    expect(document.activeElement?.textContent).toBe('first');
  });

  test('Shift+Tab on first focusable cycles to last', () => {
    const { getByText, getByTestId } = render(<Modal open />);
    const first = getByText('first') as HTMLButtonElement;
    first.focus();
    fireEvent.keyDown(getByTestId('modal'), {
      key: 'Tab',
      shiftKey: true,
    });
    expect(document.activeElement?.textContent).toBe('last');
  });

  test('Tab on last focusable cycles to first', () => {
    const { getByText, getByTestId } = render(<Modal open />);
    const last = getByText('last') as HTMLButtonElement;
    last.focus();
    fireEvent.keyDown(getByTestId('modal'), { key: 'Tab' });
    expect(document.activeElement?.textContent).toBe('first');
  });

  test('inactive prop disables trap', () => {
    const onClose = vi.fn();
    const { container } = render(
      <div>
        <button>outside</button>
        <Modal open={false} onClose={onClose} />
      </div>,
    );
    // Modal not rendered.
    expect(container.querySelector('[data-testid="modal"]')).toBeNull();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });

  test('lockScroll=true sets body overflow:hidden while open', () => {
    const { rerender } = render(<Modal open />);
    expect(document.body.style.overflow).toBe('hidden');
    rerender(<Modal open={false} />);
    expect(document.body.style.overflow).toBe('');
  });

  test('lockScroll=false leaves body overflow untouched', () => {
    document.body.style.overflow = 'auto';
    render(<Modal open options={{ lockScroll: false }} />);
    expect(document.body.style.overflow).toBe('auto');
    // cleanup
    document.body.style.overflow = '';
  });
});
