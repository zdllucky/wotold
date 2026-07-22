// [B24.2] useAssistantChats: optimistic ask-flow, pending, delete, ошибка.
// Мок Tauri ДО импорта хука (vitest hoisting — канон CallDetailPage.test).

import { describe, expect, it, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from '@tauri-apps/api/core';
import type { AssistantMessage } from '@wotold/contracts';
import { useAssistantChats } from './useAssistantChats';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

function assistantMsg(id: string, text: string): AssistantMessage {
  return {
    id,
    role: 'assistant',
    text,
    answer: {
      kind: 'answer',
      text,
      sources: [],
      fragments: [],
      fragmentTokens: 100,
      windowTokens: 8192,
    },
    createdAt: '2026-07-22T10:00:01Z',
  };
}

function userMsg(id: string, text: string): AssistantMessage {
  return { id, role: 'user', text, answer: null, createdAt: '2026-07-22T10:00:00Z' };
}

describe('useAssistantChats', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'assistant_list_chats') return [];
      return null;
    });
  });

  it('ask: optimistic user-message → pending → тред из БД', async () => {
    let resolveAsk: (v: unknown) => void = () => {};
    const askPromise = new Promise((r) => {
      resolveAsk = r;
    });
    const answerMessage = assistantMsg('m2', 'ответ');
    const finalThread = [userMsg('m1', 'вопрос?'), answerMessage];
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'assistant_list_chats')
        return [{ id: 'chat-1', callId: null, title: 'вопрос?', createdAt: '' }];
      if (cmd === 'assistant_ask') return askPromise;
      if (cmd === 'assistant_get_chat') {
        expect((args as { chatId: string }).chatId).toBe('chat-1');
        return finalThread;
      }
      return null;
    });

    const { result } = renderHook(() => useAssistantChats());
    act(() => {
      void result.current.ask('вопрос?');
    });

    // Optimistic: вопрос виден сразу, pending активен.
    await waitFor(() => expect(result.current.pending).toBe(true));
    expect(result.current.messages.at(-1)?.text).toBe('вопрос?');
    expect(result.current.messages.at(-1)?.role).toBe('user');

    act(() => {
      resolveAsk({ chatId: 'chat-1', message: answerMessage });
    });
    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(result.current.activeChatId).toBe('chat-1');
    expect(result.current.messages).toHaveLength(2);
    expect(result.current.messages[1]?.answer?.kind).toBe('answer');
    expect(result.current.error).toBeNull();
  });

  it('ошибка ask: pending снят, error установлен, optimistic откатывается', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'assistant_list_chats') return [];
      if (cmd === 'assistant_ask') throw new Error('llm boom');
      return null;
    });
    const { result } = renderHook(() => useAssistantChats());
    // Дождаться initial refresh + чистого состояния нового чата.
    await waitFor(() => expect(mockInvoke).toHaveBeenCalled());
    act(() => {
      result.current.startNewChat();
    });

    await act(async () => {
      await result.current.ask('вопрос?');
    });
    expect(result.current.pending).toBe(false);
    expect(result.current.error).not.toBeNull();
    expect(result.current.messages).toHaveLength(0);
  });

  it('deleteChat активного чата сбрасывает тред и обновляет список', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'assistant_list_chats') return [];
      if (cmd === 'assistant_get_chat') return [userMsg('m1', 'старый')];
      if (cmd === 'assistant_delete_chat') return null;
      return null;
    });
    const { result } = renderHook(() => useAssistantChats());
    await act(async () => {
      await result.current.openChat('chat-x');
    });
    expect(result.current.messages).toHaveLength(1);

    await act(async () => {
      await result.current.deleteChat('chat-x');
    });
    expect(result.current.activeChatId).toBeNull();
    expect(result.current.messages).toHaveLength(0);
    expect(mockInvoke).toHaveBeenCalledWith('assistant_delete_chat', { chatId: 'chat-x' });
  });
});
