// [B24.4] AssistantPage: empty-состояние, новый чат первым вопросом,
// группировка по дням, удаление. Мок Tauri до импорта (канон CallDetailPage.test).

import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from '@tauri-apps/api/core';
import { ToastProvider } from '../ui';
import { resetAssistantChatsCacheForTests } from '../hooks/useAssistantChats';
import { AssistantPage } from './AssistantPage';
import { SUGGESTIONS } from '../components/assistant/suggestions';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

function renderPage(onOpenCall: (id: string) => void = () => {}) {
  return render(
    <ToastProvider>
      <AssistantPage onOpenCall={onOpenCall} />
    </ToastProvider>,
  );
}

const STATS = { indexedCalls: 9, totalCalls: 19, totalDurationSec: 3720 };

function todayIso(): string {
  return new Date().toISOString();
}
function yesterdayIso(): string {
  return new Date(Date.now() - 86_400_000).toISOString();
}

describe('AssistantPage', () => {
  beforeEach(() => {
    resetAssistantChatsCacheForTests();
    localStorage.clear();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'assistant_index_stats') return STATS;
      if (cmd === 'assistant_list_chats') return [];
      if (cmd === 'assistant_get_chat') return [];
      return null;
    });
  });

  it('empty-состояние: заголовок, 4 случайные подсказки из пула, чип статистики', async () => {
    const { container } = renderPage();
    expect(screen.getByText('Поиск по всем звонкам')).toBeInTheDocument();
    // [B27.4] Ровно 4 чипа, каждый из ru-пула 50 подсказок.
    const chips = container.querySelectorAll('.ask-suggest button');
    expect(chips).toHaveLength(4);
    for (const chip of Array.from(chips)) {
      expect(SUGGESTIONS.ru).toContain(chip.textContent);
    }
    await waitFor(() =>
      expect(screen.getByText('в поиске 9 из 19 звонков · 1 ч 2 мин')).toBeInTheDocument(),
    );
    expect(screen.getByText('Чатов пока нет')).toBeInTheDocument();
  });

  it('вопрос из композера создаёт новый чат (chatId null)', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'assistant_index_stats') return STATS;
      if (cmd === 'assistant_list_chats') return [];
      if (cmd === 'assistant_ask') {
        const a = (args as { args: { chatId: string | null; question: string } }).args;
        expect(a.chatId).toBeNull();
        expect(a.question).toBe('мой вопрос');
        return {
          chatId: 'chat-1',
          message: { id: 'm2', role: 'assistant', text: 'ответ', answer: null, createdAt: todayIso() },
        };
      }
      if (cmd === 'assistant_get_chat')
        return [
          { id: 'm1', role: 'user', text: 'мой вопрос', answer: null, createdAt: todayIso() },
          { id: 'm2', role: 'assistant', text: 'ответ', answer: null, createdAt: todayIso() },
        ];
      return null;
    });
    renderPage();
    const input = screen.getByPlaceholderText('Спросить по всем звонкам…');
    fireEvent.change(input, { target: { value: 'мой вопрос' } });
    fireEvent.submit(input.closest('form') as HTMLFormElement);
    await waitFor(() => expect(screen.getByText('ответ')).toBeInTheDocument());
  });

  it('группировка чатов по дням + удаление', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'assistant_index_stats') return STATS;
      if (cmd === 'assistant_list_chats')
        return [
          { id: 'c-today', callId: null, title: 'Сегодняшний чат', createdAt: todayIso() },
          { id: 'c-yest', callId: null, title: 'Вчерашний чат', createdAt: yesterdayIso() },
        ];
      if (cmd === 'assistant_delete_chat') {
        expect((args as { chatId: string }).chatId).toBe('c-today');
        return null;
      }
      return null;
    });
    renderPage();
    await waitFor(() => expect(screen.getByText('Сегодняшний чат')).toBeInTheDocument());
    expect(screen.getByText('Сегодня')).toBeInTheDocument();
    expect(screen.getByText('Вчера')).toBeInTheDocument();

    // [B24.7 a11y C1] Строка чата — <li> с кнопкой открытия и соседней
    // кнопкой удаления (не вложенной).
    const row = screen.getByText('Сегодняшний чат').closest('li') as HTMLElement;
    fireEvent.click(row.querySelector('[aria-label="Удалить чат"]') as HTMLElement);
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith('assistant_delete_chat', { chatId: 'c-today' }),
    );
  });
});


// ── [B26.11] Панель чатов: clamp, collapse, fuzzy-поиск, persist ──

describe('панель чатов', () => {
  const chat = (id: string, title: string) => ({
    id,
    callId: null,
    title,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  });

  beforeEach(() => {
    resetAssistantChatsCacheForTests();
    localStorage.clear();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'assistant_index_stats')
        return { indexedCalls: 1, totalCalls: 1, totalDurationSec: 60 };
      if (cmd === 'assistant_list_chats')
        return [chat('c1', 'Планёрки итоги июня'), chat('c2', 'Бюджет на квартал')];
      if (cmd === 'assistant_get_chat') return [];
      return null;
    });
  });

  // [B27.3] Открытый чат показывает свой титул в хедере вместо чипа статистики.
  it('активный чат → его название в .view-head, statsChip скрыт', async () => {
    const { container } = renderPage();
    await waitFor(() => expect(screen.getByText('Планёрки итоги июня')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /Планёрки итоги июня/ }));
    await waitFor(() => {
      const head = container.querySelector('.view-head') as HTMLElement;
      expect(head.querySelector('.as-head-chat')?.textContent).toBe('Планёрки итоги июня');
    });
    expect(screen.queryByText(/в поиске 1 из 1/)).not.toBeInTheDocument();
  });

  it('collapse скрывает список, expand возвращает; persist; «Новый чат» в шапке', async () => {
    const { container } = renderPage();
    await waitFor(() =>
      expect(screen.getByText('Планёрки итоги июня')).toBeInTheDocument(),
    );
    // [B29.2-3] Поиск и «Новый чат» живут в ViewHead.
    const head = container.querySelector('.view-head') as HTMLElement;
    expect(head.contains(screen.getByLabelText('Поиск по чатам…'))).toBe(true);
    expect(head.contains(screen.getByRole('button', { name: 'Новый чат' }))).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: 'Свернуть список чатов' }));
    expect(screen.queryByText('Планёрки итоги июня')).not.toBeInTheDocument();
    expect(localStorage.getItem('wk-aschats-collapsed')).toBe('1');
    // «Новый чат» и поиск остаются в шапке даже при свёрнутой панели.
    expect(screen.getByRole('button', { name: 'Новый чат' })).toBeInTheDocument();
    expect(screen.getByLabelText('Поиск по чатам…')).toBeInTheDocument();
    // Клик «Новый чат» НЕ разворачивает панель.
    fireEvent.click(screen.getByRole('button', { name: 'Новый чат' }));
    expect(localStorage.getItem('wk-aschats-collapsed')).toBe('1');

    fireEvent.click(screen.getByRole('button', { name: 'Развернуть список чатов' }));
    expect(await screen.findByText('Планёрки итоги июня')).toBeInTheDocument();
  });

  it('fuzzy-поиск фильтрует чаты, Esc сбрасывает', async () => {
    renderPage();
    await waitFor(() => expect(screen.getByText('Бюджет на квартал')).toBeInTheDocument());
    const input = screen.getByLabelText('Поиск по чатам…');
    fireEvent.change(input, { target: { value: 'плнрк' } });
    expect(screen.getByText('Планёрки итоги июня')).toBeInTheDocument();
    expect(screen.queryByText('Бюджет на квартал')).not.toBeInTheDocument();

    fireEvent.change(input, { target: { value: 'qqqq' } });
    expect(screen.getByText('Чатов по запросу не найдено')).toBeInTheDocument();

    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.getByText('Бюджет на квартал')).toBeInTheDocument();
  });
});
