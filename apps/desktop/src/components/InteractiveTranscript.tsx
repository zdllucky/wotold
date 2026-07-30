// [B10] M7.3 follow-up: интерактивный транскрипт chat-bubble layout.
//
// Парсит raw_stt.json (writeн в pipeline::run::persist_artifacts), берёт
// .merged массив сегментов. Группирует подряд идущих одного спикера в
// блок-баббл. Owner справа, остальные слева. Цвет бейджа спикера
// стабилен на speaker_tag (hash → palette).
//
// Fallback: если raw_stt.json отсутствует (старые звонки до B10) —
// рендер ReactMarkdown оригинального transcript.md.

import {
  forwardRef,
  memo,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
} from 'react';
import ReactMarkdown from 'react-markdown';
import type { CallSpeakerView } from '../api/speakers';
import { Avatar, Empty, Icon } from '../ui';
import { useI18n } from '../i18n';
import { humanSpeakerLabel } from '../utils/callMeta';
import { findActiveGroupIdx, type GroupRange } from '../lib/transcriptActive';

/** [TD-27] Одна реплика. Вынесена и мемоизирована: `currentTime` тикает ~4
 *  раза в секунду, и до этого КАЖДЫЙ тик перерисовывал все строки транскрипта
 *  (часовой звонок — сотни `.turn`), потому что разметка жила прямо в теле
 *  родителя. Теперь на тик перерисовываются ровно две строки: та, что
 *  перестала быть активной, и та, что стала.
 *
 *  Props намеренно примитивные — объект пересоздавался бы на каждый рендер и
 *  сравнение по ссылке всё бы обнулило. `onSeek` стабилизируется в родителе
 *  через ref по той же причине. */
/** [TD-27] Общая проверка reduced-motion для автоскролла. */
function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

interface TurnRowProps {
  tag: string;
  color: string;
  firstName: string;
  text: string;
  start: number;
  isActive: boolean;
  timecode: string;
  emptyLabel: string;
  onSeek?: (start: number) => void;
  rowRef?: React.Ref<HTMLDivElement>;
}

const TurnRow = memo(function TurnRow({
  color,
  firstName,
  text,
  start,
  isActive,
  timecode,
  emptyLabel,
  onSeek,
  rowRef,
}: TurnRowProps) {
  const isClickable = !!onSeek;
  return (
    <div
      ref={rowRef}
      className={'turn' + (isClickable ? ' turn--seek' : '') + (isActive ? ' turn--active' : '')}
      onClick={isClickable ? () => onSeek!(start) : undefined}
      role={isClickable ? 'button' : undefined}
      tabIndex={isClickable ? 0 : undefined}
      onKeyDown={
        isClickable
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onSeek!(start);
              }
            }
          : undefined
      }
    >
      <div>
        <div className="turn-sp" style={{ color }}>
          <Avatar name={firstName} color={color} size={20} />
          {firstName}
        </div>
        <div className="turn-time">
          {timecode}
          {isClickable && (
            <Icon name="play" size={9} style={{ marginLeft: 4, opacity: isActive ? 1 : 0 }} />
          )}
        </div>
      </div>
      <div className="turn-text">{text || <span className="subtle">{emptyLabel}</span>}</div>
    </div>
  );
});

interface Segment {
  start: number;
  end: number;
  text: string;
  speakerTag: string;
}

interface RawStt {
  version: number;
  merged?: Segment[];
}

// [B16] Lightweight runtime validator — без zod dependency. Pipeline пишет
// raw_stt.json который мы читаем — если shape поменяется (или файл corrupt),
// component не должен runtime-crash'ить.
function isSegment(x: unknown): x is Segment {
  if (!x || typeof x !== 'object') return false;
  const o = x as Record<string, unknown>;
  return (
    typeof o.start === 'number' &&
    typeof o.end === 'number' &&
    typeof o.text === 'string' &&
    typeof o.speakerTag === 'string'
  );
}

function parseRawStt(json: string): RawStt | null {
  try {
    const raw = JSON.parse(json) as unknown;
    if (!raw || typeof raw !== 'object') return null;
    const o = raw as Record<string, unknown>;
    if (typeof o.version !== 'number') return null;
    const merged = Array.isArray(o.merged) ? o.merged.filter(isSegment) : undefined;
    return { version: o.version, merged };
  } catch {
    return null;
  }
}

const OWNER_TAG = 'owner';

// [B17] Палитра спикеров — cobalt/emerald/rust/indigo/teal из tokens.css
// (см. --sp1..--sp5). Owner всегда sp1 (cobalt), остальные — циклом по hash.
function hashTag(tag: string): number {
  let h = 0;
  for (let i = 0; i < tag.length; i++) {
    h = (h * 31 + tag.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

function colorVarFor(tag: string): string {
  if (tag === OWNER_TAG) return 'var(--sp1)';
  const idx = (hashTag(tag) % 4) + 2; // 2..5
  return `var(--sp${idx})`;
}

function formatTimecode(sec: number): string {
  const h = Math.floor(sec / 3600);
  const mm = Math.floor((sec % 3600) / 60)
    .toString()
    .padStart(2, '0');
  const ss = Math.floor(sec % 60)
    .toString()
    .padStart(2, '0');
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

interface Group {
  tag: string;
  segments: Segment[];
}

function groupBySpeaker(segments: Segment[]): Group[] {
  const groups: Group[] = [];
  for (const s of segments) {
    const last = groups[groups.length - 1];
    if (last && last.tag === s.speakerTag) {
      last.segments.push(s);
    } else {
      groups.push({ tag: s.speakerTag, segments: [s] });
    }
  }
  return groups;
}

interface Props {
  rawSttJson: string | null;
  fallbackMd: string | null;
  /** Список call_speakers — для display_name override на бейджах. Опционально. */
  speakers?: CallSpeakerView[];
  /** [B17 V3.2] Текущая позиция аудио (sec). Если в [groupStart, groupEnd]
   *  → блок подсвечивается. */
  currentTime?: number;
  /** Клик по блоку → seek в начало этого блока. */
  onSeek?: (seconds: number) => void;
  /** [P-fix10] Идёт STT и расшифровки ещё нет → ghost-строки (skeleton) вместо
   *  пустоты, чтобы не казалось зависшим. */
  generating?: boolean;
  /** [B20.8] Автоскролл к активной реплике. false = юзер скроллит сам,
   *  follow вернёт только кнопка в плеере (scrollToActive). */
  follow?: boolean;
}

/** [B20.8] Императивный хэндл — кнопка «к текущему» в плеере. */
export interface InteractiveTranscriptHandle {
  scrollToActive: () => void;
}

/** speaker_tag → label для бейджа. Confirmed-contact → display_name,
 *  иначе — fallback на tag ('owner' → 'я', прочее как есть). */
function buildLabelMap(speakers?: CallSpeakerView[]): Map<string, string> {
  const m = new Map<string, string>();
  if (!speakers) return m;
  for (const s of speakers) {
    if (s.confirmed && s.contact_display_name) {
      m.set(s.speaker_tag, s.contact_display_name);
    }
  }
  return m;
}

export const InteractiveTranscript = forwardRef<InteractiveTranscriptHandle, Props>(
  function InteractiveTranscript(
    { rawSttJson, fallbackMd, speakers, currentTime, onSeek, generating = false, follow = true },
    ref,
  ) {
  const { t } = useI18n();
  const labels = useMemo(() => buildLabelMap(speakers), [speakers]);
  const segments: Segment[] | null = useMemo(() => {
    if (!rawSttJson) return null;
    const parsed = parseRawStt(rawSttJson);
    return parsed?.merged ?? null;
  }, [rawSttJson]);

  const groups = useMemo(
    () => (segments ? groupBySpeaker(segments) : []),
    [segments],
  );

  // [B17 V3.2, B20.9] Index of group containing currentTime, or -1.
  const groupRanges: GroupRange[] = useMemo(
    () =>
      groups.map((g) => {
        const start = g.segments[0]?.start ?? 0;
        return { start, end: g.segments[g.segments.length - 1]?.end ?? start };
      }),
    [groups],
  );
  const activeIdx = useMemo(
    () => (currentTime == null ? -1 : findActiveGroupIdx(groupRanges, currentTime)),
    [currentTime, groupRanges],
  );

  // [B20.8] Auto-scroll active row: только в follow-режиме. Ручной скролл юзера
  // выключает follow (см. CallDetailPage listeners), кнопка в плеере включает
  // обратно и дёргает scrollToActive() через ref.
  // [TD-27] `onSeek` приходит из CallDetailPage инлайновой стрелкой, то есть
  // с новой ссылкой на каждый рендер. Без стабилизации memo на TurnRow не
  // сработал бы вовсе: пропс менялся бы каждый тик.
  const onSeekRef = useRef(onSeek);
  onSeekRef.current = onSeek;
  const handleSeek = useCallback((start: number) => onSeekRef.current?.(start), []);
  const seekProp = onSeek ? handleSeek : undefined;

  const activeRowRef = useRef<HTMLDivElement | null>(null);
  useImperativeHandle(
    ref,
    () => ({
      scrollToActive: () => {
        activeRowRef.current?.scrollIntoView({
          behavior: prefersReducedMotion() ? 'auto' : 'smooth',
          block: 'center',
        });
      },
    }),
    [],
  );
  useEffect(() => {
    if (!follow || activeIdx < 0) return;
    const el = activeRowRef.current;
    if (!el) return;
    // [TD-27] Плавный скролл запускается на КАЖДОЙ смене активной реплики,
    // то есть во время воспроизведения почти непрерывно. При
    // prefers-reduced-motion это ровно тот случай, ради которого настройка и
    // существует — прыгаем мгновенно. Приём тот же, что в useTypewriter.
    el.scrollIntoView({
      behavior: prefersReducedMotion() ? 'auto' : 'smooth',
      block: 'center',
    });
  }, [activeIdx, follow]);

  if (!segments || segments.length === 0) {
    if (fallbackMd) {
      return (
        <div className="markdown">
          <ReactMarkdown>{fallbackMd}</ReactMarkdown>
        </div>
      );
    }
    // [P-fix10] Идёт STT — ghost-строки (shimmer) вместо пустоты.
    if (generating) {
      return (
        <div aria-busy="true" aria-label={t('callDetail.generatingTranscript')}>
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="turn">
              <div>
                <div className="turn-sp">
                  <span
                    className="skeleton"
                    style={{ width: 20, height: 20, borderRadius: '50%' }}
                  />
                  <span
                    className="skeleton"
                    style={{ width: 56, height: 9, borderRadius: 'var(--r-xs)' }}
                  />
                </div>
              </div>
              <div className="turn-text">
                <span
                  className="skeleton"
                  style={{ display: 'block', width: '85%', height: 12, borderRadius: 'var(--r-xs)' }}
                />
              </div>
            </div>
          ))}
        </div>
      );
    }
    return <Empty description={t('callDetail.emptyTranscript')} />;
  }

  return (
    <div>
      {groups.map((g, idx) => {
        const start = g.segments[0]?.start ?? 0;
        const color = colorVarFor(g.tag);
        // [V5.3] Если есть привязанный контакт → берём first name (display
        // contact display_name могло быть "Иван Петров", показываем "Иван").
        // Иначе → humanSpeakerLabel целиком ("Голос 1", "Я" — split'ить нельзя
        // потому что "Голос 1" разорвётся на просто "Голос").
        const contactLabel = labels.get(g.tag);
        const firstName = contactLabel
          ? contactLabel.split(/\s+/)[0] ?? contactLabel
          : humanSpeakerLabel(g.tag, t);
        const text = g.segments
          .map((s) => s.text.trim())
          .filter(Boolean)
          .join(' ');
        const isActive = idx === activeIdx;
        return (
          <TurnRow
            key={`${g.tag}-${idx}`}
            tag={g.tag}
            color={color}
            firstName={firstName}
            text={text}
            start={start}
            isActive={isActive}
            timecode={formatTimecode(start)}
            emptyLabel="…"
            onSeek={seekProp}
            rowRef={isActive ? activeRowRef : undefined}
          />
        );
      })}
    </div>
  );
  },
);
