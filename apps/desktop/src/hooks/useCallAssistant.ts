// [B24.2] Тред ассистента внутри звонка (вкладка «Ассистент», SPEC §3):
// один персистентный чат на звонок, создаётся первым вопросом.
// Гонки (ревью B24): двойной ask — синхронный pendingRef; смена звонка
// во время pending — устаревший ответ отбрасывается (guard по currentCallRef).

import { useCallback, useEffect, useRef, useState } from 'react';
import type { AssistantMessage } from '@wotold/contracts';

import { askAssistant, getAssistantCallThread, getAssistantChat } from '../api/assistant';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';

let optimisticSeq = 0;

export interface UseCallAssistant {
  messages: AssistantMessage[];
  pending: boolean;
  error: string | null;
  ask: (question: string) => Promise<void>;
}

export function useCallAssistant(callId: string): UseCallAssistant {
  // [TD-25] Тексты ошибок берутся из словаря — humanError требует `t`.
  const { t } = useI18n();
  const [messages, setMessages] = useState<AssistantMessage[]>([]);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const chatIdRef = useRef<string | null>(null);
  const currentCallRef = useRef(callId);
  const pendingRef = useRef(false);

  useEffect(() => {
    currentCallRef.current = callId;
    let cancelled = false;
    chatIdRef.current = null;
    setMessages([]);
    setError(null);
    getAssistantCallThread(callId)
      .then((thread) => {
        if (cancelled || currentCallRef.current !== callId || !thread) return;
        chatIdRef.current = thread.chat.id;
        setMessages(thread.messages);
      })
      .catch((e) => console.warn('assistant call thread load:', e));
    return () => {
      cancelled = true;
    };
  }, [callId]);

  const ask = useCallback(
    async (question: string) => {
      const q = question.trim();
      if (!q || pendingRef.current) return;
      pendingRef.current = true;
      const askCallId = callId;
      setError(null);
      const optimistic: AssistantMessage = {
        id: `optimistic-${++optimisticSeq}`,
        role: 'user',
        text: q,
        answer: null,
        createdAt: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, optimistic]);
      setPending(true);
      try {
        const out = await askAssistant({
          chatId: chatIdRef.current,
          callId: chatIdRef.current ? null : askCallId,
          question: q,
        });
        // Смена звонка пока ждали → ответ устарел, вид не трогаем (ревью H4).
        if (currentCallRef.current !== askCallId) return;
        chatIdRef.current = out.chatId;
        const msgs = await getAssistantChat(out.chatId);
        if (currentCallRef.current !== askCallId) return;
        setMessages(msgs);
      } catch (e) {
        if (currentCallRef.current !== askCallId) return;
        setError(humanError(e, t));
        if (chatIdRef.current) {
          try {
            setMessages(await getAssistantChat(chatIdRef.current));
          } catch {
            setMessages((prev) => prev.filter((m) => m.id !== optimistic.id));
          }
        } else {
          setMessages((prev) => prev.filter((m) => m.id !== optimistic.id));
        }
      } finally {
        pendingRef.current = false;
        setPending(false);
      }
    },
    [callId],
  );

  return { messages, pending, error, ask };
}
