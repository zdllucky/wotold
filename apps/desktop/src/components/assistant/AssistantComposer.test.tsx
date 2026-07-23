// [B24.4/ревью] AssistantComposer: Enter-путь (в форме нет submit-кнопки),
// trim, disabled, кнопка send. [B27.7] textarea: Shift+Enter = перенос, IME.

import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';

import { AssistantComposer } from './AssistantComposer';

function setup(overrides: Partial<Parameters<typeof AssistantComposer>[0]> = {}) {
  const onAsk = vi.fn();
  render(
    <AssistantComposer placeholder="Спросить…" icon="search" onAsk={onAsk} {...overrides} />,
  );
  const input = screen.getByPlaceholderText('Спросить…') as HTMLTextAreaElement;
  return { onAsk, input };
}

describe('AssistantComposer', () => {
  it('Enter отправляет и чистит драфт', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: '  вопрос  ' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAsk).toHaveBeenCalledWith('вопрос');
    expect(input.value).toBe('');
  });

  it('submit формы отправляет', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: 'вопрос' } });
    fireEvent.submit(input.closest('form') as HTMLFormElement);
    expect(onAsk).toHaveBeenCalledWith('вопрос');
  });

  it('кнопка send отправляет', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: 'вопрос' } });
    fireEvent.click(screen.getByLabelText('Отправить'));
    expect(onAsk).toHaveBeenCalledWith('вопрос');
  });

  // [B27.7] Многострочность.
  it('Shift+Enter не отправляет и не чистит драфт (перенос строки)', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: 'строка' } });
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
    expect(onAsk).not.toHaveBeenCalled();
    expect(input.value).toBe('строка');
  });

  it('Enter во время IME-набора (isComposing) не отправляет', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: 'текст' } });
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
    expect(onAsk).not.toHaveBeenCalled();
    expect(input.value).toBe('текст');
  });

  it('многострочный драфт отправляется как есть (с внутренними переносами)', () => {
    const { onAsk, input } = setup();
    fireEvent.change(input, { target: { value: 'строка 1\nстрока 2' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAsk).toHaveBeenCalledWith('строка 1\nстрока 2');
  });

  it('пустой драфт и disabled не отправляются, драфт сохраняется', () => {
    const { onAsk, input } = setup({ disabled: true });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAsk).not.toHaveBeenCalled();
    // disabled: текст не должен стираться молча (ревью H2 чанка).
    fireEvent.change(input, { target: { value: 'висящий вопрос' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAsk).not.toHaveBeenCalled();
    expect(input.value).toBe('висящий вопрос');
  });
});
