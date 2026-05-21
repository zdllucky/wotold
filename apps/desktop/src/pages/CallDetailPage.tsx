import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { humanError } from '../api/errors';
import ReactMarkdown from 'react-markdown';
import { ask, save } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  cancelReprocess,
  deleteCall,
  exportCallMarkdown,
  getCall,
  listCallActionItems,
  readCallArtifact,
  regenerateRecap,
  reprocessCall,
  type ActionItem,
  type PipelineCancelledEvent,
} from '../api/calls';
import { listContacts, type Contact } from '../api/contacts';
import {
  listCallSpeakers,
  unbindCallSpeaker,
  type CallAutoBoundEvent,
  type CallSpeakerView,
} from '../api/speakers';
import type { Call, CallProgressEvent } from '../api/recording';
import { Empty, Tabs } from '../ui';
import { CallStateTag, PipelineStrip } from '../components/call-state';
import { PIPELINE_STEP_KEYS, type CallProgress } from '../types/callState';
import { AudioScrubber } from '../components/AudioScrubber';
import { InteractiveTranscript } from '../components/InteractiveTranscript';
import { SpeakerConfirmModal } from '../components/SpeakerConfirmModal';
import { useCallAudio } from '../hooks/useCallAudio';
import { useI18n } from '../i18n';
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
  const { t } = useI18n();
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

  // [V7] auto-bound event — pipeline закончил matching и нашёл N speaker'ов
  // с score >= threshold. Refetch speakers и показываем undo баннер.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<CallAutoBoundEvent>('call:auto_bound', (e) => {
      if (e.payload.call_id !== callId) return;
      void refetchSpeakersAndContacts();
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('call:auto_bound listener:', err));
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refetch handle stable across renders
  }, [callId]);

  // [V8] Lifecycle events — pipeline:started / pipeline:finished /
  // pipeline:cancelled триггерят refetch'и для текущего звонка.
  // На started: refetch call meta (статус 'processing' уже стоит, ок).
  // На finished/cancelled: refetch всё (артефакты могли обновиться).
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const attach = async () => {
      try {
        const u1 = await listen<{ call_id: string }>('pipeline:started', (e) => {
          if (e.payload.call_id !== callId) return;
          // Status уже processing — лёгкий refetch только meta для синка.
          void getCall(callId).then((c) => c && setCall(c));
        });
        unlisteners.push(u1);
        const u2 = await listen<{ call_id: string; status: string }>(
          'pipeline:finished',
          (e) => {
            if (e.payload.call_id !== callId) return;
            void refetchAll();
          },
        );
        unlisteners.push(u2);
        const u3 = await listen<PipelineCancelledEvent>(
          'pipeline:cancelled',
          (e) => {
            if (e.payload.call_id !== callId) return;
            void refetchAll();
          },
        );
        unlisteners.push(u3);
      } catch (e) {
        console.warn('pipeline lifecycle listeners failed:', e);
      }
    };
    void attach();
    return () => {
      for (const u of unlisteners) u();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refetchAll stable per callId
  }, [callId]);

  // [V6.4] Live pipeline progress — слушаем `call:progress` события для этого
  // звонка и патчим Call object. UI: PipelineStrip + reassurance banner.
  // Без этого юзеру пришлось бы вручную F5 чтобы увидеть переход step 2→3.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<CallProgressEvent>('call:progress', (e) => {
      if (e.payload.call_id !== callId) return;
      setCall((prev) =>
        prev
          ? {
              ...prev,
              pipeline_step: e.payload.step,
              pipeline_pct: e.payload.pct,
              pipeline_eta_sec: e.payload.eta_sec,
              upload_bytes: e.payload.upload_bytes,
            }
          : prev,
      );
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('call:progress listener:', err));
    return () => unlisten?.();
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

  // [V8] Refetch full state — артефакты + call meta + speakers. Используется
  // и в reprocess, и в pipeline:finished/cancelled listeners.
  const refetchAll = async () => {
    if (!call) return;
    const [fresh, freshTranscript, freshRaw, freshTasks, freshCall, freshSpeakers] =
      await Promise.allSettled([
        readCallArtifact(call.id, 'recap'),
        readCallArtifact(call.id, 'transcript'),
        readCallArtifact(call.id, 'raw_stt'),
        listCallActionItems(call.id),
        getCall(call.id),
        listCallSpeakers(call.id),
      ]);
    if (fresh.status === 'fulfilled') setRecap(fresh.value);
    if (freshTranscript.status === 'fulfilled') setTranscript(freshTranscript.value);
    if (freshRaw.status === 'fulfilled') setRawStt(freshRaw.value);
    if (freshTasks.status === 'fulfilled') setTasks(freshTasks.value);
    if (freshCall.status === 'fulfilled') setCall(freshCall.value);
    if (freshSpeakers.status === 'fulfilled') setSpeakersLite(freshSpeakers.value);
  };

  const onReprocess = async () => {
    if (!call) return;
    const ok = await ask(t('callDetail.reprocessConfirmBody'), {
      title: t('callDetail.reprocessConfirmTitle'),
      kind: 'warning',
      okLabel: t('callDetail.reprocessConfirmOk'),
      cancelLabel: t('common.cancel'),
    });
    if (!ok) return;
    setReprocessing(true);
    setError(null);
    // [V8] Optimistic patch — сразу переводим call.status='processing' чтобы
    // ReprocessBanner показался. Backend `reprocess_call` теперь spawn'ит
    // task и возвращается мгновенно; точное состояние подтянется через
    // `call:progress` / `pipeline:finished` события.
    setCall((prev) =>
      prev
        ? {
            ...prev,
            status: 'processing',
            pipeline_step: 1,
            pipeline_pct: 0,
            pipeline_eta_sec: null,
            upload_bytes: null,
          }
        : prev,
    );
    try {
      await reprocessCall(call.id);
    } catch (e) {
      setError(t('callDetail.reprocessFailed', { error: String(e) }));
      // Откат optimistic patch — если backend сразу же отверг (например
      // нет аудио на диске), возвращаем status каким был.
      await refetchAll();
    } finally {
      setReprocessing(false);
    }
  };

  // [V8] Cancel running reprocess. Backend abort'ает pipeline task и
  // восстанавливает status='ready' (если артефакты пережили) или
  // 'failed' (первичная отмена). pipeline:cancelled listener подтянет.
  const onCancelReprocess = async () => {
    if (!call) return;
    try {
      await cancelReprocess(call.id);
    } catch (e) {
      console.warn('cancel reprocess failed:', e);
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
      setError(t('callDetail.regenerateFailed', { error: String(e) }));
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
        title: t('callDetail.exportTitle'),
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
      t('callDetail.deleteConfirmBody', { title: call.title ?? call.id.slice(0, 8) }),
      {
        title: 'Wotold',
        kind: 'warning',
        okLabel: t('callDetail.deleteConfirmOk'),
        cancelLabel: t('common.cancel'),
      },
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

  if (loading) return <p className="muted">{t('common.loading')}</p>;
  if (error)
    return (
      <p role="alert" style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}>
        {error}
      </p>
    );
  if (!call) return <p className="muted">{t('callDetail.notFound')}</p>;

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
        {t('common.backAll')}
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
          onRegenerateRecap={() => void onRegenerateRecap()}
          onExport={() => void onExportMarkdown()}
          onDelete={onDelete}
          reprocessing={reprocessing}
          regenerating={regenerating}
          regenerateDisabled={!transcript}
          exporting={exporting}
          deleting={deleting}
        />

        {/* Participants chips — same row после title */}
        {speakersLite.length > 0 && (
          <ParticipantsRow speakers={speakersLite} />
        )}
      </header>

      {/* [V8] Если есть прежние артефакты (recap или transcript) → это
          reprocess, рендерим компактный баннер с Cancel и оставляем старый
          контент видимым в табах. Иначе первичная обработка → полный
          ProcessingPanel с ghost-rows (без Cancel — нечего отменять
          к чистому состоянию). */}
      {call.status === 'processing' &&
        (recap || transcript ? (
          <ReprocessBanner
            call={call}
            onCancel={() => void onCancelReprocess()}
          />
        ) : (
          <ProcessingPanel call={call} />
        ))}

      {call.status === 'failed' && (
        <ErrorScreen
          call={call}
          reprocessing={reprocessing}
          onRetry={() => void onReprocess()}
        />
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
            {t('callDetail.recapFailBadge')}
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
            {regenerating ? t('callDetail.regenerating') : t('callDetail.regenerateRecap')}
          </button>
        </div>
      )}

      {/* [V7] Auto-bound banner — показывается пока есть speaker'ы с
          auto_bound_at, дает явный undo и аудит происхождения привязки. */}
      <AutoBoundBanner
        speakers={speakersLite}
        onUndone={() => void refetchSpeakersAndContacts()}
      />

      <Tabs value={tab} onChange={(v) => setTab(v as Tab)}>
        <Tabs.List>
          {(['recap', 'transcript', 'tasks', 'speakers'] as Tab[]).map((tabId) => (
            <Tabs.Trigger key={tabId} value={tabId}>
              {tabLabel(tabId, t)}
            </Tabs.Trigger>
          ))}
        </Tabs.List>

        <Tabs.Panel value="recap">
          {/* [V5.4] Кнопка «↻ Пересоздать саммари» перенесена в kebab
              menu (HeaderActions) — было два «обращения» к одной операции,
              UI clutter. Failed-banner внизу всё ещё имеет inline CTA
              для retry, потому что там это критичный fix-state. */}
          <MdPanel md={recap} emptyHint={t('callDetail.emptyRecap')} />
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
          speakers).
          [V6.5] Включаем и для failed: аудио сохранено локально, юзер
          должен иметь возможность послушать запись даже если транскрипт
          не получился. enabled=false только когда нет ни одной дорожки. */}
      <AudioScrubber
        audio={audio}
        seed={hashCallId(callId)}
        enabled
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

/**
 * [V7] Баннер «Авто-привязано: N · ↩ Отменить». Рендерится пока есть
 * speaker'ы с `auto_bound_at != null` AND `confirmed=1`. Один клик «отменить»
 * unbind'ит все авто-привязки этого звонка (caveat: не трогает manual
 * confirmed). Юзер может потом пере-подтвердить вручную через таб
 * «Участники» или inline «? кто это» chip.
 */
function AutoBoundBanner({
  speakers,
  onUndone,
}: {
  speakers: CallSpeakerView[];
  onUndone: () => void;
}) {
  const { t } = useI18n();
  const [undoing, setUndoing] = useState(false);
  const autoBound = speakers.filter(
    (s) => s.auto_bound_at != null && s.confirmed && s.contact_id,
  );
  if (autoBound.length === 0) return null;
  const names = autoBound
    .map((s) => s.contact_display_name)
    .filter((n): n is string => Boolean(n))
    .join(', ');
  const handleUndo = async () => {
    setUndoing(true);
    try {
      await Promise.all(autoBound.map((s) => unbindCallSpeaker(s.id)));
    } catch (e) {
      console.warn('auto-bound undo failed:', e);
    } finally {
      setUndoing(false);
      onUndone();
    }
  };
  return (
    <div
      className="activity-strip"
      data-comment-anchor="call-auto-bound-banner"
      style={{ marginBottom: 14 }}
    >
      <span className="stat-tag-dot" aria-hidden="true" />
      <span>
        {autoBound.length === 1
          ? t('callDetail.autoBoundOne', { name: names })
          : t('callDetail.autoBoundMany', { n: autoBound.length, names })}
      </span>
      <button
        type="button"
        className="btn btn--quiet btn--sm"
        onClick={() => void handleUndo()}
        disabled={undoing}
        style={{ marginLeft: 'auto' }}
      >
        {undoing ? t('common.loading') : t('callDetail.autoBoundUndo')}
      </button>
    </div>
  );
}

/**
 * [V6.5] ErrorScreen — спокойный fail-state. Объясняет что аудио сохранено,
 * показывает 3 retry actions и diagnostics block. Бывший small card с одной
 * кнопкой заменён на полный layout per handoff design.
 *
 * provider hint извлекается из call.provider (последний фактически
 * использованный STT). Кнопка «попробовать через другого провайдера»
 * показывается только если provider не пустой — иначе оставляем generic.
 */
function ErrorScreen({
  call,
  reprocessing,
  onRetry,
}: {
  call: Call;
  reprocessing: boolean;
  onRetry: () => void;
}) {
  const { t } = useI18n();
  const reason = call.failed_reason?.trim() || t('callDetail.failBadge');
  const provider = call.provider?.trim() || null;
  const alternativeProvider = provider
    ? provider === 'soniox'
      ? 'gladia'
      : provider === 'gladia'
        ? 'soniox'
        : null
    : null;
  return (
    <div className="card" style={{ marginBottom: 18 }}>
      <CallStateTag state="error" />
      <h2
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 22,
          margin: '12px 0 4px',
          letterSpacing: '-0.01em',
        }}
      >
        {t('callDetail.errorTitle')}
      </h2>
      <p
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 15,
          margin: '0 0 12px',
          color: 'var(--text)',
        }}
      >
        {reason}
      </p>
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          fontSize: 14,
          margin: '0 0 16px',
        }}
      >
        {t('callDetail.errorAudioSaved')}
      </p>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        <button
          type="button"
          className="btn btn--primary btn--sm"
          onClick={onRetry}
          disabled={reprocessing}
        >
          {reprocessing ? t('callDetail.retrying') : t('callDetail.errorRetry')}
        </button>
        {alternativeProvider && (
          <button
            type="button"
            className="btn btn--quiet btn--sm"
            onClick={onRetry}
            disabled={reprocessing}
            title={t('callDetail.errorRetryProvider', { provider: alternativeProvider })}
          >
            {t('callDetail.errorRetryProvider', { provider: alternativeProvider })}
          </button>
        )}
      </div>
      <ErrorDiagnostics call={call} />
    </div>
  );
}

function ErrorDiagnostics({ call }: { call: Call }) {
  const { t } = useI18n();
  return (
    <details style={{ marginTop: 18 }}>
      <summary
        className="small-caps"
        style={{ cursor: 'pointer', color: 'var(--text-muted)' }}
      >
        {t('callDetail.errorDiagnosticsTitle')}
      </summary>
      <dl
        style={{
          display: 'grid',
          gridTemplateColumns: '160px 1fr',
          gap: '6px 16px',
          marginTop: 10,
          fontFamily: 'var(--font-mono)',
          fontSize: 12,
        }}
      >
        <dt className="muted">{t('callDetail.errorDiagnosticsCode')}</dt>
        <dd style={{ margin: 0 }}>PIPELINE_FAIL</dd>
        {call.provider && (
          <>
            <dt className="muted">
              {t('callDetail.errorDiagnosticsProvider')}
            </dt>
            <dd style={{ margin: 0 }}>{call.provider}</dd>
          </>
        )}
        <dt className="muted">
          {t('callDetail.errorDiagnosticsLastAt')}
        </dt>
        <dd style={{ margin: 0 }}>{call.updated_at}</dd>
        <dt className="muted">
          {t('callDetail.errorDiagnosticsQuota')}
        </dt>
        <dd style={{ margin: 0 }}>—</dd>
      </dl>
    </details>
  );
}

/**
 * [V8] ReprocessBanner — компактный overlay над уже видимым контентом
 * звонка. Юзер видит что reprocess идёт, но **старые** recap/transcript
 * остаются в табах под баннером. Cancel кнопка → backend abort'ает
 * pipeline task + restore статуса на 'ready'.
 *
 * Отличие от ProcessingPanel: без ghost-rows (контент уже есть) и с
 * Cancel кнопкой (первичная обработка не отменяется до 'ready' — нечего
 * восстанавливать).
 */
function ReprocessBanner({
  call,
  onCancel,
}: {
  call: Call;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const step = (Math.min(
    Math.max(call.pipeline_step ?? 1, 1),
    PIPELINE_STEP_KEYS.length,
  ) as CallProgress['step']);
  const stageKey =
    PIPELINE_STEP_KEYS[step - 1] ?? PIPELINE_STEP_KEYS[0];
  const progress: CallProgress = {
    step,
    pct: 0,
    stageLabel: t(stageKey!),
    etaSec: call.pipeline_eta_sec ?? undefined,
  };
  return (
    <div style={{ marginBottom: 18 }}>
      <PipelineStrip progress={progress} />
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 12,
          marginTop: 10,
          fontFamily: 'var(--font-serif)',
          fontStyle: 'italic',
          fontSize: 13,
          color: 'var(--text-muted)',
        }}
      >
        <span style={{ flex: 1, minWidth: 0 }}>
          {t('callDetail.reprocessRunning')}
        </span>
        <button
          type="button"
          className="btn btn--quiet btn--sm"
          onClick={onCancel}
          data-comment-anchor="reprocess-cancel"
        >
          {t('callDetail.reprocessCancel')}
        </button>
      </div>
    </div>
  );
}

/**
 * [V6.4] ProcessingPanel — рендерит PipelineStrip + reassurance card +
 * ghost-rows mockup транскрипта. Юзер видит «что-то происходит» вместо
 * пустого экрана. DB-state восстанавливается на reload, событие
 * `call:progress` обновляет live tick без F5.
 */
function ProcessingPanel({ call }: { call: Call }) {
  const { t } = useI18n();
  // Step может быть NULL до первого emit_progress — показываем step=1 (upload).
  const step = (Math.min(
    Math.max(call.pipeline_step ?? 1, 1),
    PIPELINE_STEP_KEYS.length,
  ) as CallProgress['step']);
  const pct = Math.max(0, Math.min(100, call.pipeline_pct ?? 0));
  const eta = call.pipeline_eta_sec ?? undefined;
  const stageKey =
    PIPELINE_STEP_KEYS[step - 1] ?? PIPELINE_STEP_KEYS[0];
  const progress: CallProgress = {
    step,
    pct,
    stageLabel: t(stageKey),
    etaSec: eta,
  };
  return (
    <div style={{ marginBottom: 18 }}>
      <PipelineStrip progress={progress} defaultOpen />
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-serif)',
          fontSize: 14,
          fontStyle: 'italic',
          marginTop: 14,
          marginBottom: 0,
        }}
      >
        {t('callDetail.reassureCanClose')}
      </p>
      <div className="transcript" style={{ marginTop: 18 }}>
        {/* Ghost-rows — намёк что транскрипт скоро появится. Без дёрганий
            при загрузке (skeletons mounted один раз, до получения transcript). */}
        {[0, 1, 2].map((i) => (
          <div key={i} className="transcript-row transcript-row--ghost">
            <div className="transcript-speaker" aria-hidden="true">···</div>
            <div className="transcript-text" aria-hidden="true">···</div>
            <div className="transcript-time" aria-hidden="true">···</div>
          </div>
        ))}
      </div>
    </div>
  );
}

function TasksPanel({ tasks, contacts }: { tasks: ActionItem[]; contacts: Contact[] }) {
  const { t } = useI18n();
  if (tasks.length === 0) {
    return <Empty description={t('callDetail.emptyTasks')} />;
  }
  const nameById = new Map(contacts.map((c) => [c.id, c.display_name]));
  return (
    <ol style={{ listStyle: 'none', padding: 0, margin: 0 }}>
      {tasks.map((task, i) => {
        const owner = task.owner_contact_id ? nameById.get(task.owner_contact_id) : null;
        return (
          <li
            key={task.id}
            style={{
              display: 'grid',
              gridTemplateColumns: '24px 1fr auto',
              gap: 14,
              padding: '12px 0',
              borderTop: i === 0 ? 'none' : '1px solid var(--line-soft)',
              alignItems: 'baseline',
              color: task.done ? 'var(--muted)' : 'var(--ink)',
              textDecoration: task.done ? 'line-through' : 'none',
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
              {task.text}
              {owner && (
                <span className="muted" style={{ fontSize: 13, marginLeft: 8 }}>
                  — {owner}
                </span>
              )}
              {task.due && (
                <span className="muted" style={{ fontSize: 13, marginLeft: 8 }}>
                  {t('callDetail.taskDueShort', { date: task.due })}
                </span>
              )}
            </span>
            <span
              className="small-caps"
              style={{
                fontSize: 10,
                color: task.done ? 'var(--success)' : 'var(--muted)',
              }}
            >
              {task.done ? t('callDetail.taskStatusDone') : t('callDetail.taskStatusOpen')}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

type TFn = ReturnType<typeof useI18n>['t'];

function tabLabel(tab: Tab, t: TFn): string {
  switch (tab) {
    case 'recap':
      return t('callDetail.tabRecap');
    case 'transcript':
      return t('callDetail.tabTranscript');
    case 'tasks':
      return t('callDetail.tabTasks');
    case 'speakers':
      return t('callDetail.tabSpeakers');
  }
}

// Action overflow menu — kebab top-right с reprocess + regenerate-recap
// + export + delete.
function HeaderActions({
  onReprocess,
  onRegenerateRecap,
  onExport,
  onDelete,
  reprocessing,
  regenerating,
  regenerateDisabled,
  exporting,
  deleting,
}: {
  onReprocess: () => void;
  onRegenerateRecap: () => void;
  onExport: () => void;
  onDelete: () => void;
  reprocessing: boolean;
  regenerating: boolean;
  regenerateDisabled: boolean;
  exporting: boolean;
  deleting: boolean;
}) {
  const { t } = useI18n();
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
        aria-label={t('callDetail.actionsAria')}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={reprocessing || deleting || exporting || regenerating}
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
        title={t('callDetail.actionsTitle')}
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
            disabled={reprocessing || deleting || exporting || regenerating}
          >
            {reprocessing ? t('callDetail.reprocessing') : t('callDetail.reprocess')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onRegenerateRecap();
            }}
            disabled={regenerating || regenerateDisabled || reprocessing || deleting || exporting}
            title={regenerateDisabled ? t('callDetail.regenerateNoTranscript') : undefined}
          >
            {regenerating ? t('callDetail.regenerating') : t('callDetail.regenerateRecap')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onExport();
            }}
            disabled={exporting || reprocessing || deleting || regenerating}
          >
            {exporting ? t('callDetail.exporting') : t('callDetail.exportMd')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onDelete();
            }}
            disabled={deleting || reprocessing || exporting}
            danger
          >
            {deleting ? t('common.deleting') : t('common.delete')}
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
  title,
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      disabled={disabled}
      title={title}
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
// [V5.2] Dedupe по contact_id — STT может одного человека разбить на S1+S2,
// показываем только уникальных людей.
function ParticipantsRow({ speakers }: { speakers: CallSpeakerView[] }) {
  const { t, locale } = useI18n();
  const namedAll = speakers.filter((s) => s.confirmed && s.contact_display_name);
  // Уникальные по contact_id (если есть; иначе fallback на speaker.id).
  const seen = new Set<string>();
  const named: CallSpeakerView[] = [];
  for (const s of namedAll) {
    const key = s.contact_id ?? `__sp_${s.id}`;
    if (!seen.has(key)) {
      seen.add(key);
      named.push(s);
    }
  }
  if (named.length === 0) return null;
  const SP = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];
  const initials = (name: string) =>
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('');
  const declN =
    locale === 'ru'
      ? pluralParticipants(named.length)
      : named.length === 1
        ? t('participants.one')
        : t('participants.many');
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

