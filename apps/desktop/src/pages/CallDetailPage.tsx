import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';
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
import { listCallSpeakers, type CallSpeakerView } from '../api/speakers';
import type { Call } from '../api/recording';
import { Empty, Pill, Tabs } from '../ui';
import { CallAudioPlayer } from '../components/CallAudioPlayer';
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
  const [speakersLite, setSpeakersLite] = useState<CallSpeakerView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    setError(null);
    // [B16 audit P1]: allSettled чтобы failed artifact (например ещё не существует
    // recap.md когда юзер открыл звонок до завершения pipeline) не ломал все state setters.
    Promise.allSettled([
      getCall(callId),
      readCallArtifact(callId, 'recap'),
      readCallArtifact(callId, 'transcript'),
      readCallArtifact(callId, 'raw_stt'),
      listCallActionItems(callId),
      listContacts(),
      listCallSpeakers(callId),
    ])
      .then(([rCall, rRecap, rTrans, rRaw, rTasks, rContacts, rSpeakers]) => {
        // Call meta — критично. Без неё страница не имеет смысла.
        if (rCall.status === 'fulfilled') {
          setCall(rCall.value);
        } else {
          setError(humanError(rCall.reason));
        }
        if (rRecap.status === 'fulfilled') setRecap(rRecap.value);
        if (rTrans.status === 'fulfilled') setTranscript(rTrans.value);
        if (rRaw.status === 'fulfilled') setRawStt(rRaw.value);
        if (rTasks.status === 'fulfilled') setTasks(rTasks.value);
        if (rContacts.status === 'fulfilled') setContacts(rContacts.value);
        if (rSpeakers.status === 'fulfilled') setSpeakersLite(rSpeakers.value);
        // Log невидимые failures чтобы они не исчезли silent.
        for (const [name, r] of [
          ['recap', rRecap],
          ['transcript', rTrans],
          ['raw_stt', rRaw],
          ['tasks', rTasks],
          ['contacts', rContacts],
          ['speakers', rSpeakers],
        ] as const) {
          if (r.status === 'rejected') console.warn(`CallDetail ${name} load failed`, r.reason);
        }
      })
      .finally(() => setLoading(false));
  }, [callId]);

  const [deleting, setDeleting] = useState(false);
  const [regenerating, setRegenerating] = useState(false);
  const [reprocessing, setReprocessing] = useState(false);

  const onReprocess = async () => {
    if (!call) return;
    const ok = await ask(
      `Перезапустить обработку звонка?\n\nЗапись будет заново распознана и пересоздана саммари. Текущая расшифровка и рекап перезапишутся.`,
      { title: 'Wotold', kind: 'warning', okLabel: 'Перезапустить', cancelLabel: 'Отмена' },
    );
    if (!ok) return;
    setReprocessing(true);
    setError(null);
    try {
      await reprocessCall(call.id);
      // Pipeline:finished event на бекенде сам триггерит refresh где надо;
      // здесь явно перечитаем артефакты.
      const [fresh, freshTranscript, freshRaw, freshTasks, freshCall, freshSpeakers] = await Promise.all([
        readCallArtifact(call.id, 'recap'),
        readCallArtifact(call.id, 'transcript'),
        readCallArtifact(call.id, 'raw_stt'),
        listCallActionItems(call.id),
        getCall(call.id),
        listCallSpeakers(call.id),
      ]);
      setRecap(fresh);
      setTranscript(freshTranscript);
      setRawStt(freshRaw);
      setTasks(freshTasks);
      setCall(freshCall);
      setSpeakersLite(freshSpeakers);
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
      setError(`Не удалось пересоздать саммари: ${String(e)}`);
    } finally {
      setRegenerating(false);
    }
  };

  const onDelete = async () => {
    if (!call) return;
    const ok = await ask(
      `Удалить звонок «${call.title ?? call.id.slice(0, 8)}»?\n\nЭто навсегда удалит запись, расшифровку, саммари, задачи и образцы голоса этого звонка.`,
      { title: 'Wotold', kind: 'warning', okLabel: 'Удалить', cancelLabel: 'Отмена' },
    );
    if (!ok) return;
    setDeleting(true);
    try {
      await deleteCall(call.id);
      onBack();
    } catch (e) {
      setError(humanError(e));
      setDeleting(false);
    }
  };

  if (loading) return <p className="hint">Загрузка…</p>;
  if (error) return <p className="error">{error}</p>;
  if (!call) return <p className="hint">Звонок не найден.</p>;

  return (
    <section>
      <button
        type="button"
        className="btn btn--quiet"
        onClick={onBack}
        style={{ marginBottom: 18, paddingLeft: 0 }}
      >
        ← Все звонки
      </button>

      <header style={{ marginBottom: 24 }}>
        <div className="small-caps" style={{ marginBottom: 8 }}>
          {formatStarted(call.started_at)} · {formatDuration(call.duration_sec)}
          {call.provider ? ` · ${call.provider}` : ''}
          {call.lang_detected ? ` · ${call.lang_detected.toUpperCase()}` : ''}
        </div>
        <div
          style={{
            display: 'flex',
            alignItems: 'flex-start',
            gap: 18,
            flexWrap: 'wrap',
            marginBottom: 8,
          }}
        >
          <h1 className="title" style={{ fontSize: 36, flex: 1, minWidth: 240 }}>
            {call.title ?? deriveAutoTitle(call, speakersLite)}
          </h1>
          <div style={{ display: 'flex', gap: 8 }}>
            <Pill tone={statusTone(call.status)}>{call.status}</Pill>
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={() => void onReprocess()}
              disabled={reprocessing || deleting}
              title="Заново прогнать STT + recap на существующих аудио"
            >
              {reprocessing ? 'Переобработка…' : '↻ Переобработать'}
            </button>
            <button
              type="button"
              className="btn btn--danger btn--sm"
              onClick={onDelete}
              disabled={deleting || reprocessing}
            >
              {deleting ? 'Удаляем…' : 'Удалить'}
            </button>
          </div>
        </div>
      </header>

      {call.status !== 'failed' && <CallAudioPlayer callId={callId} />}

      {call.status === 'failed' && call.failed_reason && (
        <div
          className="card"
          style={{
            marginBottom: 18,
            borderColor: 'var(--warning)',
          }}
        >
          <div className="small-caps" style={{ color: 'var(--warning)', marginBottom: 6 }}>
            ⚠ Не удалось распознать речь
          </div>
          <p
            style={{
              fontFamily: 'var(--font-serif)',
              fontSize: 16,
              margin: '0 0 14px',
            }}
          >
            {call.failed_reason}
          </p>
          <button
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => void onReprocess()}
            disabled={reprocessing}
          >
            {reprocessing ? 'Перезапускаем…' : 'Попробовать ещё раз'}
          </button>
        </div>
      )}

      {call.recap_failed_reason && call.status !== 'failed' && (
        <div
          className="card"
          style={{
            marginBottom: 18,
            borderColor: 'var(--warning)',
          }}
        >
          <div className="small-caps" style={{ color: 'var(--warning)', marginBottom: 6 }}>
            ⚠ Не удалось создать саммари
          </div>
          <p
            style={{
              fontFamily: 'var(--font-serif)',
              fontSize: 16,
              margin: '0 0 14px',
            }}
          >
            {call.recap_failed_reason}
          </p>
          <button
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => void onRegenerateRecap()}
            disabled={regenerating}
          >
            {regenerating ? 'Пересоздаём…' : '↻ Пересоздать саммари'}
          </button>
        </div>
      )}

      <Tabs value={tab} onChange={(v) => setTab(v as Tab)}>
        <Tabs.List>
          {(['recap', 'transcript', 'tasks', 'speakers'] as Tab[]).map((t) => (
            <Tabs.Trigger key={t} value={t}>
              {tabLabel(t)}
            </Tabs.Trigger>
          ))}
        </Tabs.List>

        <Tabs.Panel value="recap">
          <div
            style={{
              display: 'flex',
              justifyContent: 'flex-end',
              marginBottom: 14,
            }}
          >
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={() => void onRegenerateRecap()}
              disabled={regenerating || !transcript}
              title={!transcript ? 'Нет транскрипта для регенерации' : undefined}
            >
              {regenerating ? 'Пересоздаём…' : '↻ Пересоздать саммари'}
            </button>
          </div>
          <MdPanel md={recap} emptyHint="Саммари ещё не сгенерировано." />
        </Tabs.Panel>
        <Tabs.Panel value="transcript">
          <InteractiveTranscript
            rawSttJson={rawStt}
            fallbackMd={transcript}
            speakers={speakersLite}
          />
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
    <div
      style={{
        fontFamily: 'var(--font-serif)',
        fontSize: 17,
        lineHeight: 1.6,
        color: 'var(--ink)',
      }}
    >
      <ReactMarkdown>{md}</ReactMarkdown>
    </div>
  );
}

function TasksPanel({ tasks, contacts }: { tasks: ActionItem[]; contacts: Contact[] }) {
  if (tasks.length === 0) {
    return (
      <Empty description="Здесь будут задачи, упомянутые в звонке. Пока Wotold их не нашёл — попробуй переобработать звонок или дождись пересборки." />
    );
  }
  const nameById = new Map(contacts.map((c) => [c.id, c.display_name]));
  return (
    <ol style={{ listStyle: 'none', padding: 0, margin: 0 }}>
      {tasks.map((t, i) => {
        const owner = t.owner_contact_id ? nameById.get(t.owner_contact_id) : null;
        return (
          <li
            key={t.id}
            style={{
              display: 'grid',
              gridTemplateColumns: '24px 1fr auto',
              gap: 14,
              padding: '12px 0',
              borderTop: i === 0 ? 'none' : '1px solid var(--line-soft)',
              alignItems: 'baseline',
              color: t.done ? 'var(--muted)' : 'var(--ink)',
              textDecoration: t.done ? 'line-through' : 'none',
            }}
          >
            <span
              className="mono"
              style={{
                color: 'var(--accent)',
                fontSize: 13,
              }}
              aria-hidden
            >
              {String(i + 1).padStart(2, '0')}
            </span>
            <span
              style={{
                fontFamily: 'var(--font-serif)',
                fontSize: 16,
              }}
            >
              {t.text}
              {owner && (
                <span className="muted" style={{ fontSize: 13, marginLeft: 8 }}>
                  — {owner}
                </span>
              )}
              {t.due && (
                <span className="muted" style={{ fontSize: 13, marginLeft: 8 }}>
                  · до {t.due}
                </span>
              )}
            </span>
            <span
              className="small-caps"
              style={{
                fontSize: 10,
                color: t.done ? 'var(--success)' : 'var(--muted)',
              }}
            >
              {t.done ? '✓ done' : 'open'}
            </span>
          </li>
        );
      })}
    </ol>
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
      return 'Саммари';
    case 'transcript':
      return 'Расшифровка';
    case 'tasks':
      return 'Задачи';
    case 'speakers':
      return 'Участники';
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

// [B16] Auto-name для звонка без title — берём имя первого confirmed
// контакта (не owner) + дату. Так список перестаёт выглядеть как
// «Звонок a0f3…», «Звонок 5d21…», начинают читаться по существу.
function deriveAutoTitle(call: Call, speakers: CallSpeakerView[]): string {
  const dateStr = (() => {
    try {
      return new Date(call.started_at).toLocaleDateString('ru-RU', {
        day: '2-digit',
        month: 'short',
      });
    } catch {
      return '';
    }
  })();

  const namedSpeakers = speakers
    .filter((s) => s.confirmed && s.contact_display_name)
    .map((s) => s.contact_display_name!);

  if (namedSpeakers.length > 0) {
    const primary = namedSpeakers[0];
    const suffix = namedSpeakers.length > 1 ? ` +${namedSpeakers.length - 1}` : '';
    return dateStr ? `${primary}${suffix} · ${dateStr}` : `${primary}${suffix}`;
  }

  return dateStr ? `Звонок · ${dateStr}` : `Звонок ${call.id.slice(0, 8)}`;
}
