// [B24.2/ревью H7] useCallAssistant: загрузка треда, optimistic ask,
// откат при ошибке, отбрасывание устаревшего ответа при смене звонка.

import { describe, expect, it, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import type { AssistantMessage } from '@wotold/contracts';
import { useCallAssistant } from './useCallAssistant';

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

function userMsg(id: string, text: string): AssistantMessage {
  return { id, role: 'user', text, answer: null, createdAt: '2026-07-22T10:00:00Z' };
}

describe('useCallAssistant', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('грузит существующий тред звонка при mount', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'assistant_get_call_thread') {
        expect((args as { callId: string }).callId).toBe('call-1');
        return {
          chat: { id: 'chat-1', callId: 'call-1', title: 'тред', createdAt: '' },
          messages: [userMsg('m1', 'старый вопрос')],
        };
      }
      return null;
    });
    const { result } = renderHook(() => useCallAssistant('call-1'));
    await waitFor(() => expect(result.current.messages).toHaveLength(1));
  });

  it('ask: optimistic → тред из БД; chatId переиспользуется', async () => {
    const thread = [userMsg('m1', 'вопрос'), userMsg('m2', 'ответ-как-msg')];
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'assistant_get_call_thread') return null; // треда ещё нет
      if (cmd === 'assistant_ask') {
        const a = (args as { args: { callId: string | null; chatId: string | null } }).args;
        expect(a.callId).toBe('call-1');
        expect(a.chatId).toBeNull();
        return { chatId: 'chat-new', message: thread[1] };
      }
      if (cmd === 'assistant_get_chat') return thread;
      return null;
    });
    const { result } = renderHook(() => useCallAssistant('call-1'));
    await act(async () => {
      await result.current.ask('вопрос');
    });
    expect(result.current.messages).toHaveLength(2);
    expect(result.current.pending).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('ошибка ask без существующего треда: optimistic откатывается', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'assistant_get_call_thread') return null;
      if (cmd === 'assistant_ask') throw new Error('boom');
      return null;
    });
    const { result } = renderHook(() => useCallAssistant('call-1'));
    await act(async () => {
      await result.current.ask('вопрос');
    });
    expect(result.current.messages).toHaveLength(0);
    expect(result.current.error).not.toBeNull();
    expect(result.current.pending).toBe(false);
  });

  it('смена звонка во время pending: устаревший ответ отброшен', async () => {
    let resolveAsk: (v: unknown) => void = () => {};
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'assistant_get_call_thread') return null;
      if (cmd === 'assistant_ask') return new Promise((r) => (resolveAsk = r));
      if (cmd === 'assistant_get_chat') return [userMsg('old', 'из старого звонка')];
      return null;
    });
    const { result, rerender } = renderHook(({ id }) => useCallAssistant(id), {
      initialProps: { id: 'call-1' },
    });
    act(() => {
      void result.current.ask('вопрос по call-1');
    });
    await waitFor(() => expect(result.current.pending).toBe(true));

    // Юзер ушёл на другой звонок — тред сброшен.
    rerender({ id: 'call-2' });
    await waitFor(() => expect(result.current.messages).toHaveLength(0));

    // Старый ask доехал — вид call-2 не перезаписан старым тредом.
    act(() => {
      resolveAsk({ chatId: 'chat-old', message: userMsg('old', 'из старого звонка') });
    });
    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(result.current.messages).toHaveLength(0);
  });
});
