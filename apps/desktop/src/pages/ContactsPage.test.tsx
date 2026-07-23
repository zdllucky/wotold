// [B23] ContactsPage — add/edit только через модалку, панель view-only.

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Contact } from '../api/contacts';

const mkContact = (over: Partial<Contact> = {}): Contact => ({
  id: 'c-1',
  display_name: 'Иван Петров',
  is_owner: false,
  org: null,
  role: 'CTO',
  attributes: {},
  notes: null,
  created_at: 'now',
  updated_at: 'now',
  source: 'local',
  external_id: null,
  external_etag: null,
  identifiers: [],
  ...over,
});

vi.mock('../api/contacts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/contacts')>();
  return {
    ...actual,
    listContacts: vi.fn(async () => [mkContact()]),
    createContact: vi.fn(async () => mkContact({ id: 'c-new', display_name: 'Анна' })),
    updateContact: vi.fn(async () => mkContact()),
    deleteContact: vi.fn(async () => {}),
  };
});
vi.mock('../api/recording', () => ({ listCalls: vi.fn(async () => []) }));
vi.mock('../api/speakers', () => ({ listCallSpeakers: vi.fn(async () => []) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: vi.fn(async () => true) }));
vi.mock('./VoiceSamplesSection', () => ({ VoiceSamplesSection: () => null }));

import { ContactsPage } from './ContactsPage';

afterEach(() => {
  cleanup();
  localStorage.clear(); // [B29.5b] collapse-стейт панели не течёт между тестами
});

describe('ContactsPage', () => {
  it('add button opens dialog «Новый контакт»; no inline form in detail pane', async () => {
    render(<ContactsPage />);
    await waitFor(() => expect(screen.getAllByText('Иван Петров').length).toBeGreaterThan(0));
    expect(screen.queryByRole('dialog')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Добавить контакт' }));
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.getByText('Новый контакт')).toBeTruthy();
  });

  it('edit opens dialog prefilled with contact name', async () => {
    render(<ContactsPage />);
    await waitFor(() => expect(screen.getAllByText('Иван Петров').length).toBeGreaterThan(0));

    fireEvent.click(screen.getByRole('button', { name: /редактировать/i }));
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeTruthy();
    expect(screen.getByText('Редактировать контакт')).toBeTruthy();
    expect((screen.getByLabelText('Имя') as HTMLInputElement).value).toBe('Иван Петров');
  });

  it('successful create closes the dialog', async () => {
    render(<ContactsPage />);
    await waitFor(() => expect(screen.getAllByText('Иван Петров').length).toBeGreaterThan(0));

    fireEvent.click(screen.getByRole('button', { name: 'Добавить контакт' }));
    fireEvent.change(screen.getByLabelText('Имя'), { target: { value: 'Анна' } });
    fireEvent.submit(document.querySelector('form')!);
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('failed create keeps dialog open with error visible INSIDE it', async () => {
    const { createContact } = await import('../api/contacts');
    (createContact as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('db locked'));

    render(<ContactsPage />);
    await waitFor(() => expect(screen.getAllByText('Иван Петров').length).toBeGreaterThan(0));

    fireEvent.click(screen.getByRole('button', { name: 'Добавить контакт' }));
    fireEvent.change(screen.getByLabelText('Имя'), { target: { value: 'Анна' } });
    fireEvent.submit(document.querySelector('form')!);

    await waitFor(() => {
      const dialog = screen.getByRole('dialog');
      // Ошибка обязана рендериться внутри диалога — панель под оверлеем не видна.
      const alert = dialog.querySelector('[role="alert"]');
      expect(alert?.textContent).toContain('db locked');
    });
  });
});

// ── [B29.4-5b] Панель списка: side-list + collapse до полосы аватаров ──
describe('ContactsPage панель', () => {
  it('aside — .side-list (без .rrail); collapse → аватары, клик открывает, persist', async () => {
    const { container } = render(<ContactsPage />);
    await waitFor(() => expect(screen.getAllByText('Иван Петров').length).toBeGreaterThan(0));
    const aside = container.querySelector('aside') as HTMLElement;
    expect(aside.className).toContain('side-list');
    expect(aside.className).not.toContain('rrail');

    fireEvent.click(screen.getByRole('button', { name: 'Свернуть список контактов' }));
    expect(localStorage.getItem('wk-ct-collapsed')).toBe('1');
    // Строк .lrow нет, вместо них аватар-кнопка с именем.
    expect(container.querySelector('.lrow')).toBeNull();
    const avatarBtn = container.querySelector('.side-list-mini .avatar') as HTMLElement;
    expect(avatarBtn.getAttribute('aria-label')).toBe('Иван Петров');

    fireEvent.click(avatarBtn); // клик открывает контакт (уже открыт — не падает)
    fireEvent.click(screen.getByRole('button', { name: 'Развернуть список контактов' }));
    await waitFor(() => expect(container.querySelector('.lrow')).not.toBeNull());
  });
});
