// [B24.3] Сообщение-ответ ассистента — «точь в точь» wk2-assistant.jsx:81-130.
// Дизайн-канон: docs/design/wotold-v2/assistant.md. Три kind'а:
// answer (текст + источники + контекст + действия), refusal (нота shield,
// без «Контекста поиска»), empty (текст + опц. эскалация).

import { useCallback, useEffect, useRef, useState, type MouseEvent } from 'react';
import type { AssistantAnswer, AssistantFragment, AssistantSource } from '@wotold/contracts';

import { shareText } from '../../api/assistant';
import { useI18n } from '../../i18n';
import { FragmentRow } from './FragmentRow';
import { parseFragmentRefs } from './fragmentRefs';
import { MsgTime } from './MsgTime';
import { Chip, Icon, IconBtn } from '../../ui';

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
  /** [B26.6] id сообщения — ключ ленивой подгрузки полного текста фрагмента. */
  messageId: string;
  /** [B26.8] Время сообщения для MsgTime. */
  createdAt: string;
  /** Звонок текущего экрана (вкладка звонка) — его источники становятся seek-чипами. */
  callId?: string | null;
  onOpenCall?: (callId: string) => void;
  onSeek?: (ms: number) => void;
  onAskGlobal?: (question: string) => void;
  /** [B26.5] Клик по контакт-источнику (sentinel contact:*) → раздел «Контакты». */
  onOpenContacts?: () => void;
}

export function AnswerMsg({
  answer,
  question,
  messageId,
  createdAt,
  callId = null,
  onOpenCall,
  onSeek,
  onAskGlobal,
  onOpenContacts,
}: AnswerMsgProps) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  // [B27.8] «Контекст поиска» — controlled: клик по [N]-ссылке раскрывает
  // блок и подсвечивает фрагмент.
  const [ctxOpen, setCtxOpen] = useState(false);
  const [flashIdx, setFlashIdx] = useState<number | null>(null);
  const fragEls = useRef<(HTMLDivElement | null)[]>([]);
  const flashTimer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (flashTimer.current != null) window.clearTimeout(flashTimer.current);
    },
    [],
  );

  const revealFragment = useCallback((i: number) => {
    setCtxOpen(true);
    setFlashIdx(i);
    if (flashTimer.current != null) window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlashIdx(null), 1600);
    const smooth = !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    requestAnimationFrame(() => {
      fragEls.current[i]?.scrollIntoView({ block: 'nearest', behavior: smooth ? 'smooth' : 'auto' });
    });
  }, []);

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

  // [B27.6] Нативный share-пикер macOS у кнопки; ошибка/не-macOS —
  // тихий фоллбек копией с источниками.
  const shareNative = useCallback(
    (e: MouseEvent<HTMLButtonElement>) => {
      const body = copyWithSourcesText(answer);
      const r = e.currentTarget.getBoundingClientRect();
      shareText(body, { x: r.x, y: r.y, w: r.width, h: r.height }).catch(() => doCopy(body));
    },
    [answer, doCopy],
  );

  const sourceChip = (s: AssistantSource, i: number) => {
    // [B26.5] Контакт-источник (sentinel) — ведёт в раздел «Контакты».
    const isContact = s.callId.startsWith('contact:');
    const own = !isContact && callId != null && s.callId === callId;
    const seek = own && onSeek && s.startMs != null ? () => onSeek(s.startMs as number) : undefined;
    const open = !own && !isContact && onOpenCall ? () => onOpenCall(s.callId) : undefined;
    return (
      <Chip
        key={`${s.callId}-${s.startMs ?? 'x'}-${i}`}
        size="sm"
        tone="line"
        icon={isContact ? 'user' : own ? 'clock' : 'doc'}
        onClick={isContact ? onOpenContacts : (seek ?? open)}
      >
        {sourceLabel(s, callId)}
      </Chip>
    );
  };

  // [B26.6] Управляемая строка: усечённый текст + lazy-раскрытие.
  // [B27.8] Обёртка .frag-wrap — якорь скролла и flash-подсветки [N]-ссылок.
  const frag = (f: AssistantFragment, i: number) => (
    <div
      key={`${f.callId}-${f.startMs ?? 'x'}-${i}`}
      className={'frag-wrap' + (flashIdx === i ? ' frag--flash' : '')}
      ref={(el) => {
        fragEls.current[i] = el;
      }}
    >
      <FragmentRow fragment={f} index={i} messageId={messageId} />
    </div>
  );

  // [B27.8] Текст ответа: «[2]» / «[2, 4]» → кликабельные ссылки на фрагменты.
  const fragmentCount = answer.kind === 'refusal' ? 0 : answer.fragments.length;
  const textSegments = parseFragmentRefs(answer.text, fragmentCount);

  return (
    <div className="ask-bubble" data-selectable>
      {answer.kind === 'refusal' && (
        <div className="ask-note">
          <Icon name="shield" size={13} />
          {t('assistant.refusalNote')}
        </div>
      )}
      <div style={{ whiteSpace: 'pre-line' }}>
        {textSegments.map((seg, i) =>
          seg.kind === 'text' ? (
            seg.text
          ) : (
            <button
              key={`ref-${i}`}
              type="button"
              className="frag-ref mono"
              aria-label={t('assistant.fragRefLabel', { n: seg.indices[0] })}
              onClick={() => revealFragment(seg.indices[0] - 1)}
            >
              {seg.raw}
            </button>
          ),
        )}
      </div>
      {answer.sources.length > 0 && <div className="src-row">{answer.sources.map(sourceChip)}</div>}
      {answer.escalate && onAskGlobal && (
        <div className="src-row">
          <Chip size="sm" tone="accent" icon="search" onClick={() => onAskGlobal(question)}>
            {t('assistant.escalate')}
          </Chip>
        </div>
      )}
      {answer.kind !== 'refusal' && answer.fragments.length > 0 && (
        <details className="ctx" open={ctxOpen}>
          {/* [B27.8] preventDefault — иначе двойной toggle (native + state). */}
          <summary
            onClick={(e) => {
              e.preventDefault();
              setCtxOpen((v) => !v);
            }}
          >
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
      {/* [B27.1] Один ряд: действия слева (hover-reveal), время справа. */}
      <div className="ans-foot">
        {answer.kind === 'answer' && (
        <div className="ans-acts">
          <IconBtn
            icon={copied ? 'check' : 'copy'}
            size="sm"
            label={t('assistant.copy')}
            tip={copied ? t('assistant.copied') : t('assistant.copy')}
            onClick={() => doCopy(answer.text)}
          />
          <IconBtn
            icon="send"
            size="sm"
            label={t('assistant.share')}
            tip={t('assistant.share')}
            onClick={shareNative}
          />
        </div>
        )}
        <MsgTime createdAt={createdAt} />
      </div>
    </div>
  );
}
