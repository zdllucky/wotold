// [B10] M7.3 follow-up: интерактивный транскрипт chat-bubble layout.
//
// Парсит raw_stt.json (writeн в pipeline::run::persist_artifacts), берёт
// .merged массив сегментов. Группирует подряд идущих одного спикера в
// блок-баббл. Owner справа, остальные слева. Цвет бейджа спикера
// стабилен на speaker_tag (hash → palette).
//
// Fallback: если raw_stt.json отсутствует (старые звонки до B10) —
// рендер ReactMarkdown оригинального transcript.md.

import { useEffect, useMemo, useRef } from 'react';
import ReactMarkdown from 'react-markdown';
import type { CallSpeakerView } from '../api/speakers';
import { Empty } from '../ui';
import { humanSpeakerLabel } from '../utils/callMeta';

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

// [B17] Atelier v2 speaker palette — cobalt/emerald/rust/indigo/teal из tokens.css
// (см. --sp-1..--sp-5). Owner всегда sp-1 (cobalt), остальные — циклом по hash.
function hashTag(tag: string): number {
  let h = 0;
  for (let i = 0; i < tag.length; i++) {
    h = (h * 31 + tag.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

function colorVarFor(tag: string): string {
  if (tag === OWNER_TAG) return 'var(--sp-1)';
  const idx = (hashTag(tag) % 4) + 2; // 2..5
  return `var(--sp-${idx})`;
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
  /** Клик по «? Кто это?» chip → открывает SpeakerConfirmModal на заданном теге. */
  onIdentifySpeaker?: (speakerTag: string) => void;
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

/** Set теэгов, которые ещё не подтверждены — для них показываем «Кто это?» chip. */
function buildUnconfirmedSet(speakers?: CallSpeakerView[]): Set<string> {
  const s = new Set<string>();
  if (!speakers) return s;
  for (const sp of speakers) {
    if (!sp.confirmed) s.add(sp.speaker_tag);
  }
  return s;
}

export function InteractiveTranscript({
  rawSttJson,
  fallbackMd,
  speakers,
  currentTime,
  onSeek,
  onIdentifySpeaker,
}: Props) {
  const labels = useMemo(() => buildLabelMap(speakers), [speakers]);
  const unconfirmed = useMemo(() => buildUnconfirmedSet(speakers), [speakers]);
  const segments: Segment[] | null = useMemo(() => {
    if (!rawSttJson) return null;
    const parsed = parseRawStt(rawSttJson);
    return parsed?.merged ?? null;
  }, [rawSttJson]);

  const groups = useMemo(
    () => (segments ? groupBySpeaker(segments) : []),
    [segments],
  );

  // [B17 V3.2] Index of group containing currentTime, or -1.
  const activeIdx = useMemo(() => {
    if (currentTime == null || groups.length === 0) return -1;
    for (let i = 0; i < groups.length; i++) {
      const g = groups[i]!;
      const start = g.segments[0]?.start ?? 0;
      const end = g.segments[g.segments.length - 1]?.end ?? start;
      if (currentTime >= start && currentTime <= end) return i;
    }
    return -1;
  }, [currentTime, groups]);

  // Auto-scroll active row into view (smooth, only when user not interacting).
  const activeRowRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (activeIdx < 0) return;
    const el = activeRowRef.current;
    if (!el) return;
    el.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }, [activeIdx]);

  if (!segments || segments.length === 0) {
    if (fallbackMd) {
      return (
        <div className="markdown">
          <ReactMarkdown>{fallbackMd}</ReactMarkdown>
        </div>
      );
    }
    return <Empty description="Транскрипт ещё не готов." />;
  }

  return (
    <div className="transcript">
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
          : humanSpeakerLabel(g.tag);
        const text = g.segments
          .map((s) => s.text.trim())
          .filter(Boolean)
          .join(' ');
        const isActive = idx === activeIdx;
        const isClickable = !!onSeek;
        return (
          <div
            key={`${g.tag}-${idx}`}
            ref={isActive ? activeRowRef : undefined}
            className="transcript-row"
            onClick={isClickable ? () => onSeek!(start) : undefined}
            style={{
              cursor: isClickable ? 'pointer' : 'default',
              background: isActive ? 'var(--accent-soft)' : 'transparent',
              borderLeft: isActive
                ? `3px solid ${color}`
                : '3px solid transparent',
              paddingLeft: 9,
              marginLeft: -12,
              borderRadius: 'var(--radius-sm)',
              transition:
                'background var(--duration-fast), border-color var(--duration-fast)',
            }}
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
            <div className="transcript-speaker" style={{ color }}>
              {firstName}
              {unconfirmed.has(g.tag) && onIdentifySpeaker && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onIdentifySpeaker(g.tag);
                  }}
                  className="transcript-identify-chip"
                  title="Кто это? Подтвердить голос"
                  aria-label={`Кто это? Подтвердить голос ${g.tag}`}
                >
                  ? кто это
                </button>
              )}
            </div>
            <div className="transcript-text">
              {text || <span className="subtle">…</span>}
            </div>
            <div className="transcript-time">{formatTimecode(start)}</div>
          </div>
        );
      })}
    </div>
  );
}
