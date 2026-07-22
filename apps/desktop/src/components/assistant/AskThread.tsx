// [B24.4] Тред сообщений ассистента — общий для раздела и вкладки звонка.
// user → пузырь справа; assistant → AnswerMsg; pending → Wave-пузырь.

import type { AssistantMessage } from '@wotold/contracts';

import { Wave } from '../../ui';
import { AnswerMsg } from './AnswerMsg';

export interface AskThreadProps {
  messages: AssistantMessage[];
  pending: boolean;
  pendingText: string;
  /** Звонок текущего экрана (вкладка звонка) — источники его становятся seek-чипами. */
  callId?: string | null;
  onOpenCall?: (callId: string) => void;
  onSeek?: (ms: number) => void;
  onAskGlobal?: (question: string) => void;
}

/** Вопрос, на который отвечает assistant-сообщение = предыдущий user-текст. */
function questionFor(messages: AssistantMessage[], index: number): string {
  for (let i = index - 1; i >= 0; i -= 1) {
    const m = messages[i];
    if (m && m.role === 'user') return m.text;
  }
  return '';
}

export function AskThread({
  messages,
  pending,
  pendingText,
  callId = null,
  onOpenCall,
  onSeek,
  onAskGlobal,
}: AskThreadProps) {
  return (
    // [B24.7 a11y H1] role="log": новые сообщения анонсируются AT (implicit
    // aria-live=polite), чат-паттерн ARIA.
    <div className="ask-thread" role="log">
      {messages.map((m, i) =>
        m.role === 'user' ? (
          <div className="ask-row fade-up" data-me="true" key={m.id}>
            <div className="ask-bubble">{m.text}</div>
          </div>
        ) : (
          <div className="ask-row fade-up" data-me="false" key={m.id}>
            {m.answer ? (
              <AnswerMsg
                answer={m.answer}
                question={questionFor(messages, i)}
                callId={callId}
                onOpenCall={onOpenCall}
                onSeek={onSeek}
                onAskGlobal={onAskGlobal}
              />
            ) : (
              <div className="ask-bubble" style={{ whiteSpace: 'pre-line' }}>
                {m.text}
              </div>
            )}
          </div>
        ),
      )}
      {pending && (
        <div className="ask-row" data-me="false">
          {/* [B24.7 a11y H1] role="status" — «Поиск…» объявляется AT. */}
          <div className="ask-bubble ask-pend" role="status">
            <span aria-hidden="true">
              <Wave bars={4} color="var(--text-3)" />
            </span>
            {pendingText}
          </div>
        </div>
      )}
    </div>
  );
}
