import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import { ask } from '@tauri-apps/plugin-dialog';

import {
  deleteCall,
  getCall,
  listCallActionItems,
  readCallArtifact,
  regenerateRecap,
  reprocessCall,
  type ActionItem,
} from '../api/calls';
import { listContacts, type Contact } from '../api/contacts';
import type { Call } from '../api/recording';
import { Button, Card, Empty, Pill, Tabs } from '../ui';
import { InteractiveTranscript } from '../components/InteractiveTranscript';
import { SpeakersSection } from './SpeakersSection';

type Tab = 'recap' | 'transcript' | 'tasks' | 'speakers';

interface CallDetailPageProps {
  callId: string;
  onBack: () => void;
}

export function CallDetailPage({ callId, onBack }: CallDetailPageProps) {
  const [call, setCall] = useState<Call | null>(null);
  const [tab, setTab] = useState<Tab>('recap');
  const [recap, setRecap] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<string | null>(null);
  const [rawStt, setRawStt] = useState<string | null>(null);
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
      readCallArtifact(callId, 'raw_stt'),
      listCallActionItems(callId),
      listContacts(),
    ])
      .then(([c, r, t, raw, ai, cs]) => {
        setCall(c);
        setRecap(r);
        setTranscript(t);
        setRawStt(raw);
        setTasks(ai);
        setContacts(cs);
      })
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [callId]);

  const [deleting, setDeleting] = useState(false);
  const [regenerating, setRegenerating] = useState(false);
  const [reprocessing, setReprocessing] = useState(false);

  const onReprocess = async () => {
    if (!call) return;
    const ok = await ask(
      `Перезапустить обработку звонка?\n\nЗаново прогонит STT (Soniox/Gladia) и recap (Groq) на существующих mic.wav + system.wav. Текущий transcript и recap будут перезаписаны.`,
      { title: 'Wotold', kind: 'warning', okLabel: 'Перезапустить', cancelLabel: 'Отмена' },
    );
    if (!ok) return;
    setReprocessing(true);
    setError(null);
    try {
      await reprocessCall(call.id);
      // Pipeline:finished event на бекенде сам триггерит refresh где надо;
      // здесь явно перечитаем артефакты.
      const [fresh, freshTranscript, freshRaw, freshTasks, freshCall] = await Promise.all([
        readCallArtifact(call.id, 'recap'),
        readCallArtifact(call.id, 'transcript'),
        readCallArtifact(call.id, 'raw_stt'),
        listCallActionItems(call.id),
        getCall(call.id),
      ]);
      setRecap(fresh);
      setTranscript(freshTranscript);
      setRawStt(freshRaw);
      setTasks(freshTasks);
      setCall(freshCall);
    } catch (e) {
      setError(`Не удалось перезапустить: ${String(e)}`);
    } finally {
      setReprocessing(false);
    }
  };

  const onRegenerateRecap = async () => {
    setRegenerating(true);
    setError(null);
    try {
      await regenerateRecap(callId);
      // Перечитываем артефакты + action items.
      const [fresh, freshTasks] = await Promise.all([
        readCallArtifact(callId, 'recap'),
        listCallActionItems(callId),
      ]);
      setRecap(fresh);
      setTasks(freshTasks);
    } catch (e) {
      setError(`Не удалось перегенерить рекап: ${String(e)}`);
    } finally {
      setRegenerating(false);
    }
  };

  const onDelete = async () => {
    if (!call) return;
    const ok = await ask(
      `Удалить звонок «${call.title ?? call.id.slice(0, 8)}»?\n\nЭто навсегда удалит аудио, транскрипт, рекап, задачи и связанные voice samples.`,
      { title: 'Wotold', kind: 'warning', okLabel: 'Удалить', cancelLabel: 'Отмена' },
    );
    if (!ok) return;
    setDeleting(true);
    try {
      await deleteCall(call.id);
      onBack();
    } catch (e) {
      setError(String(e));
      setDeleting(false);
    }
  };

  if (loading) return <p className="hint">Загрузка…</p>;
  if (error) return <p className="error">{error}</p>;
  if (!call) return <p className="hint">Звонок не найден.</p>;

  const tabHasContent: Record<Tab, boolean> = {
    recap: !!recap,
    transcript: !!transcript,
    tasks: (tasks?.length ?? 0) > 0,
    // Спикеры рассчитываются внутри SpeakersSection — counter нет смысла считать
    // здесь без второго round-trip; пусть всегда показывается без ∅-маркера.
    speakers: true,
  };

  return (
    <section className="call-detail-section">
      <button type="button" className="call-detail-back" onClick={onBack}>
        ← к списку
      </button>

      <header className="call-detail-header">
        <div className="call-detail-title-row">
          <h2 className="call-detail-title">
            {call.title ?? `Звонок ${call.id.slice(0, 8)}`}
          </h2>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void onReprocess()}
            disabled={reprocessing || deleting}
            busy={reprocessing}
            title="Заново прогнать STT + recap на существующих аудио"
          >
            {reprocessing ? 'Переобработка…' : '↻ Переобработать'}
          </Button>
          <Button
            variant="danger"
            size="sm"
            onClick={onDelete}
            disabled={deleting || reprocessing}
            busy={deleting}
          >
            Удалить
          </Button>
        </div>
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
          {(['recap', 'transcript', 'tasks', 'speakers'] as Tab[]).map((t) => (
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
          <div className="recap-panel-head">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => void onRegenerateRecap()}
              disabled={regenerating || !transcript}
              busy={regenerating}
              title={!transcript ? 'Нет транскрипта для регенерации' : undefined}
            >
              {regenerating ? 'Пересоздаём…' : '↻ Пересоздать рекап'}
            </Button>
          </div>
          <MdPanel md={recap} emptyHint="Рекап ещё не сгенерирован." />
        </Tabs.Panel>
        <Tabs.Panel value="transcript">
          <InteractiveTranscript rawSttJson={rawStt} fallbackMd={transcript} />
        </Tabs.Panel>
        <Tabs.Panel value="tasks">
          <TasksPanel tasks={tasks ?? []} contacts={contacts} />
        </Tabs.Panel>
        <Tabs.Panel value="speakers">
          <SpeakersSection callId={callId} />
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
    case 'speakers':
      return 'Спикеры';
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
