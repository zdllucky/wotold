// [B10] M7.3 follow-up: интерактивный транскрипт chat-bubble layout.
//
// Парсит raw_stt.json (writeн в pipeline::run::persist_artifacts), берёт
// .merged массив сегментов. Группирует подряд идущих одного спикера в
// блок-баббл. Owner справа, остальные слева. Цвет бейджа спикера
// стабилен на speaker_tag (hash → palette).
//
// Fallback: если raw_stt.json отсутствует (старые звонки до B10) —
// рендер ReactMarkdown оригинального transcript.md.

import { useMemo } from 'react';
import ReactMarkdown from 'react-markdown';
import { Empty } from '../ui';

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

const OWNER_TAG = 'owner';

// Палитра для спикеров — циклится по hash(tag).
const SPEAKER_COLORS = [
  'var(--color-accent)',
  'var(--color-success)',
  'var(--color-warning)',
  'var(--color-danger)',
  'oklch(60% 0.15 280)', // purple
  'oklch(60% 0.15 30)', // orange
];

function hashTag(tag: string): number {
  let h = 0;
  for (let i = 0; i < tag.length; i++) {
    h = (h * 31 + tag.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

function colorFor(tag: string): string {
  if (tag === OWNER_TAG) return 'var(--color-accent)';
  return SPEAKER_COLORS[hashTag(tag) % SPEAKER_COLORS.length] ?? 'var(--color-text-muted)';
}

function formatTimecode(sec: number): string {
  const mm = Math.floor(sec / 60);
  const ss = Math.floor(sec % 60)
    .toString()
    .padStart(2, '0');
  return `${mm}:${ss}`;
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
}

export function InteractiveTranscript({ rawSttJson, fallbackMd }: Props) {
  const segments: Segment[] | null = useMemo(() => {
    if (!rawSttJson) return null;
    try {
      const parsed = JSON.parse(rawSttJson) as RawStt;
      return parsed.merged ?? null;
    } catch {
      return null;
    }
  }, [rawSttJson]);

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

  const groups = groupBySpeaker(segments);

  return (
    <div className="transcript-stream">
      {groups.map((g, idx) => {
        const isOwner = g.tag === OWNER_TAG;
        const start = g.segments[0]?.start ?? 0;
        const color = colorFor(g.tag);
        return (
          <div
            key={`${g.tag}-${idx}`}
            className="transcript-bubble"
            data-owner={isOwner ? 'true' : 'false'}
          >
            <div className="transcript-bubble-head" style={{ color }}>
              <span className="transcript-bubble-tag" style={{ backgroundColor: color }}>
                {g.tag === OWNER_TAG ? 'я' : g.tag}
              </span>
              <span className="transcript-bubble-time text-subtle">
                {formatTimecode(start)}
              </span>
            </div>
            <div className="transcript-bubble-body">
              {g.segments.map((s, i) => (
                <p key={`${idx}-${i}`} className="transcript-bubble-line">
                  {s.text.trim() || <span className="text-subtle">…</span>}
                </p>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
