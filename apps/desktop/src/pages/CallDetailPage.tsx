import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';

import {
  getCall,
  listCallActionItems,
  readCallArtifact,
  type ActionItem,
} from '../api/calls';
import { listContacts, type Contact } from '../api/contacts';
import type { Call } from '../api/recording';
import { Card, Empty, Pill, Tabs } from '../ui';

type Tab = 'recap' | 'transcript' | 'tasks';

interface CallDetailPageProps {
  callId: string;
  onBack: () => void;
}

export function CallDetailPage({ callId, onBack }: CallDetailPageProps) {
  const [call, setCall] = useState<Call | null>(null);
  const [tab, setTab] = useState<Tab>('recap');
  const [recap, setRecap] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<string | null>(null);
  const [tasks, setTasks] = useState<ActionItem[] | null>(null);
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    setError(null);
    Promise.all([
      getCall(callId),
      readCallArtifact(callId, 'recap'),
      readCallArtifact(callId, 'transcript'),
      listCallActionItems(callId),
      listContacts(),
    ])
      .then(([c, r, t, ai, cs]) => {
        setCall(c);
        setRecap(r);
        setTranscript(t);
        setTasks(ai);
        setContacts(cs);
      })
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [callId]);

  if (loading) return <p className="hint">Загрузка…</p>;
  if (error) return <p className="error">{error}</p>;
  if (!call) return <p className="hint">Звонок не найден.</p>;

  const tabHasContent: Record<Tab, boolean> = {
    recap: !!recap,
    transcript: !!transcript,
    tasks: (tasks?.length ?? 0) > 0,
  };

  return (
    <section className="call-detail-section">
      <button type="button" className="call-detail-back" onClick={onBack}>
        ← к списку
      </button>

      <header className="call-detail-header">
        <h2 className="call-detail-title">
          {call.title ?? `Звонок ${call.id.slice(0, 8)}`}
        </h2>
        <div className="call-detail-meta">
          <span>{formatStarted(call.started_at)}</span>
          <span>·</span>
          <span>{formatDuration(call.duration_sec)}</span>
          <Pill tone={statusTone(call.status)}>{call.status}</Pill>
          {call.provider && <span>· {call.provider}</span>}
          {call.lang_detected && <span>· {call.lang_detected}</span>}
        </div>
      </header>

      {call.status === 'failed' && call.failed_reason && (
        <Card className="call-failed-banner" variant="default">
          <div className="call-failed-head">
            <span className="call-failed-icon" aria-hidden>
              ⚠
            </span>
            <span className="call-failed-title">Транскрипция не удалась</span>
          </div>
          <p className="call-failed-reason">{call.failed_reason}</p>
        </Card>
      )}

      <Tabs value={tab} onChange={(v) => setTab(v as Tab)}>
        <Tabs.List>
          {(['recap', 'transcript', 'tasks'] as Tab[]).map((t) => (
            <Tabs.Trigger
              key={t}
              value={t}
              counter={!tabHasContent[t] ? '∅' : undefined}
            >
              {tabLabel(t)}
            </Tabs.Trigger>
          ))}
        </Tabs.List>

        <Tabs.Panel value="recap">
          <MdPanel md={recap} emptyHint="Рекап ещё не сгенерирован." />
        </Tabs.Panel>
        <Tabs.Panel value="transcript">
          <MdPanel md={transcript} emptyHint="Транскрипт ещё не готов." />
        </Tabs.Panel>
        <Tabs.Panel value="tasks">
          <TasksPanel tasks={tasks ?? []} contacts={contacts} />
        </Tabs.Panel>
      </Tabs>
    </section>
  );
}

function MdPanel({ md, emptyHint }: { md: string | null; emptyHint: string }) {
  if (!md) return <Empty description={emptyHint} />;
  return (
    <div className="markdown">
      <ReactMarkdown>{md}</ReactMarkdown>
    </div>
  );
}

function TasksPanel({ tasks, contacts }: { tasks: ActionItem[]; contacts: Contact[] }) {
  if (tasks.length === 0) {
    return <Empty description="Action items пусты." />;
  }
  const nameById = new Map(contacts.map((c) => [c.id, c.display_name]));
  return (
    <ul className="task-list">
      {tasks.map((t) => {
        const owner = t.owner_contact_id ? nameById.get(t.owner_contact_id) : null;
        return (
          <li key={t.id} className="task-row" data-done={t.done ? 'true' : 'false'}>
            <span className="task-check" aria-hidden>
              {t.done ? '☑' : '☐'}
            </span>
            <span className="task-text">{t.text}</span>
            {owner && <span className="task-owner">— {owner}</span>}
            {t.due && <span className="task-due">· до {t.due}</span>}
          </li>
        );
      })}
    </ul>
  );
}

function statusTone(status: string): 'accent' | 'success' | 'warning' | 'danger' | 'neutral' {
  switch (status) {
    case 'recording':
      return 'danger';
    case 'processing':
      return 'accent';
    case 'ready':
      return 'success';
    case 'failed':
      return 'danger';
    default:
      return 'neutral';
  }
}

function tabLabel(t: Tab): string {
  switch (t) {
    case 'recap':
      return 'Рекап';
    case 'transcript':
      return 'Расшифровка';
    case 'tasks':
      return 'Задачи';
  }
}

function formatStarted(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString('ru-RU', {
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

function formatDuration(sec: number | null): string {
  if (sec == null) return '—';
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}
