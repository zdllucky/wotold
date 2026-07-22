// [B24.2] Состояние раздела «Ассистент»: список чатов + активный тред + ask.
//
// Keep-alive (решение design-gate B24.0): module-level кэш переживает
// переключение видов; при реактивации — фоновый рефетч. Optimistic: вопрос
// рендерится сразу, pending-пузырь до resolve. Гонки (ревью B24):
// - двойной ask гасится синхронным pendingRef (не state);
// - переключение чата во время pending НЕ дёргает вид обратно (guard по
//   activeRef на момент старта);
// - out-of-order openChat: последний клик выигрывает (activeRef ставится
//   синхронно до await);
// - listener assistant:status снимается даже при fast-unmount (cancelled).
//
// ВАЖНО: хук рассчитан на ОДИН смонтированный инстанс (AssistantPage) —
// кэш общий, синхронизации между инстансами нет.

import { useCallback, useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AssistantChatMeta, AssistantMessage } from '@wotold/contracts';

import {
  askAssistant,
  ASSISTANT_STATUS_EVENT,
  deleteAssistantChat,
  getAssistantChat,
  listAssistantChats,
  type AssistantStatusEvent,
} from '../api/assistant';
import { humanError } from '../api/errors';

/** Локальный id optimistic-сообщений (не из БД). */
let optimisticSeq = 0;

interface CacheShape {
  chats: AssistantChatMeta[];
  activeChatId: string | null;
  messages: AssistantMessage[];
}

// Module-level keep-alive между mount'ами раздела.
const cache: CacheShape = { chats: [], activeChatId: null, messages: [] };

// [B24.4] Мост эскалации/⌘K: App кладёт вопрос сюда и переключает вид на
// «Ассистент»; смонтированный хук консьюмит и стреляет новым глобальным чатом.
let queuedGlobalQuestion: string | null = null;
let notifyQueued: (() => void) | null = null;

/** Новый глобальный чат с готовым вопросом (эскалация из звонка, ⌘K-fallback). */
export function requestGlobalQuestion(question: string): void {
  queuedGlobalQuestion = question;
  notifyQueued?.();
}

/** ТОЛЬКО для тестов: сброс module-кэша между кейсами. */
export function resetAssistantChatsCacheForTests(): void {
  cache.chats = [];
  cache.activeChatId = null;
  cache.messages = [];
  queuedGlobalQuestion = null;
}

export interface UseAssistantChats {
  chats: AssistantChatMeta[];
  activeChatId: string | null;
  messages: AssistantMessage[];
  /** Идёт запрос: показывать pending-пузырь. */
  pending: boolean;
  /** Фаза из assistant:status для активного чата. */
  phase: AssistantStatusEvent['phase'] | null;
  /** Последняя ошибка (уже humanError). null после успешной операции. */
  error: string | null;
  ask: (question: string) => Promise<void>;
  openChat: (chatId: string) => Promise<void>;
  /** «Новый чат» — сброс активного (чат создастся первым вопросом). */
  startNewChat: () => void;
  deleteChat: (chatId: string) => Promise<void>;
  /** Эскалация/⌘K: новый глобальный чат с готовым вопросом. */
  askInNewChat: (question: string) => Promise<void>;
}

export function useAssistantChats(): UseAssistantChats {
  const [chats, setChats] = useState<AssistantChatMeta[]>(cache.chats);
  const [activeChatId, setActiveChatId] = useState<string | null>(cache.activeChatId);
  const [messages, setMessages] = useState<AssistantMessage[]>(cache.messages);
  const [pending, setPending] = useState(false);
  const [phase, setPhase] = useState<AssistantStatusEvent['phase'] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const activeRef = useRef<string | null>(cache.activeChatId);
  // Синхронный guard двойного ask (state отстаёт на тик — ревью H5).
  const pendingRef = useRef(false);

  const syncCache = useCallback((next: Partial<CacheShape>) => {
    Object.assign(cache, next);
  }, []);

  const refreshChats = useCallback(async () => {
    const list = await listAssistantChats();
    setChats(list);
    syncCache({ chats: list });
  }, [syncCache]);

  // Реактивация раздела: фоновый рефетч поверх кэша.
  useEffect(() => {
    refreshChats().catch((e) => console.warn('assistant chats refresh:', e));
    const id = cache.activeChatId;
    if (id) {
      getAssistantChat(id)
        .then((msgs) => {
          if (activeRef.current === id) {
            setMessages(msgs);
            syncCache({ messages: msgs });
          }
        })
        .catch((e) => console.warn('assistant chat refresh:', e));
    }
  }, [refreshChats, syncCache]);

  // assistant:status активного чата → фаза pending-пузыря.
  // cancelled-флаг: cleanup может случиться до resolve listen() (ревью H1).
  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    listen<AssistantStatusEvent>(ASSISTANT_STATUS_EVENT, (e) => {
      if (e.payload.chat_id !== activeRef.current) return;
      setPhase(e.payload.phase);
    })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch((err) => console.warn('assistant:status listener:', err));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const setActive = useCallback(
    (chatId: string | null, msgs: AssistantMessage[]) => {
      activeRef.current = chatId;
      setActiveChatId(chatId);
      setMessages(msgs);
      syncCache({ activeChatId: chatId, messages: msgs });
    },
    [syncCache],
  );

  const openChat = useCallback(
    async (chatId: string) => {
      // Последний клик выигрывает: ref ставится синхронно (ревью H3).
      activeRef.current = chatId;
      setActiveChatId(chatId);
      try {
        const msgs = await getAssistantChat(chatId);
        if (activeRef.current !== chatId) return; // пришёл устаревший ответ
        setMessages(msgs);
        syncCache({ activeChatId: chatId, messages: msgs });
      } catch (e) {
        setError(humanError(e));
      }
    },
    [syncCache],
  );

  const startNewChat = useCallback(() => {
    setActive(null, []);
  }, [setActive]);

  const deleteChat = useCallback(
    async (chatId: string) => {
      try {
        await deleteAssistantChat(chatId);
        if (activeRef.current === chatId) {
          setActive(null, []);
        }
        await refreshChats();
      } catch (e) {
        setError(humanError(e));
      }
    },
    [refreshChats, setActive],
  );

  const runAsk = useCallback(
    async (question: string, chatId: string | null) => {
      const q = question.trim();
      if (!q || pendingRef.current) return;
      pendingRef.current = true;
      setError(null);
      const optimistic: AssistantMessage = {
        id: `optimistic-${++optimisticSeq}`,
        role: 'user',
        text: q,
        answer: null,
        createdAt: new Date().toISOString(),
      };
      const base = chatId === activeRef.current ? messages : [];
      setMessages([...base, optimistic]);
      setPending(true);
      setPhase(null);
      try {
        const out = await askAssistant({ chatId, callId: null, question: q });
        // Guard (ревью H2): пока ждали — юзер мог уйти в другой чат.
        // Вид не дёргаем обратно; ответ уже в БД, список чатов обновляем.
        const stillHere = activeRef.current === chatId;
        if (stillHere) {
          activeRef.current = out.chatId;
          setActiveChatId(out.chatId);
          const msgs = await getAssistantChat(out.chatId);
          if (activeRef.current === out.chatId) {
            setMessages(msgs);
            syncCache({ activeChatId: out.chatId, messages: msgs });
          }
        }
        await refreshChats();
      } catch (e) {
        setError(humanError(e));
        // Вопрос уже мог попасть в БД (persist до LLM) — перечитываем тред.
        if (activeRef.current === chatId && chatId) {
          try {
            const msgs = await getAssistantChat(chatId);
            setMessages(msgs);
            syncCache({ messages: msgs });
          } catch {
            setMessages(base);
          }
        } else if (activeRef.current === chatId) {
          setMessages(base);
          await refreshChats().catch(() => {});
        }
      } finally {
        pendingRef.current = false;
        setPending(false);
        setPhase(null);
      }
    },
    [messages, refreshChats, syncCache],
  );

  const ask = useCallback((question: string) => runAsk(question, activeRef.current), [runAsk]);

  const askInNewChat = useCallback(
    async (question: string) => {
      setActive(null, []);
      await runAsk(question, null);
    },
    [runAsk, setActive],
  );

  // Консьюм очереди эскалации/⌘K: на mount и на каждый requestGlobalQuestion.
  // Регистрация — один раз ([]-deps), актуальный askInNewChat через ref
  // (иначе notify пере-подписывался бы на каждый чих messages — ревью).
  const askInNewChatRef = useRef(askInNewChat);
  useEffect(() => {
    askInNewChatRef.current = askInNewChat;
  }, [askInNewChat]);
  useEffect(() => {
    const consume = () => {
      const q = queuedGlobalQuestion;
      if (!q) return;
      queuedGlobalQuestion = null;
      void askInNewChatRef.current(q);
    };
    notifyQueued = consume;
    consume();
    return () => {
      if (notifyQueued === consume) notifyQueued = null;
    };
  }, []);

  return {
    chats,
    activeChatId,
    messages,
    pending,
    phase,
    error,
    ask,
    openChat,
    startNewChat,
    deleteChat,
    askInNewChat,
  };
}
