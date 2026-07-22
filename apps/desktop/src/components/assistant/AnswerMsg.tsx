// [B24.3] Сообщение-ответ ассистента — «точь в точь» wk2-assistant.jsx:81-130.
// Дизайн-канон: docs/design/wotold-v2/assistant.md. Три kind'а:
// answer (текст + источники + контекст + действия), refusal (нота shield,
// без «Контекста поиска»), empty (текст + опц. эскалация).

import { useCallback, useState } from 'react';
import type { AssistantAnswer, AssistantFragment, AssistantSource } from '@wotold/contracts';

import { useI18n } from '../../i18n';
import { Chip, Dropdown, Icon, IconBtn, MenuItem } from '../../ui';

const COPIED_RESET_MS = 1400;

/** `[m:ss]` — как в transcript.md/answer.rs (минуты без часов). */
export function fmtSourceClock(ms: number): string {
  const totalSec = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(totalSec / 60)}:${String(totalSec % 60).padStart(2, '0')}`;
}

/** Цвет спикера (design-gate B24.0): owner → --sp1, прочие — stable-hash → --sp2..5. */
export function speakerColor(callId: string, tag: string | null): string {
  if (!tag || tag === 'owner') return 'var(--sp1)';
  let h = 0;
  const key = `${callId}:${tag}`;
  for (let i = 0; i < key.length; i += 1) {
    h = (h * 31 + key.charCodeAt(i)) >>> 0;
  }
  return `var(--sp${(h % 4) + 2})`;
}

function sourceLabel(s: AssistantSource, ownCallId: string | null): string {
  const clock = s.startMs != null ? fmtSourceClock(s.startMs) : null;
  if (ownCallId && s.callId === ownCallId) return clock ?? s.callTitle;
  return clock ? `${s.callTitle} · ${clock}` : s.callTitle;
}

function copyWithSourcesText(answer: AssistantAnswer): string {
  const src = answer.sources
    .map((s) => (s.startMs != null ? `${s.callTitle} · ${fmtSourceClock(s.startMs)}` : s.callTitle))
    .join('; ');
  return src ? `${answer.text}\n\nИсточники: ${src}` : answer.text;
}

export interface AnswerMsgProps {
  answer: AssistantAnswer;
  /** Вопрос, на который это ответ (для эскалации «Искать во всех звонках»). */
  question: string;
  /** Звонок текущего экрана (вкладка звонка) — его источники становятся seek-чипами. */
  callId?: string | null;
  onOpenCall?: (callId: string) => void;
  onSeek?: (ms: number) => void;
  onAskGlobal?: (question: string) => void;
}

export function AnswerMsg({
  answer,
  question,
  callId = null,
  onOpenCall,
  onSeek,
  onAskGlobal,
}: AnswerMsgProps) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);

  const doCopy = useCallback((text: string) => {
    // «Скопировано» — только после успешной записи (ревью H6); отказ
    // clipboard глотается без false-success (SPEC §8: консоль чистая).
    try {
      navigator.clipboard
        .writeText(text)
        .then(() => {
          setCopied(true);
          window.setTimeout(() => setCopied(false), COPIED_RESET_MS);
        })
        .catch(() => {});
    } catch {
      // clipboard API недоступен вовсе
    }
  }, []);

  const sendEmail = useCallback(() => {
    const body = copyWithSourcesText(answer);
    const href = `mailto:?body=${encodeURIComponent(body)}`;
    const a = document.createElement('a');
    a.href = href;
    a.rel = 'noopener';
    try {
      a.click();
    } catch {
      // WKWebView без opener — фоллбек: копия с источниками (design-gate B24.0)
      doCopy(body);
    }
  }, [answer, doCopy]);

  const sourceChip = (s: AssistantSource, i: number) => {
    const own = callId != null && s.callId === callId;
    const seek = own && onSeek && s.startMs != null ? () => onSeek(s.startMs as number) : undefined;
    const open = !own && onOpenCall ? () => onOpenCall(s.callId) : undefined;
    return (
      <Chip key={`${s.callId}-${s.startMs ?? 'x'}-${i}`} size="sm" tone="line" icon={own ? 'clock' : 'doc'} onClick={seek ?? open}>
        {sourceLabel(s, callId)}
      </Chip>
    );
  };

  const frag = (f: AssistantFragment, i: number) => (
    <div className="frag" key={`${f.callId}-${f.startMs ?? 'x'}-${i}`}>
      <b style={{ color: speakerColor(f.callId, f.speaker) }}>{f.speaker ?? f.callTitle}</b>
      <span className="u-muted">
        {' '}
        · {f.callTitle}
        {f.startMs != null ? ` · ${fmtSourceClock(f.startMs)}` : ''}
      </span>
      <br />
      {f.text}
    </div>
  );

  return (
    <div className="ask-bubble">
      {answer.kind === 'refusal' && (
        <div className="ask-note">
          <Icon name="shield" size={13} />
          {t('assistant.refusalNote')}
        </div>
      )}
      <div style={{ whiteSpace: 'pre-line' }}>{answer.text}</div>
      {answer.sources.length > 0 && <div className="src-row">{answer.sources.map(sourceChip)}</div>}
      {answer.escalate && onAskGlobal && (
        <div className="src-row">
          <Chip size="sm" tone="accent" icon="search" onClick={() => onAskGlobal(question)}>
            {t('assistant.escalate')}
          </Chip>
        </div>
      )}
      {answer.kind !== 'refusal' && answer.fragments.length > 0 && (
        <details className="ctx">
          <summary>
            <Icon name="chevronRight" size={11} className="ctx-arr" />
            {t('assistant.ctxSummary')}
          </summary>
          {answer.fragments.map(frag)}
          <div className="ctx-meta mono">
            {t('assistant.ctxMeta', {
              n: answer.fragments.length,
              tokens: (answer.fragmentTokens / 1000).toFixed(1),
            })}
          </div>
        </details>
      )}
      {answer.kind === 'answer' && (
        <div className="ans-acts">
          <IconBtn
            icon={copied ? 'check' : 'copy'}
            size="sm"
            label={t('assistant.copy')}
            tip={copied ? t('assistant.copied') : t('assistant.copy')}
            onClick={() => doCopy(answer.text)}
          />
          <Dropdown
            align="left"
            width={238}
            trigger={({ toggle, open }) => (
              <IconBtn
                icon="send"
                size="sm"
                label={t('assistant.share')}
                tip={t('assistant.share')}
                onClick={toggle}
                hasPopup
                expanded={open}
              />
            )}
          >
            <MenuItem icon="copy" onClick={() => doCopy(copyWithSourcesText(answer))}>
              {t('assistant.shareWithSources')}
            </MenuItem>
            <MenuItem icon="external" onClick={sendEmail}>
              {t('assistant.shareEmail')}
            </MenuItem>
          </Dropdown>
        </div>
      )}
    </div>
  );
}
