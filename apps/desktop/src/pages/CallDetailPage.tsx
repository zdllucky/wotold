import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { humanError } from '../api/errors';
import ReactMarkdown from 'react-markdown';
import { ask, save } from '@tauri-apps/plugin-dialog';

import {
  deleteCall,
  exportCallMarkdown,
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
import { Empty, Tabs } from '../ui';
import { AudioScrubber } from '../components/AudioScrubber';
import { InteractiveTranscript } from '../components/InteractiveTranscript';
import { SpeakerConfirmModal } from '../components/SpeakerConfirmModal';
import { useCallAudio } from '../hooks/useCallAudio';
import { SpeakersSection, extractSamples } from './SpeakersSection';
import { convertFileSrc } from '@tauri-apps/api/core';
import { getCallAudioPath } from '../api/calls';
import {
  findSpeakerAtTime,
  formatHeaderMeta,
  hashCallId,
  pluralParticipants,
  simpleDateTitle,
} from '../utils/callMeta';

type Tab = 'recap' | 'transcript' | 'tasks' | 'speakers';

interface CallDetailPageProps {
  callId: string;
  onBack: () => void;
}

export function CallDetailPage({ callId, onBack }: CallDetailPageProps) {
  const [call, setCall] = useState<Call | null>(null);
  // [B17 V3.9] Default tab → transcript (per artboard §5 reference).
  const [tab, setTab] = useState<Tab>('transcript');
  const [recap, setRecap] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<string | null>(null);
  const [rawStt, setRawStt] = useState<string | null>(null);
  const [tasks, setTasks] = useState<ActionItem[] | null>(null);
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [speakersLite, setSpeakersLite] = useState<CallSpeakerView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  // [B17 V4.1] Inline-confirm popup из транскрипта — speaker_tag клика.
  const [confirmingTag, setConfirmingTag] = useState<string | null>(null);
  // Источники аудио + sample-extract — переиспользуются модалом.
  const [micSrc, setMicSrc] = useState<string | null>(null);
  const [systemSrc, setSystemSrc] = useState<string | null>(null);

  // [B17 V3.2] Single audio source — shared между AudioScrubber и
  // InteractiveTranscript (для highlight current + click-to-seek).
  const audio = useCallAudio(callId, call?.duration_sec ?? 0);

  // [B17 V3.3] Current speaker info — derived from rawStt segments + audio
  // currentTime. Используется в AudioScrubber SpeakerChip.
  const currentSpeaker = useMemo(
    () => findSpeakerAtTime(rawStt, speakersLite, audio.currentTime),
    [rawStt, speakersLite, audio.currentTime],
  );

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
      getCallAudioPath(callId, 'mic'),
      getCallAudioPath(callId, 'system'),
    ])
      .then(([rCall, rRecap, rTrans, rRaw, rTasks, rContacts, rSpeakers, rMic, rSys]) => {
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
        setMicSrc(rMic.status === 'fulfilled' ? convertFileSrc(rMic.value) : null);
        setSystemSrc(rSys.status === 'fulfilled' ? convertFileSrc(rSys.value) : null);
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
  const [exporting, setExporting] = useState(false);

  // [B17 V4.1] Per-tag sample bubble (text + start/end/src) — для модала
  // и (потенциально) для будущего sample-row inline-feature в транскрипте.
  const samplesByTag = useMemo(
    () => extractSamples(rawStt, micSrc, systemSrc),
    [rawStt, micSrc, systemSrc],
  );

  // [B17 V4.1] Перечитать speakers + contacts после mutation в табе или
  // inline modal. SpeakersSection + SpeakerConfirmModal вызывают это
  // → ParticipantsRow в шапке + chip'ы в транскрипте обновятся динамически.
  const refetchSpeakersAndContacts = async () => {
    try {
      const [s, c] = await Promise.all([
        listCallSpeakers(callId),
        listContacts(),
      ]);
      setSpeakersLite(s);
      setContacts(c);
    } catch (e) {
      console.warn('refetch speakers/contacts failed', e);
    }
  };

  const confirmingSpeaker = useMemo(
    () =>
      confirmingTag
        ? speakersLite.find((s) => s.speaker_tag === confirmingTag) ?? null
        : null,
    [confirmingTag, speakersLite],
  );

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

  const onExportMarkdown = async () => {
    if (!call) return;
    const defaultName = `${(call.title?.trim() || `wotold-${call.id.slice(0, 8)}`).replace(/[^\p{L}\p{N}_.-]/gu, '_')}.md`;
    let dest: string | null = null;
    try {
      dest = (await save({
        defaultPath: defaultName,
        filters: [{ name: 'Markdown', extensions: ['md'] }],
        title: 'Сохранить расшифровку звонка',
      })) as string | null;
    } catch (e) {
      setError(humanError(e));
      return;
    }
    if (!dest) return; // cancel
    setExporting(true);
    setError(null);
    try {
      await exportCallMarkdown(call.id, dest);
    } catch (e) {
      setError(humanError(e));
    } finally {
      setExporting(false);
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

  if (loading) return <p className="muted">Загрузка…</p>;
  if (error)
    return (
      <p role="alert" style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}>
        {error}
      </p>
    );
  if (!call) return <p className="muted">Звонок не найден.</p>;

  return (
    // [B17 V3.8] flex column + minHeight: 100% — scrubber последний child
    // получает marginTop: auto и прижимается к низу .app-main scroll viewport.
    // Без этого при коротком контенте (например пустой recap) sticky bottom
    // не активируется, scrubber висит в середине экрана.
    <section
      style={{
        display: 'flex',
        flexDirection: 'column',
        minHeight: '100%',
      }}
    >
      <button
        type="button"
        className="btn btn--quiet"
        onClick={onBack}
        style={{ marginBottom: 18, paddingLeft: 0 }}
      >
        ← Все звонки
      </button>

      <header style={{ marginBottom: 22, position: 'relative' }}>
        {/* Meta — human Russian per reference §5: ВТОРНИК · 19 МАЯ · 11:24 · 32 МИН 14 СЕК */}
        <div className="small-caps" style={{ marginBottom: 8 }}>
          {formatHeaderMeta(call)}
        </div>

        {/* Title — LLM-generated если есть, иначе простой fallback "Звонок · 20 мая" */}
        <h1
          className="title"
          style={{ fontSize: 36, margin: 0, marginBottom: 14 }}
        >
          {call.title?.trim() || simpleDateTitle(call)}
        </h1>

        {/* Action overflow — kebab menu top-right с reprocess/export/delete */}
        <HeaderActions
          onReprocess={() => void onReprocess()}
          onExport={() => void onExportMarkdown()}
          onDelete={onDelete}
          reprocessing={reprocessing}
          exporting={exporting}
          deleting={deleting}
        />

        {/* Participants chips — same row после title */}
        {speakersLite.length > 0 && (
          <ParticipantsRow speakers={speakersLite} />
        )}
      </header>

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
            currentTime={audio.currentTime}
            onSeek={(s) => {
              audio.seek(s);
              if (!audio.playing && audio.ready) audio.togglePlay();
            }}
            onIdentifySpeaker={(tag) => setConfirmingTag(tag)}
          />
        </Tabs.Panel>
        <Tabs.Panel value="tasks">
          <TasksPanel tasks={tasks ?? []} contacts={contacts} />
        </Tabs.Panel>
        <Tabs.Panel value="speakers">
          <SpeakersSection
            callId={callId}
            onSpeakersChanged={() => void refetchSpeakersAndContacts()}
          />
        </Tabs.Panel>
      </Tabs>

      {/* [B17 V3.1] Sticky-bottom audio scrubber pill — overflow'ит над
          контентом любого активного таба (transcript / recap / tasks /
          speakers). Скрыта только при status='failed'. */}
      <AudioScrubber
        audio={audio}
        seed={hashCallId(callId)}
        enabled={call.status !== 'failed'}
        currentSpeaker={currentSpeaker}
        onJumpToSpeaker={
          currentSpeaker ? () => setTab('transcript') : undefined
        }
      />

      {confirmingSpeaker && (
        <SpeakerConfirmModal
          speaker={confirmingSpeaker}
          contacts={contacts}
          sample={samplesByTag.get(confirmingSpeaker.speaker_tag) ?? null}
          onClose={() => setConfirmingTag(null)}
          onConfirmed={() => void refetchSpeakersAndContacts()}
        />
      )}
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

// Action overflow menu — kebab top-right с reprocess + export + delete.
function HeaderActions({
  onReprocess,
  onExport,
  onDelete,
  reprocessing,
  exporting,
  deleting,
}: {
  onReprocess: () => void;
  onExport: () => void;
  onDelete: () => void;
  reprocessing: boolean;
  exporting: boolean;
  deleting: boolean;
}) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!containerRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', handler);
    return () => window.removeEventListener('mousedown', handler);
  }, [open]);

  return (
    <div
      ref={containerRef}
      style={{
        position: 'absolute',
        top: 0,
        right: 0,
      }}
    >
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-label="Действия со звонком"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={reprocessing || deleting || exporting}
        style={{
          width: 32,
          height: 32,
          borderRadius: 'var(--radius-sm)',
          border: 'none',
          background: open ? 'var(--bg-2)' : 'transparent',
          color: 'var(--muted)',
          cursor: 'pointer',
          fontSize: 18,
          lineHeight: 1,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
        }}
        title="Действия"
      >
        ⋯
      </button>
      {open && (
        <div
          role="menu"
          style={{
            position: 'absolute',
            top: 'calc(100% + 6px)',
            right: 0,
            zIndex: 30,
            background: 'var(--paper)',
            border: '1px solid var(--line)',
            borderRadius: 'var(--radius-md)',
            boxShadow: 'var(--shadow-2)',
            padding: 4,
            minWidth: 180,
          }}
        >
          <MenuItem
            onClick={() => {
              setOpen(false);
              onReprocess();
            }}
            disabled={reprocessing || deleting || exporting}
          >
            {reprocessing ? 'Переобработка…' : '↻ Переобработать'}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onExport();
            }}
            disabled={exporting || reprocessing || deleting}
          >
            {exporting ? 'Сохраняем…' : '↓ Скачать .md'}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onDelete();
            }}
            disabled={deleting || reprocessing || exporting}
            danger
          >
            {deleting ? 'Удаляем…' : 'Удалить'}
          </MenuItem>
        </div>
      )}
    </div>
  );
}

function MenuItem({
  children,
  onClick,
  disabled,
  danger,
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      disabled={disabled}
      style={{
        display: 'block',
        width: '100%',
        textAlign: 'left',
        padding: '8px 12px',
        border: 'none',
        background: 'transparent',
        color: danger ? 'var(--signal)' : 'var(--ink)',
        fontSize: 13.5,
        fontFamily: 'var(--font-sans)',
        cursor: disabled ? 'not-allowed' : 'pointer',
        opacity: disabled ? 0.5 : 1,
        borderRadius: 'var(--radius-sm)',
      }}
      onMouseEnter={(e) => {
        if (!disabled) e.currentTarget.style.background = 'var(--bg-2)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = 'transparent';
      }}
    >
      {children}
    </button>
  );
}

// Participants row — sp chips для confirmed speakers + "· N участника".
function ParticipantsRow({ speakers }: { speakers: CallSpeakerView[] }) {
  const named = speakers.filter((s) => s.confirmed && s.contact_display_name);
  if (named.length === 0) return null;
  const SP = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];
  const initials = (name: string) =>
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('');
  const declN = pluralParticipants(named.length);
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        flexWrap: 'wrap',
      }}
    >
      {named.map((s, i) => (
        <span className="sp" key={s.id}>
          <span
            className="sp-avatar"
            style={{ background: SP[i % SP.length] }}
          >
            {initials(s.contact_display_name ?? '')}
          </span>
          {s.contact_display_name}
        </span>
      ))}
      <span className="muted" style={{ fontSize: 12, marginLeft: 4 }}>
        · {named.length} {declN}
      </span>
    </div>
  );
}

