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
import { retryChunk } from '../api/recording';
import { humanError } from '../api/errors';
import { engineLabelHuman } from '../utils/engineLabel';
import { Dropdown, Icon, IconBtn, MenuItem, MenuSep, Tabs } from '../ui';
import {
  AutoBoundBanner,
  CallDetailSkeleton,
  CallTypeBadge,
  DecisionsBlock,
  ErrorScreen,
  LegacyRecapBanner,
  MdPanel,
  OpenQuestionsBlock,
  PrivacyDisclaimer,
  ProcessingPanel,
  RecapRegenSuggestionStrip,
  ReprocessBanner,
  TasksPanel,
} from '../components/call-detail';
import { CallRail } from '../components/call-detail/CallRail';
import { AudioScrubber } from '../components/AudioScrubber';
import { InteractiveTranscript } from '../components/InteractiveTranscript';
import { SpeakerConfirmModal } from '../components/SpeakerConfirmModal';
import { EngineChip } from '../components/EngineChip';
import { CallStateTag, ProgressRail } from '../components/call-state';
import { useCallAudio } from '../hooks/useCallAudio';
import { useCallDetail } from '../hooks/useCallDetail';
import { useI18n } from '../i18n';
import { extractSamples } from './SpeakersSection';
import {
  findSpeakerAtTime,
  formatHeaderMeta,
  hashCallId,
  simpleDateTitle,
} from '../utils/callMeta';

// [B18.3a] v2 IA: 2 tabs. Tasks fold into Recap; speakers move to CallRail.
type Tab = 'recap' | 'transcript';

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
    bgBusy,
    setBgBusy,
    justGenerated,
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
  const [reprocessing, setReprocessing] = useState(false);
  const [exporting, setExporting] = useState(false);
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
      // [P16.1 review] Functional updater — capture latest state inside
      // updater, не stale closure. Между snapshot capture и catch
      // `call:progress` Tauri event мог обновить state — revert не должен
      // discard concurrent updates. Берём только поля которые мы patched:
      // status + pipeline_* — остальное (`prev` actual) keeps.
      setCall((prev) =>
        prev
          ? {
              ...prev,
              status: snapshotBefore.status,
              pipeline_step: snapshotBefore.pipeline_step,
              pipeline_pct: snapshotBefore.pipeline_pct,
              pipeline_eta_sec: snapshotBefore.pipeline_eta_sec,
              upload_bytes: snapshotBefore.upload_bytes,
              recap_failed_reason: snapshotBefore.recap_failed_reason,
            }
          : prev,
      );
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

  // [Global regen] Fire-and-forget: команда regenerate_recap регистрирует
  // фон-задачу в pipeline_tasks и возвращается сразу. Результат подтянет
  // pipeline:finished listener (refetchAll в useCallDetail) даже после возврата
  // на страницу; busy-флаг (bgBusy) сбросит он же. Задача переживает навигацию
  // и считается в бейдже у «Звонки».
  const onRegenerateRecap = async () => {
    setBgBusy(true);
    setError(null);
    // [P1.3] Сброс elapsed timer'а на старте — UI начинает с «Пересоздаём…».
    setRecapElapsedSec(null);
    // [Bug-fix #6] Регенерация запущена — recap-regen suggestion больше не нужен.
    setPendingRecapRegen(false);
    // Optimistic clear stale recap_failed_reason; snapshot для revert на reject.
    const snapshotBefore = call;
    setCall((prev) => (prev ? { ...prev, recap_failed_reason: null } : prev));
    try {
      await regenerateRecap(callId);
    } catch (e) {
      // Reject = guard «уже обрабатывается» / spawn-ошибка → revert busy + state.
      setError(t('callDetail.regenerateFailed', { error: humanError(e) }));
      // [P16.1 review] Functional updater — restore только patched поле
      // (recap_failed_reason), не stomp concurrent state из `call:progress`.
      // bgBusy-модель (regen = фон-задача): setBgBusy(false) на reject,
      // на успехе bgBusy сбрасывает pipeline:finished listener.
      if (snapshotBefore) {
        setCall((prev) =>
          prev
            ? { ...prev, recap_failed_reason: snapshotBefore.recap_failed_reason }
            : prev,
        );
      }
      setBgBusy(false);
    }
  };

  // [M14 T-17 / Global regen] Title regen — тоже фон-задача. Новый title
  // подтянется через refetchAll на pipeline:finished.
  const onRegenerateTitle = async () => {
    setBgBusy(true);
    setError(null);
    try {
      await regenerateTitle(callId);
    } catch (e) {
      setError(t('callDetail.regenerateTitleFailed', { error: humanError(e) }));
      setBgBusy(false);
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
      <p role="alert" style={{ color: 'var(--danger)', fontFamily: 'var(--font)' }}>
        {error}
      </p>
    );
  if (!call) return <p className="muted">{t('callDetail.notFound')}</p>;

  const title = call.title?.trim() || simpleDateTitle(call);
  const hasFailedChunks = (chunks ?? []).some((c) => c.status === 'failed');

  return (
    // [B18.9] v2 IA: shared `.view-head` breadcrumb bar at the very top
    // (Звонки › <title> + kebab), then the existing two-column body below.
    // `.main` gives the flex column + relative positioning; negative margins
    // pull the bar full-bleed across the padded `.app-main` scroll viewport.
    <div className="main">
      <div className="view-head" style={{ margin: '-34px -44px 0' }}>
        {/* Back to inbox — plain text button, no border/bg (prototype CallView). */}
        <button
          type="button"
          onClick={onBack}
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            color: 'var(--text-3)',
            fontSize: 'var(--t-13)',
            padding: 0,
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.color = 'var(--text)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.color = 'var(--text-3)';
          }}
        >
          {t('common.backAll')}
        </button>
        <Icon name="chevronRight" size={13} style={{ color: 'var(--text-faint)' }} />
        <span className="u-trunc" style={{ fontWeight: 600, maxWidth: 360 }}>
          {title}
        </span>
        <div style={{ flex: 1 }} />
        {/* Action overflow — kebab folding the former HeaderActions menu
            (reprocess / regenerate-recap / regenerate-title / export / delete)
            into a shared Dropdown wired to the same handlers. */}
        <Dropdown
          align="right"
          width={200}
          trigger={({ open, toggle }) => (
            <IconBtn
              icon="dots"
              onClick={toggle}
              label={t('callDetail.actionsAria')}
              hasPopup
              expanded={open}
            />
          )}
        >
          <MenuItem
            icon="refresh"
            disabled={reprocessing || deleting || exporting || bgBusy || hasFailedChunks}
            title={hasFailedChunks ? t('chunkProgress.resumeBlockedHint') : undefined}
            onClick={() => void onReprocess()}
          >
            {reprocessing ? t('callDetail.reprocessing') : t('callDetail.reprocess')}
          </MenuItem>
          <MenuItem
            icon="sparkle"
            disabled={bgBusy || !transcript || reprocessing || deleting || exporting}
            onClick={() => void onRegenerateRecap()}
          >
            {bgBusy
              ? recapElapsedSec !== null
                ? t('callDetail.regeneratingWithElapsed', { sec: recapElapsedSec })
                : t('callDetail.regenerating')
              : t('callDetail.regenerateRecap')}
          </MenuItem>
          <MenuItem
            icon="edit"
            disabled={bgBusy || !transcript || reprocessing || deleting || exporting}
            onClick={() => void onRegenerateTitle()}
          >
            {bgBusy ? t('callDetail.regeneratingTitle') : t('callDetail.regenerateTitle')}
          </MenuItem>
          <MenuItem
            icon="download"
            disabled={exporting || reprocessing || deleting || bgBusy}
            onClick={() => void onExportMarkdown()}
          >
            {exporting ? t('callDetail.exporting') : t('callDetail.exportMd')}
          </MenuItem>
          <MenuSep />
          <MenuItem
            icon="trash"
            danger
            disabled={deleting || reprocessing || exporting}
            onClick={() => void onDelete()}
          >
            {deleting ? t('common.deleting') : t('common.delete')}
          </MenuItem>
        </Dropdown>
      </div>

      {/* [B17 V3.8] flex column + minHeight: 100% — scrubber последний child
          получает marginTop: auto и прижимается к низу .app-main scroll viewport.
          Без этого при коротком контенте (например пустой recap) sticky bottom
          не активируется, scrubber висит в середине экрана.
          [B18.3a] Two-column: doc-column (flex 1, scrubber sticks bottom) + CallRail. */}
      <div style={{ display: 'flex', minHeight: '100%', gap: 0 }}>
        <div
          style={{
            flex: 1,
            minWidth: 0,
            display: 'flex',
            flexDirection: 'column',
            minHeight: '100%',
            paddingRight: 28,
          }}
        >
          {/* Meta — date · engine · type (moved from the removed 36px header into
              a compact line at the top of the body). Human Russian per ref §5:
              ВТОРНИК · 19 МАЯ · 11:24 · 32 МИН 14 СЕК */}
          <div
            className="small-caps"
            style={{
              marginTop: 22,
              marginBottom: 18,
              display: 'flex',
              alignItems: 'center',
              gap: 10,
              flexWrap: 'wrap',
            }}
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

      {/* [Processing status] Фон-regen (саммари/название) не меняет status —
          звонок остаётся 'ready'. Показываем strip с indeterminate rail +
          elapsed, чтобы пользователь видел «идёт обработка» (как глобальная
          задача, переживающая навигацию). Полный pipeline (status='processing')
          выше имеет свой ProcessingPanel/ReprocessBanner. */}
      {bgBusy && call.status === 'ready' && (
        <div
          className="card"
          style={{ marginBottom: 18, display: 'flex', flexDirection: 'column', gap: 10 }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <CallStateTag state="processing" labelOverride={t('callState.busyGeneric')} />
            <span className="muted" style={{ fontSize: 13 }}>
              {recapElapsedSec != null
                ? t('callDetail.bgBusyStripElapsed', { sec: recapElapsedSec })
                : t('callDetail.bgBusyStrip')}
            </span>
          </div>
          <ProgressRail indeterminate ariaLabel={t('callState.busyGeneric')} />
        </div>
      )}

      {call.status === 'failed' && (
        <ErrorScreen
          call={call}
          reprocessing={reprocessing}
          onRetry={() => void onReprocess()}
          hasFailedChunks={hasFailedChunks}
        />
      )}

      {call.recap_failed_reason && call.status !== 'failed' && (
        <div
          className="card"
          style={{
            marginBottom: 18,
            borderColor: 'var(--warn)',
          }}
        >
          <div className="small-caps" style={{ color: 'var(--warn)', marginBottom: 6 }}>
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
              fontFamily: 'var(--font)',
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
            disabled={bgBusy}
          >
            {bgBusy ? t('callDetail.regenerating') : t('callDetail.regenerateRecap')}
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
            busy={bgBusy}
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
            busy={bgBusy}
            onRegenerate={() => void onRegenerateRecap()}
            onDismiss={() => setPendingRecapRegen(false)}
          />
        )}

      <Tabs value={tab} onChange={(v) => setTab(v as Tab)}>
        <Tabs.List>
          {(['transcript', 'recap'] as Tab[]).map((tabId) => (
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
            animate={justGenerated}
            generating={call.status === 'processing' || bgBusy}
            generatingLabel={
              recapElapsedSec != null
                ? `${t('callDetail.generatingRecap')} ${recapElapsedSec}s`
                : t('callDetail.generatingRecap')
            }
            emptyHint={t('callDetail.emptyRecap')}
            onRegenerate={() => void onRegenerateRecap()}
            regenerating={bgBusy}
            regenerateDisabled={!transcript || reprocessing}
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
          {/* [B18.3a] Tasks folded into Recap (was a separate tab). */}
          <TasksPanel
            tasks={tasks ?? []}
            contacts={contacts}
            onJumpToTranscript={(ms) => {
              setTab('transcript');
              audio.seek(ms / 1000);
            }}
          />
        </Tabs.Panel>
        <Tabs.Panel value="transcript">
          <InteractiveTranscript
            rawSttJson={rawStt}
            fallbackMd={transcript}
            speakers={speakersLite}
            currentTime={audio.currentTime}
            reveal={justGenerated}
            generating={call.status === 'processing'}
            onSeek={(s) => {
              audio.seek(s);
              if (!audio.playing && audio.ready) audio.togglePlay();
            }}
            onIdentifySpeaker={(tag) => setConfirmingTag(tag)}
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
      </div>

        <CallRail
          call={call}
          speakers={speakersLite}
          onIdentify={(tag) => setConfirmingTag(tag)}
          onExport={() => void onExportMarkdown()}
          exporting={exporting}
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
      </div>
    </div>
  );
}

type TFn = ReturnType<typeof useI18n>['t'];

function tabLabel(tab: Tab, t: TFn): string {
  switch (tab) {
    case 'recap':
      return t('callDetail.tabRecap');
    case 'transcript':
      return t('callDetail.tabTranscript');
  }
}

