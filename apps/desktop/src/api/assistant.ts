// [B24.2] Тонкие invoke-врапперы команд ассистента (Rust: commands/assistant.rs).
// Типы — из контракта S2 (@wotold/contracts), не дублируем.

import { invoke } from '@tauri-apps/api/core';
import type {
  AssistantAskArgs,
  AssistantAskOutcome,
  AssistantCallThread,
  AssistantChatMeta,
  AssistantIndexStats,
  AssistantMessage,
} from '@wotold/contracts';

export { ASSISTANT_STATUS_EVENT } from '@wotold/contracts';
export type { AssistantStatusEvent } from '@wotold/contracts';

/** Чип «в поиске X из Y звонков · ЧЧ ч ММ мин». */
export function getAssistantIndexStats(): Promise<AssistantIndexStats> {
  return invoke<AssistantIndexStats>('assistant_index_stats');
}

/** Глобальные чаты раздела, свежие сверху. */
export function listAssistantChats(): Promise<AssistantChatMeta[]> {
  return invoke<AssistantChatMeta[]>('assistant_list_chats');
}

/** Сообщения чата по порядку. */
export function getAssistantChat(chatId: string): Promise<AssistantMessage[]> {
  return invoke<AssistantMessage[]>('assistant_get_chat', { chatId });
}

/** Тред звонка (chat + messages), null если ещё не создан. */
export function getAssistantCallThread(callId: string): Promise<AssistantCallThread | null> {
  return invoke<AssistantCallThread | null>('assistant_get_call_thread', { callId });
}

/** Удалить чат (messages каскадом). Идемпотентно. */
export function deleteAssistantChat(chatId: string): Promise<void> {
  return invoke<void>('assistant_delete_chat', { chatId });
}

/** Вопрос ассистенту: классификатор → retrieval → LLM → persist. */
export function askAssistant(args: AssistantAskArgs): Promise<AssistantAskOutcome> {
  return invoke<AssistantAskOutcome>('assistant_ask', { args });
}

/** [B25] Тумблер «Семантический поиск» (default on). */
export function getAssistantSemanticSearch(): Promise<boolean> {
  return invoke<boolean>('assistant_get_semantic_search');
}

/** [B25] Переключить семантический поиск. Включение фоново докачивает
 * модель эмбеддера (прогресс — model:progress) и запускает backfill. */
export function setAssistantSemanticSearch(enabled: boolean): Promise<void> {
  return invoke<void>('assistant_set_semantic_search', { enabled });
}
