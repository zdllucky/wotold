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
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'assistant_index_stats') return STATS;
      if (cmd === 'assistant_list_chats') return [];
      if (cmd === 'assistant_get_chat') return [];
      return null;
    });
  });

  it('empty-состояние: заголовок, 4 подсказки, чип статистики', async () => {
    renderPage();
    expect(screen.getByText('Поиск по всем звонкам')).toBeInTheDocument();
    expect(screen.getByText('Когда обсуждали приватность?')).toBeInTheDocument();
    expect(screen.getByText('Решения планёрки продукта')).toBeInTheDocument();
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

    const row = screen.getByText('Сегодняшний чат').closest('[role="button"]') as HTMLElement;
    fireEvent.click(row.querySelector('[aria-label="Удалить чат"]') as HTMLElement);
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith('assistant_delete_chat', { chatId: 'c-today' }),
    );
  });
});
