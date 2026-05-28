// [Phase 5 R7] Thin orchestrator after sub-component + state-hook extraction.
//
// Owns local UI state (tab, confirmingTag, busy flags) и Tauri action
// handlers — все мутирующие операции (delete / reprocess / regenerate-recap
// / export). Состояние данных + listener'ы живут в useCallDetail.
//
// Sub-components extracted to ./components/call-detail/* (see barrel).

import { useMemo, useState } from 'react';
import { ask, save } from '@tauri-apps/plugin-dialog';

import {
  cancelReprocess,
  deleteCall,
  exportCallMarkdown,
  regenerateRecap,
  regenerateTitle,
  reprocessCall,
} from '../api/calls';
import { forceRestt, retryChunk } from '../api/recording';
import { humanError } from '../api/errors';
import { engineLabelHuman } from '../utils/engineLabel';
import { Tabs } from '../ui';
import {
  AutoBoundBanner,
  CallDetailSkeleton,
  CallTypeBadge,
  DecisionsBlock,
  ErrorScreen,
  HeaderActions,
  LegacyRecapBanner,
  MdPanel,
  OpenQuestionsBlock,
  ParticipantsRow,
  PrivacyDisclaimer,
  ProcessingPanel,
  RecapRegenSuggestionStrip,
  ReprocessBanner,
  TasksPanel,
} from '../components/call-detail';
import { AudioScrubber } from '../components/AudioScrubber';
import { InteractiveTranscript } from '../components/InteractiveTranscript';
import { SpeakerConfirmModal } from '../components/SpeakerConfirmModal';
import { EngineChip } from '../components/EngineChip';
import { useCallAudio } from '../hooks/useCallAudio';
import { useCallDetail } from '../hooks/useCallDetail';
import { useI18n } from '../i18n';
import { SpeakersSection, extractSamples } from './SpeakersSection';
import {
  findSpeakerAtTime,
  formatHeaderMeta,
  hashCallId,
  simpleDateTitle,
} from '../utils/callMeta';

type Tab = 'recap' | 'transcript' | 'tasks' | 'speakers';

interface CallDetailPageProps {
  callId: string;
  onBack: () => void;
}

export function CallDetailPage({ callId, onBack }: CallDetailPageProps) {
  const { t } = useI18n();
  const {
    call,
    setCall,
    recap,
    transcript,
    rawStt,
    tasks,
    contacts,
    speakers: speakersLite,
    chunks,
    decisions,
    openQuestions,
    micSrc,
    systemSrc,
    recapElapsedSec,
    setRecapElapsedSec,
    loading,
    error,
    setError,
    refetchAll,
    refetchSpeakersAndContacts,
  } = useCallDetail(callId);

  // [B17 V3.9] Default tab → transcript (per artboard §5 reference).
  const [tab, setTab] = useState<Tab>('transcript');
  // [B17 V4.1] Inline-confirm popup из транскрипта — speaker_tag клика.
  const [confirmingTag, setConfirmingTag] = useState<string | null>(null);

  const [deleting, setDeleting] = useState(false);
  const [regenerating, setRegenerating] = useState(false);
  const [regeneratingTitle, setRegeneratingTitle] = useState(false);
  const [reprocessing, setReprocessing] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [forceRestting, setForceRestting] = useState(false);
  // [Bug-fix #6] После bind speaker → contact подсказываем regenerate recap.
  // Memory-only flag — пере-показывается на следующий bind action в звонке.
  const [pendingRecapRegen, setPendingRecapRegen] = useState(false);

  // [B17 V3.2] Single audio source — shared между AudioScrubber и
  // InteractiveTranscript (для highlight current + click-to-seek).
  const audio = useCallAudio(callId, call?.duration_sec ?? 0);

  // [B17 V3.3] Current speaker info — derived from rawStt segments + audio
  // currentTime. Используется в AudioScrubber SpeakerChip.
  const currentSpeaker = useMemo(
    () => findSpeakerAtTime(rawStt, speakersLite, audio.currentTime),
    [rawStt, speakersLite, audio.currentTime],
  );

  // [B17 V4.1] Per-tag sample bubble (text + start/end/src) — для модала
  // и (потенциально) для будущего sample-row inline-feature в транскрипте.
  const samplesByTag = useMemo(
    () => extractSamples(rawStt, micSrc, systemSrc),
    [rawStt, micSrc, systemSrc],
  );

  const confirmingSpeaker = useMemo(
    () =>
      confirmingTag
        ? speakersLite.find((s) => s.speaker_tag === confirmingTag) ?? null
        : null,
    [confirmingTag, speakersLite],
  );

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
    // [P16.1] Snapshot pre-patch state — на sync-error revert immediately
    // вместо waiting на refetchAll → нет fake spinner пока refetch crawl'ит.
    const snapshotBefore = call;
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
      setError(t('callDetail.reprocessFailed', { error: humanError(e) }));
      // [P16.1] Immediate revert optimistic patch — UI status вернётся в
      // failed без задержки. refetchAll ниже подтянет свежий failed_reason
      // (backend P16.2 теперь пишет failed_reason на chunks gate reject).
      setCall(snapshotBefore);
      await refetchAll();
    } finally {
      setReprocessing(false);
    }
  };

  // [P14.1] Force re-STT — destructive. Дропает existing transcripts +
  // recap артефакты и запускает полный re-recognition. Применяется когда
  // existing transcripts содержат hallucinations (pre-P12 filter) либо
  // [FOREIGN] теги — chunk_assembly skip-STT path не применяет filter
  // post-hoc, нужен полный rerun.
  const onForceRestt = async () => {
    if (!call) return;
    // Cloud engine не chunked → backend всё равно refuse'нёт, скрываем UI.
    if (call.processing_via !== 'local') return;
    const ok = await ask(t('callDetail.forceResttConfirmBody'), {
      title: t('callDetail.forceResttConfirmTitle'),
      kind: 'warning',
      okLabel: t('callDetail.forceResttConfirmOk'),
      cancelLabel: t('common.cancel'),
    });
    if (!ok) return;
    setForceRestting(true);
    setError(null);
    // [P16.1] Snapshot pre-patch — revert на sync error.
    const snapshotBefore = call;
    setCall((prev) =>
      prev
        ? {
            ...prev,
            status: 'processing',
            pipeline_step: 1,
            pipeline_pct: 0,
            pipeline_eta_sec: null,
            upload_bytes: null,
            recap_failed_reason: null,
          }
        : prev,
    );
    try {
      await forceRestt(call.id);
    } catch (e) {
      setError(t('callDetail.forceResttFailed', { error: humanError(e) }));
      // [P16.1] Immediate revert чтобы UI не залип в fake processing.
      setCall(snapshotBefore);
      await refetchAll();
    } finally {
      setForceRestting(false);
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
    // [P1.3] Сброс elapsed timer'а на старте — UI начинает с «Пересоздаём…»
    // (без «… 0s») пока первый periodic event не придёт через 15s.
    setRecapElapsedSec(null);
    // [Bug-fix] Optimistic clear stale recap_failed_reason — баннер исчезает
    // сразу при клике, чтобы юзер не видел старую ошибку пока идёт новая
    // попытка. Если новая попытка упадёт, refetchAll вернёт актуальный
    // failed_reason (с свежим engine label).
    // [P16.1] Snapshot pre-patch — revert sync на reject.
    const snapshotBefore = call;
    setCall((prev) => (prev ? { ...prev, recap_failed_reason: null } : prev));
    try {
      await regenerateRecap(callId);
      // [M14 T-15] refetchAll вместо узкого recap+tasks. После legacy v1 →
      // v2 миграции меняются и Call.summary_schema_version (нужно чтобы
      // LegacyRecapBanner исчез), и decisions/open_questions (новые v2-блоки
      // в Рекап табе), и call_type (CallTypeBadge в header).
      await refetchAll();
      // [Bug-fix #6] Регенерация прошла — recap-regen suggestion больше не нужен.
      setPendingRecapRegen(false);
    } catch (e) {
      setError(t('callDetail.regenerateFailed', { error: humanError(e) }));
      // [P16.1] Revert recap_failed_reason clear — backend reject не
      // должен показать UI что прошлая ошибка исчезла.
      if (snapshotBefore) setCall(snapshotBefore);
    } finally {
      setRegenerating(false);
      // [P1.3] Final reset — даже если событие пришло позже completion.
      setRecapElapsedSec(null);
    }
  };

  // [M14 T-17] Lightweight title-only regen. После success — refetchAll чтобы
  // header показал новый title (Call object подтянет updated row).
  const onRegenerateTitle = async () => {
    setRegeneratingTitle(true);
    setError(null);
    try {
      await regenerateTitle(callId);
      await refetchAll();
    } catch (e) {
      setError(t('callDetail.regenerateTitleFailed', { error: humanError(e) }));
    } finally {
      setRegeneratingTitle(false);
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

  if (loading) return <CallDetailSkeleton onBack={onBack} />;
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
        <div
          className="small-caps"
          style={{ marginBottom: 8, display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}
        >
          {formatHeaderMeta(call)}
          {call.processing_via && (
            <EngineChip kind={call.processing_via} variant="header" />
          )}
          {/* [M14 T-11] Тип звонка (sales/standup/1:1/...) — chip справа от engine. */}
          <CallTypeBadge
            callType={call.call_type}
            confidence={call.call_type_confidence}
          />
        </div>

        {/* Title — LLM-generated если есть, иначе простой fallback "Звонок · 20 мая" */}
        <h1
          className="title"
          style={{ fontSize: 36, margin: 0, marginBottom: 14 }}
        >
          {call.title?.trim() || simpleDateTitle(call)}
        </h1>

        {/* Action overflow — kebab menu top-right с reprocess/regenerate-recap/
            regenerate-title (M14 T-17)/export/delete */}
        <HeaderActions
          onReprocess={() => void onReprocess()}
          onRegenerateRecap={() => void onRegenerateRecap()}
          onRegenerateTitle={() => void onRegenerateTitle()}
          onExport={() => void onExportMarkdown()}
          onDelete={onDelete}
          reprocessing={reprocessing}
          regenerating={regenerating}
          regenerateDisabled={!transcript}
          regeneratingTitle={regeneratingTitle}
          regenerateTitleDisabled={!transcript}
          exporting={exporting}
          deleting={deleting}
          recapElapsedSec={recapElapsedSec}
          hasFailedChunks={(chunks ?? []).some((c) => c.status === 'failed')}
          onForceRestt={
            call.processing_via === 'local' ? () => void onForceRestt() : undefined
          }
          forceRestting={forceRestting}
        />

        {/* Participants chips — same row после title */}
        {speakersLite.length > 0 && (
          <ParticipantsRow
            speakers={speakersLite}
            onConfirmAnonymous={(s) => setConfirmingTag(s.speaker_tag)}
          />
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
          <ProcessingPanel
            call={call}
            chunks={chunks}
            onRetryChunk={(idx) => {
              // [Tech-debt P0.2] retry_chunk fire-and-forget — status update
              // придёт через transcript:chunk_done event, ChunkProgressStrip
              // отжмёт "Повторяем…" автоматически.
              void retryChunk(call.id, idx).catch((e) => setError(humanError(e)));
            }}
          />
        ))}

      {call.status === 'failed' && (
        <ErrorScreen
          call={call}
          reprocessing={reprocessing}
          onRetry={() => void onReprocess()}
          hasFailedChunks={(chunks ?? []).some((c) => c.status === 'failed')}
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
            {/* [Bug-fix] Engine label — показывает какой движок обслуживал
                последнюю (упавшую) попытку. Помогает понять stale-cloud-error
                vs свежее local-падение. */}
            {(() => {
              const eng = engineLabelHuman(call.summary_engine, {
                cloud: t('callDetail.engineCloud'),
                localLight: t('callDetail.engineLocalLight'),
                localBalanced: t('callDetail.engineLocalBalanced'),
                localQuality: t('callDetail.engineLocalQuality'),
                localGeneric: t('callDetail.engineLocalGeneric'),
              });
              return eng ? (
                <span className="muted" style={{ marginLeft: 8, fontSize: 11 }}>
                  · {eng}
                </span>
              ) : null;
            })()}
          </div>
          <p
            style={{
              fontFamily: 'var(--font-serif)',
              fontSize: 16,
              margin: '0 0 14px',
            }}
          >
            {/* [Bug-fix] Map raw backend errors → human messages. Cloudflare
                proxy wraps Anthropic 429 как provider_error + raw JSON;
                humanError ловит "upstream error / 502 / Bad Gateway" → "Сервис
                временно занят. Попробуйте ещё раз через минуту". */}
            {humanError(call.recap_failed_reason)}
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

      {/* [M14 T-15] Legacy v1 → v2 upgrade banner. Виден когда summary в DB
          ещё в старом формате (schema_version IN 1/NULL) и есть recap.md,
          и звонок не в processing (одновременных regenerate нет). После
          клика → regenerateRecap → cloud LLM → T-02 persist_summary_v2 →
          pipeline:finished → refetchAll → banner исчезает автоматически. */}
      {(call.summary_schema_version === 1 || call.summary_schema_version === null) &&
        recap !== null &&
        call.status !== 'processing' && (
          <LegacyRecapBanner
            busy={regenerating}
            onUpgrade={() => void onRegenerateRecap()}
          />
        )}

      {/* [Bug-fix #6] Recap-regen suggestion — после bind speaker → contact.
          Виден только когда summary уже в v2 (legacy banner перекрывает иначе),
          recap существует, звонок не в processing, и нет одновременной регенерации. */}
      {pendingRecapRegen &&
        call.summary_schema_version === 2 &&
        recap !== null &&
        call.status !== 'processing' && (
          <RecapRegenSuggestionStrip
            busy={regenerating}
            onRegenerate={() => void onRegenerateRecap()}
            onDismiss={() => setPendingRecapRegen(false)}
          />
        )}

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
          {/* [M14 T-11] PrivacyDisclaimer для one_on_one — undismissable
              напоминание о приватности перед content. */}
          {call.call_type === 'one_on_one' && <PrivacyDisclaimer />}
          {/* [M14 T-11] V2 structured blocks выше markdown — surfaces
              key takeaways первыми (Granola/Fireflies pattern). При пустых
              decisions/openQuestions блоки рендерят null. */}
          <DecisionsBlock
            decisions={decisions}
            onJumpToTranscript={(ms) => {
              setTab('transcript');
              audio.seek(ms / 1000);
            }}
          />
          <OpenQuestionsBlock
            openQuestions={openQuestions}
            onJumpToTranscript={(ms) => {
              setTab('transcript');
              audio.seek(ms / 1000);
            }}
          />
          <MdPanel
            md={recap}
            emptyHint={t('callDetail.emptyRecap')}
            onRegenerate={() => void onRegenerateRecap()}
            regenerating={regenerating}
            regenerateDisabled={!transcript || reprocessing || forceRestting}
            emptyBody={
              call.recap_failed_reason
                ? t('callDetail.recapEmptyFailed', {
                    error: humanError(call.recap_failed_reason),
                  })
                : call.status === 'processing'
                  ? t('callDetail.recapEmptyProcessing')
                  : !transcript
                    ? t('callDetail.recapEmptyNoTranscript')
                    : t('callDetail.recapEmptyIdle')
            }
          />
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
          <TasksPanel
            tasks={tasks ?? []}
            contacts={contacts}
            onJumpToTranscript={(ms) => {
              setTab('transcript');
              audio.seek(ms / 1000);
            }}
          />
        </Tabs.Panel>
        <Tabs.Panel value="speakers">
          <SpeakersSection
            callId={callId}
            onSpeakersChanged={() => {
              void refetchSpeakersAndContacts();
              // [Bug-fix #6] Имена участников могли измениться — предложить regen.
              setPendingRecapRegen(true);
            }}
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
          onConfirmed={() => {
            void refetchSpeakersAndContacts();
            // [Bug-fix #6] Имя спикера изменилось — предложить regen recap.
            setPendingRecapRegen(true);
          }}
        />
      )}
    </section>
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

