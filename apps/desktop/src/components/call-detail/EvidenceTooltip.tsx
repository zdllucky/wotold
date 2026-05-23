// [M14 T-11] EvidenceTooltip — hover/click popover показывающий evidence
// quote из расшифровки для action item / decision / open question.
//
// Hover/focus → popover shown (auto-dismiss on blur/mouseleave).
// Click → popover sticky (stays until next click outside).
// Если есть `startMs` + `onJumpToTranscript` callback — button «К моменту»
// в popover'е переключает на таб Расшифровки + scroll to that timestamp.

import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { useI18n } from '../../i18n';

export interface EvidenceTooltipProps {
  quote: string;
  speaker?: string | null;
  startMs?: number | null;
  /** Если передан — popover показывает button «Перейти к моменту». */
  onJumpToTranscript?: (ms: number) => void;
  /** Trigger element (обычно small 💬 button). */
  children: React.ReactNode;
}

function formatMs(ms: number): string {
  const sec = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}

export function EvidenceTooltip({
  quote,
  speaker,
  startMs,
  onJumpToTranscript,
  children,
}: EvidenceTooltipProps) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [sticky, setSticky] = useState(false);
  const containerRef = useRef<HTMLSpanElement>(null);
  const popoverId = useId();

  // Click outside → close sticky.
  useEffect(() => {
    if (!sticky) return;
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setSticky(false);
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [sticky]);

  const handleTriggerClick = useCallback(() => {
    if (sticky) {
      setSticky(false);
      setOpen(false);
    } else {
      setSticky(true);
      setOpen(true);
    }
  }, [sticky]);

  const handleJump = useCallback(() => {
    if (typeof startMs === 'number' && onJumpToTranscript) {
      onJumpToTranscript(startMs);
      setSticky(false);
      setOpen(false);
    }
  }, [startMs, onJumpToTranscript]);

  return (
    <span
      ref={containerRef}
      className="evidence-tooltip"
      onMouseEnter={() => !sticky && setOpen(true)}
      onMouseLeave={() => !sticky && setOpen(false)}
      onFocus={() => !sticky && setOpen(true)}
      onBlur={(e) => {
        // Если focus уходит за пределы — close (только если не sticky).
        if (!sticky && !e.currentTarget.contains(e.relatedTarget as Node | null)) {
          setOpen(false);
        }
      }}
    >
      <button
        type="button"
        className="evidence-trigger"
        aria-describedby={open ? popoverId : undefined}
        aria-expanded={open}
        aria-label={t('evidence.fromTranscript')}
        onClick={handleTriggerClick}
      >
        {children}
      </button>
      {open && (
        <div
          id={popoverId}
          role="tooltip"
          className={`evidence-popover ${sticky ? 'evidence-popover--sticky' : ''}`}
        >
          <blockquote className="evidence-quote">{quote}</blockquote>
          <div className="evidence-meta">
            {speaker && (
              <span className="evidence-speaker">
                <span className="mono muted" style={{ fontSize: 11 }}>
                  {t('evidence.speakerLabel')}:
                </span>{' '}
                {speaker}
              </span>
            )}
            {typeof startMs === 'number' && (
              <span className="evidence-timestamp mono muted" style={{ fontSize: 11 }}>
                {formatMs(startMs)}
              </span>
            )}
          </div>
          {typeof startMs === 'number' && onJumpToTranscript && (
            <button
              type="button"
              className="btn btn--quiet evidence-jump-btn"
              onClick={handleJump}
            >
              {t('evidence.jumpToMoment')} →
            </button>
          )}
        </div>
      )}
    </span>
  );
}
