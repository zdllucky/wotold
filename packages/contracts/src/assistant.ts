// [M15.1] Контракт ассистента (RAG-чат по звонкам).
//
// Mirror Rust types из `apps/desktop/src-tauri/src/assistant/types.rs`.
// Источник истины по семантике — docs/M15_ASSISTANT_PRD.md §7.
// Wire-формат camelCase (Rust: `#[serde(rename_all = "camelCase")]`).

/** Вид ответа ассистента. refusal = генеративный запрос (без retrieval), empty = ничего не найдено. */
export type AssistantAnswerKind = 'answer' | 'refusal' | 'empty';

/** Тип пассажа индекса (источник фрагмента). */
export type AssistantPassageKind =
  | 'transcript'
  | 'recap'
  | 'decision'
  | 'action_item'
  | 'open_question';

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
