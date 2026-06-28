// [B17] Recap dossier tab per docs/design/atelier-v2/_reference/atelier-2.jsx §6.
// Two-column grid: main column (resume + key points + tasks) + 280px sidebar
// (metadata + participants + Экспорт в MD).

import ReactMarkdown from 'react-markdown';

import type { ActionItem } from '../api/calls';
import type { Contact } from '../api/contacts';
import type { Call } from '../api/recording';
import type { CallSpeakerView } from '../api/speakers';
import { Empty } from '../ui';
import { bcp47, useI18n } from '../i18n';
import { SP_COLORS, initials } from './CallDetailUtils';

interface RecapTabProps {
  recap: string | null;
  tasks: ActionItem[];
  contacts: Contact[];
  speakers: CallSpeakerView[];
  call: Call;
  onRegenerate: () => void;
  regenerating: boolean;
}

export function RecapTab({
  recap,
  tasks,
  contacts,
  speakers,
  call,
  onRegenerate,
  regenerating,
}: RecapTabProps) {
  const { locale, t } = useI18n();
  const parsed = parseRecap(recap);
  const participants = speakers.filter(
    (s) => s.confirmed && s.contact_display_name,
  );
  const nameById = new Map(contacts.map((c) => [c.id, c.display_name]));

  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: '1fr 280px',
        gap: 56,
        marginTop: 8,
      }}
    >
      <div>
        {parsed.summary ? (
          <section style={{ marginBottom: 32 }}>
            <div className="small-caps" style={{ marginBottom: 8 }}>
              {t('recap.summary')}
            </div>
            <div
              style={{
                fontFamily: 'var(--font)',
                fontSize: 19,
                lineHeight: 1.55,
                color: 'var(--text)',
                letterSpacing: '-0.005em',
              }}
            >
              {/* Markdown inside summary preserves **bold** → strong accent
                  per artboard §6 (key phrase highlighting). */}
              <ReactMarkdown
                components={{
                  p: ({ children }) => <p style={{ margin: 0 }}>{children}</p>,
                  strong: ({ children }) => (
                    <strong style={{ color: 'var(--accent)', fontWeight: 500 }}>
                      {children}
                    </strong>
                  ),
                }}
              >
                {parsed.summary}
              </ReactMarkdown>
            </div>
          </section>
        ) : null}

        {parsed.keyPoints.length > 0 && (
          <section style={{ marginBottom: 32 }}>
            <div className="small-caps" style={{ marginBottom: 12 }}>
              {t('recap.keyPoints')}
            </div>
            <ol
              style={{
                fontFamily: 'var(--font)',
                fontSize: 16,
                lineHeight: 1.6,
                paddingLeft: 0,
                listStyle: 'none',
                margin: 0,
              }}
            >
              {parsed.keyPoints.map((t, i) => (
                <li
                  key={i}
                  style={{
                    display: 'flex',
                    gap: 14,
                    padding: '6px 0',
                    borderBottom: '1px dotted var(--border-2)',
                  }}
                >
                  <span
                    className="mono"
                    style={{
                      color: 'var(--accent)',
                      minWidth: 22,
                      letterSpacing: '0.04em',
                    }}
                  >
                    {String(i + 1).padStart(2, '0')}
                  </span>
                  <span>{t}</span>
                </li>
              ))}
            </ol>
          </section>
        )}

        <section style={{ marginBottom: 32 }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'baseline',
              justifyContent: 'space-between',
              marginBottom: 12,
            }}
          >
            <div className="small-caps">{t('recap.tasksCount', { n: tasks.length })}</div>
            <button
              type="button"
              className="btn btn--quiet"
              style={{ padding: 0, fontSize: 12 }}
              onClick={onRegenerate}
              disabled={regenerating}
            >
              {regenerating ? t('recap.regenerating') : t('recap.regenerate')}
            </button>
          </div>
          {tasks.length === 0 ? (
            <Empty description={t('recap.emptyTasks')} />
          ) : (
            tasks.map((task, i) => (
              <TaskRow key={task.id ?? i} task={task} nameById={nameById} idx={i} />
            ))
          )}
        </section>

        {!parsed.summary && !parsed.keyPoints.length && recap && (
          <section style={{ marginBottom: 32 }}>
            <div className="small-caps" style={{ marginBottom: 8 }}>
              {t('recap.summaryAlt')}
            </div>
            <div className="markdown">
              <ReactMarkdown>{recap}</ReactMarkdown>
            </div>
          </section>
        )}

        {!recap && <Empty description={t('recap.emptyRecap')} />}
      </div>

      {/* Sidebar */}
      <aside>
        <div
          style={{
            borderTop: '1px solid var(--border)',
            borderBottom: '1px solid var(--border)',
            padding: '14px 0',
            marginBottom: 18,
          }}
        >
          <div className="small-caps" style={{ marginBottom: 10 }}>
            {t('recap.metadata')}
          </div>
          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              gap: 8,
              fontSize: 12,
            }}
          >
            {sidebarMeta(call, locale, t).map(([k, v]) => (
              <div
                key={k}
                style={{ display: 'flex', justifyContent: 'space-between' }}
              >
                <span className="muted">{k}</span>
                <span className="mono">{v}</span>
              </div>
            ))}
          </div>
        </div>

        {participants.length > 0 && (
          <>
            <div className="small-caps" style={{ marginBottom: 12 }}>
              {t('recap.participants')}
            </div>
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 10,
                marginBottom: 20,
              }}
            >
              {participants.map((p, i) => (
                <span className="sp" key={p.id} style={{ alignSelf: 'flex-start' }}>
                  <span
                    className="sp-avatar"
                    style={{ background: SP_COLORS[i % SP_COLORS.length] }}
                  >
                    {initials(p.contact_display_name ?? p.speaker_tag)}
                  </span>
                  {p.contact_display_name ?? p.speaker_tag}
                </span>
              ))}
            </div>
          </>
        )}

        {recap && (
          <button
            type="button"
            className="btn btn--ghost"
            style={{ width: '100%', justifyContent: 'center' }}
            onClick={() => exportMd(call, recap)}
          >
            {t('recap.exportMd')}
          </button>
        )}
      </aside>
    </div>
  );
}

export function TaskRow({
  task,
  nameById,
  idx,
}: {
  task: ActionItem;
  nameById: Map<string, string>;
  idx: number;
}) {
  const { t } = useI18n();
  const owner = task.owner_contact_id
    ? nameById.get(task.owner_contact_id) ?? null
    : null;
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'baseline',
        gap: 12,
        padding: '12px 0',
        borderBottom: '1px dotted var(--border-2)',
      }}
    >
      <span
        aria-hidden
        style={{
          width: 16,
          height: 16,
          border: `1.5px solid ${task.done ? 'var(--accent)' : 'var(--border)'}`,
          background: task.done ? 'var(--accent)' : 'transparent',
          borderRadius: 3,
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: 'var(--paper)',
          fontSize: 10,
          flexShrink: 0,
          position: 'relative',
          top: 3,
        }}
      >
        {task.done ? '✓' : ''}
      </span>
      <div style={{ flex: 1 }}>
        <div
          style={{
            fontFamily: 'var(--font)',
            fontSize: 16,
            color: 'var(--text)',
            textDecoration: task.done ? 'line-through' : 'none',
            opacity: task.done ? 0.55 : 1,
          }}
        >
          {task.text}
        </div>
        {task.due && (
          <div className="muted" style={{ fontSize: 12, marginTop: 2 }}>
            {t('recap.taskDue', { date: task.due })}
          </div>
        )}
      </div>
      {owner && (
        <span className="sp" style={{ flexShrink: 0 }}>
          <span
            className="sp-avatar"
            style={{ background: SP_COLORS[(idx + 1) % SP_COLORS.length] }}
          >
            {initials(owner)}
          </span>
          {owner}
        </span>
      )}
    </div>
  );
}

interface ParsedRecap {
  summary: string;
  keyPoints: string[];
}

function parseRecap(md: string | null): ParsedRecap {
  const out: ParsedRecap = { summary: '', keyPoints: [] };
  if (!md) return out;
  const lines = md.split(/\r?\n/);
  let section: 'summary' | 'key' | null = null;
  const summaryBuf: string[] = [];
  for (const raw of lines) {
    const line = raw.trim();
    if (!line) continue;
    const heading = line.match(/^#{1,3}\s+(.+)/);
    if (heading) {
      const h = heading[1]!.toLowerCase();
      if (/резюм|summary|обзор|итог/.test(h)) section = 'summary';
      else if (/ключев|key.*point|moments|пункт/.test(h)) section = 'key';
      else section = null;
      continue;
    }
    const bullet =
      line.match(/^[-*•]\s+(.+)/) || line.match(/^\d+[.)]\s+(.+)/);
    if (section === 'key' && bullet) {
      out.keyPoints.push(bullet[1]!);
      continue;
    }
    if (section === 'summary') {
      summaryBuf.push(line);
    } else if (!section && !out.summary && !out.keyPoints.length) {
      summaryBuf.push(line);
    }
  }
  out.summary = summaryBuf.join(' ').slice(0, 600);
  return out;
}

type TFn = ReturnType<typeof useI18n>['t'];

function sidebarMeta(call: Call, locale: string, t: TFn): Array<[string, string]> {
  const items: Array<[string, string]> = [];
  try {
    const d = new Date(call.started_at);
    items.push([
      t('recap.metaDate'),
      d.toLocaleDateString(bcp47(locale as Parameters<typeof bcp47>[0]), {
        day: 'numeric',
        month: 'long',
        year: 'numeric',
      }),
    ]);
  } catch {
    items.push([t('recap.metaDate'), call.started_at]);
  }
  if (call.provider) items.push([t('recap.metaProvider'), call.provider]);
  if (call.lang_detected) items.push([t('recap.metaLang'), call.lang_detected]);
  items.push([t('recap.metaDuration'), formatDur(call.duration_sec ?? 0)]);
  items.push([t('recap.metaId'), call.id.slice(0, 8)]);
  return items;
}

function formatDur(sec: number): string {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }
  return `${m}:${s.toString().padStart(2, '0')}`;
}

async function exportMd(call: Call, recap: string): Promise<void> {
  try {
    const blob = new Blob([recap], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${call.title ?? call.id.slice(0, 8)}.md`;
    a.click();
    URL.revokeObjectURL(url);
  } catch (e) {
    console.warn('export md failed', e);
  }
}
