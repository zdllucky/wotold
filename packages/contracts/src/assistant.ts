// [M15.1] Контракт ассистента (RAG-чат по звонкам).
//
// Mirror Rust types из `apps/desktop/src-tauri/src/assistant/types.rs`.
// Источник истины по семантике — docs/M15_ASSISTANT_PRD.md §7.
// Wire-формат camelCase (Rust: `#[serde(rename_all = "camelCase")]`).

/** Вид ответа ассистента. refusal = генеративный запрос (без retrieval), empty = ничего не найдено. */
export type AssistantAnswerKind = 'answer' | 'refusal' | 'empty';

/** Тип пассажа индекса (источник фрагмента).
 * [M16.6] call_meta — синтетическая «карточка звонка» (титул+дата+участники). */
export type AssistantPassageKind =
  | 'transcript'
  | 'recap'
  | 'decision'
  | 'action_item'
  | 'open_question'
  | 'call_meta'
  | 'contact';

/**
 * Источник ответа: звонок + опциональный таймкод.
 * `startMs === null` — источник без таймкода (например, recap-абзац).
 */
export interface AssistantSource {
  callId: string;
  /** Денормализован на момент ответа; при рендере истории резолвить заново по списку звонков. */
  callTitle: string;
  startMs: number | null;
}

/** Фрагмент, реально попавший в контекст LLM (блок «Контекст поиска»). */
export interface AssistantFragment {
  callId: string;
  callTitle: string;
  kind: AssistantPassageKind;
  speaker: string | null;
  startMs: number | null;
  text: string;
  /** [B26.4] Текст усечён на отдаче; полный текст —
   * `assistant_get_fragment_text(messageId, fragmentIndex)`. */
  textTruncated?: boolean;
}

/** Полный ответ ассистента (persist в assistant_messages.answer_json). */
export interface AssistantAnswer {
  kind: AssistantAnswerKind;
  text: string;
  /** Пусто для refusal/empty. */
  sources: AssistantSource[];
  /** Что было в контексте; пусто для refusal (retrieval не выполнялся). */
  fragments: AssistantFragment[];
  /** Оценка токенов фрагментов (для mono-строки «фрагментов: N · ≈X.XK токенов»). */
  fragmentTokens: number;
  /** Фикс окна локальной модели. */
  windowTokens: 8192;
  /** Только для empty в call-scope: показать чип «Искать во всех звонках». */
  escalate?: boolean;
}

/** Метаданные чата. `callId === null` — глобальный чат раздела «Ассистент». */
export interface AssistantChatMeta {
  id: string;
  callId: string | null;
  /** Первый вопрос, усечённый до ~42 симв. */
  title: string;
  createdAt: string;
  /** [B26.9] Последняя активность — сортировка микса «Недавних». */
  updatedAt: string;
}

/** Сообщение чата. Для role='assistant' поле answer заполнено. */
export interface AssistantMessage {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  answer: AssistantAnswer | null;
  createdAt: string;
}

/** Статистика индекса для чипа «в поиске X из Y звонков · ЧЧ ч ММ мин». */
export interface AssistantIndexStats {
  indexedCalls: number;
  totalCalls: number;
  totalDurationSec: number;
}

/** Аргументы assistant_ask (зеркало Rust `assistant::AskArgs`). */
export interface AssistantAskArgs {
  /** Продолжить существующий чат (глобальный или тред звонка). */
  chatId: string | null;
  /** Тред звонка (создаётся при первом вопросе). Игнорируется если chatId задан. */
  callId: string | null;
  question: string;
}

/** Результат assistant_ask (зеркало Rust `assistant::AskOutcome`). */
export interface AssistantAskOutcome {
  chatId: string;
  message: AssistantMessage;
}

/** Тред звонка (зеркало Rust `commands::assistant::AssistantCallThread`). */
export interface AssistantCallThread {
  chat: AssistantChatMeta;
  messages: AssistantMessage[];
}

/** Имя tauri-события фаз ответа (Rust `events::ASSISTANT_STATUS`). */
export const ASSISTANT_STATUS_EVENT = 'assistant:status';

/**
 * Payload события `assistant:status`.
 * ВНИМАНИЕ: сознательное исключение из camelCase-правила файла — Rust-структура
 * `events::AssistantStatusEvent` идёт без `rename_all` (конвенция events.rs:
 * снейк, как `call_id` в остальных событиях). Не «чинить» односторонне.
 */
export interface AssistantStatusEvent {
  chat_id: string;
  phase: 'retrieving' | 'generating';
}
