// [B23] ContactFormModal — канонная модалка формы контакта. Ключевое:
// submit-shape (identifiers/attributes/consent_voice) сохранён байт-в-байт,
// disabled-гейт по имени, переведённые kind-опции, label-passthrough.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ContactFormModal } from './ContactFormModal';
import type { Contact } from '../api/contacts';

afterEach(() => cleanup());

function contact(over: Partial<Contact> = {}): Contact {
  return {
    id: 'c-1',
    display_name: 'Иван Петров',
    is_owner: false,
    org: 'Acme',
    role: 'CTO',
    attributes: { birthday: '1990', consent_voice: 'true' },
    notes: null,
    created_at: 'now',
    updated_at: 'now',
    source: 'local',
    external_id: null,
    external_etag: null,
    identifiers: [{ id: 'i-1', kind: 'email', value: 'ivan@acme.kz', label: 'work' }],
    ...over,
  };
}

const submitForm = (container: HTMLElement) => {
  fireEvent.submit(container.querySelector('form')!);
};

describe('ContactFormModal', () => {
  it('renders as dialog titled «Новый контакт» in create mode', () => {
    render(<ContactFormModal contact={null} onClose={() => {}} onSubmit={() => {}} />);
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.getByText('Новый контакт')).toBeTruthy();
  });

  it('submit disabled while name empty, enabled after typing', () => {
    render(<ContactFormModal contact={null} onClose={() => {}} onSubmit={() => {}} />);
    const submit = screen.getByRole('button', { name: /создать/i }) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('Имя'), { target: { value: 'Анна' } });
    expect(submit.disabled).toBe(false);
  });

  it('submits trimmed shape with identifiers, attributes and consent_voice', () => {
    const onSubmit = vi.fn();
    const { container } = render(
      <ContactFormModal contact={null} onClose={() => {}} onSubmit={onSubmit} />,
    );
    fireEvent.change(screen.getByLabelText('Имя'), { target: { value: '  Анна  ' } });
    // добавить идентификатор
    fireEvent.click(screen.getAllByRole('button', { name: 'Добавить' })[0]!);
    fireEvent.change(screen.getByPlaceholderText('значение'), { target: { value: '+7700' } });
    // consent switch
    fireEvent.click(screen.getByRole('switch'));
    submitForm(container);

    expect(onSubmit).toHaveBeenCalledWith({
      display_name: 'Анна',
      org: undefined,
      role: undefined,
      notes: undefined,
      identifiers: [{ kind: 'phone', value: '+7700' }],
      attributes: { consent_voice: 'true' },
    });
  });

  it('edit mode: untouched submit preserves attributes, consent and identifier label', () => {
    const onSubmit = vi.fn();
    const { container } = render(
      <ContactFormModal contact={contact()} onClose={() => {}} onSubmit={onSubmit} />,
    );
    expect(screen.getByText('Редактировать контакт')).toBeTruthy();
    submitForm(container);

    const payload = onSubmit.mock.calls[0]![0];
    expect(payload.display_name).toBe('Иван Петров');
    expect(payload.attributes).toEqual({ birthday: '1990', consent_voice: 'true' });
    expect(payload.identifiers).toEqual([{ kind: 'email', value: 'ivan@acme.kz', label: 'work' }]);
  });

  it('identifier kind options are translated (Телефон, не raw phone)', () => {
    render(<ContactFormModal contact={null} onClose={() => {}} onSubmit={() => {}} />);
    fireEvent.click(screen.getAllByRole('button', { name: 'Добавить' })[0]!);
    // Триггер Select показывает переведённый label дефолтного kind'а.
    expect(screen.getByText('Телефон')).toBeTruthy();
    expect(screen.queryByText(/^phone$/)).toBeNull();
  });

  it('cancel calls onClose', () => {
    const onClose = vi.fn();
    render(<ContactFormModal contact={null} onClose={onClose} onSubmit={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: /отмена/i }));
    expect(onClose).toHaveBeenCalled();
  });
});
