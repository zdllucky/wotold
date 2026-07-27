// [Phase 5 R7] Thin orchestrator after sub-component + state-hook extraction.
//
// Owns local UI state (tab, confirmingTag, busy flags) и Tauri action
// handlers — все мутирующие операции (delete / reprocess / regenerate-recap
// / export). Состояние данных + listener'ы живут в useCallDetail.
//
// Sub-components extracted to ./components/call-detail/* (see barrel).

import { useEffect, useMemo, useRef, useState } from 'react';
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
import { unbindCallSpeaker } from '../api/speakers';
import { useQueueState } from '../hooks/useQueueState';
import { localEngineEvalRecap } from '../api/local-engine';
import { humanError } from '../api/errors';
import { Dropdown, Icon, IconBtn, MenuItem, MenuSep, Tabs, useToast } from '../ui';
import {
  AutoBoundBanner,
  CallDetailSkeleton,
  CallTypeBadge,
  ErrorScreen,
  LegacyRecapBanner,
  PrivacyDisclaimer,
  ProcessingPanel,
  RecapRegenSuggestionStrip,
  RecapView,
  ReprocessBanner,
} from '../components/call-detail';
import { CallRail } from '../components/call-detail/CallRail';
import { AskThread } from '../components/assistant/AskThread';
import { AssistantComposer } from '../components/assistant/AssistantComposer';
import { useCallAssistant } from '../hooks/useCallAssistant';
import { AudioScrubber } from '../components/AudioScrubber';
import {
  InteractiveTranscript,
  type InteractiveTranscriptHandle,
} from '../components/InteractiveTranscript';
import { SpeakerConfirmModal } from '../components/SpeakerConfirmModal';
import { CallStateTag, ProgressRail } from '../components/call-state';
import { useCallAudio } from '../hooks/useCallAudio';
import { useCallDetail } from '../hooks/useCallDetail';
import { bcp47, useI18n } from '../i18n';
import { extractSamples } from './SpeakersSection';
import { formatDur } from './CallDetailUtils';
import { hashCallId, simpleDateTitle } from '../utils/callMeta';

// [B18.3a] v2 IA: 2 tabs. Tasks fold into Recap; speakers move to CallRail.
// [B24.5] + вкладка «Ассистент» (только при status='ready', SPEC §3).
type Tab = 'recap' | 'transcript' | 'assistant';

interface CallDetailPageProps {
  callId: string;
  onBack: () => void;
  /** [B24.5] Переход к другому звонку из источника ответа ассистента. */
  onOpenCall?: (callId: string) => void;
  /** [B24.5] Эскалация «Искать во всех звонках» → раздел «Ассистент». */
  onAskGlobal?: (question: string) => void;
}

export function CallDetailPage({ callId, onBack, onOpenCall, onAskGlobal }: CallDetailPageProps) {
  const { t, locale } = useI18n();
  const {
    call,
    setCall,
    recap,
    transcript,
    rawStt,
    contacts,
    speakers: speakersLite,
    chunks,
    micSrc,
    systemSrc,
    recapElapsedSec,
    setRecapElapsedSec,
    recapSteps,
    bgBusy,
    setBgBusy,
    justGenerated,
    loading,
    error,
    refetchAll,
    refetchSpeakersAndContacts,
  } = useCallDetail(callId);

  // [TD-24] Сбой несмертельного действия (экспорт, регенерация, отвязка) —
  // в тост, а не в общий error-state. Раньше упавший экспорт заменял весь
  // открытый звонок — транскрипт, плеер, рекап — одним красным абзацем без
  // retry и без «назад».
  const toast = useToast();
  const actionError = (e: unknown) =>
    toast.show({ message: humanError(e, t), tone: 'danger' });

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
  // [B24.5] Персистентный тред ассистента этого звонка (SPEC §3).
  const assistantThread = useCallAssistant(callId);
  // [B24.5/ревью] Таб — пер-звонковое состояние: смена звонка сбрасывает на
  // транскрипт (иначе tab='assistant' у не-ready звонка = пустая панель).
  useEffect(() => {
    setTab('transcript');
  }, [callId]);
  useEffect(() => {
    if (tab === 'assistant' && call && call.status !== 'ready') setTab('transcript');
  }, [tab, call]);

  // [B20.8] Follow-режим транскрипта: автоскролл к активной реплике. Ручной
  // скролл (wheel/touch/скроллбар/клавиши) выключает; включает ТОЛЬКО кнопка
  // «к текущему» в плеере. Сбрасывается в on при смене звонка.
  const [follow, setFollow] = useState(true);
  const transcriptRef = useRef<InteractiveTranscriptHandle | null>(null);
  const docScrollRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => setFollow(true), [callId]);
  useEffect(() => {
    const el = docScrollRef.current;
    if (!el) return;
    const off = () => setFollow(false);
    // pointerdown на самом контейнере/пустоте = скроллбар-драг; клики по
    // .turn — это seek, follow не трогаем.
    const onPointerDown = (e: PointerEvent) => {
      if (!(e.target as Element | null)?.closest('.turn')) off();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      const scrollKeys = [
        'PageUp',
        'PageDown',
        'Home',
        'End',
        'ArrowUp',
        'ArrowDown',
        ' ',
      ];
      if (scrollKeys.includes(e.key)) off();
    };
    el.addEventListener('wheel', off, { passive: true });
    el.addEventListener('touchmove', off, { passive: true });
    el.addEventListener('pointerdown', onPointerDown);
    el.addEventListener('keydown', onKeyDown);
    return () => {
      el.removeEventListener('wheel', off);
      el.removeEventListener('touchmove', off);
      el.removeEventListener('pointerdown', onPointerDown);
      el.removeEventListener('keydown', onKeyDown);
    };
    // `loading` в deps обязателен: первый рендер — скелетон, docScrollRef ещё
    // null; листенеры вешаются вторым проходом, когда контейнер смонтирован.
  }, [callId, loading]);

  const onJumpToCurrent = () => {
    setTab('transcript');
    setFollow(true);
    // rAF — дождаться маунта transcript-панели при переключении с рекапа.
    requestAnimationFrame(() => transcriptRef.current?.scrollToActive());
  };

  // [Q] Этот звонок стоит в очереди тяжёлого ресурса? (в waiting и нигде
  // не busy) → ProcessingPanel показывает «в очереди, позиция N».
  const queue = useQueueState();
  const queuedInfo = useMemo(() => {
    if (!queue) return undefined;
    const busySomewhere = queue.resources.some((r) => r.busy?.call_id === callId);
    if (busySomewhere) return undefined;
    for (const r of queue.resources) {
      const idx = r.waiting.findIndex((w) => w.call_id === callId);
      if (idx !== -1) {
        return { resource: r.id, position: idx + 1 };
      }
    }
    return undefined;
  }, [queue, callId]);

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
      toast.show({ message: t('callDetail.reprocessFailed', { error: humanError(e, t) }), tone: 'danger' });
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
  // [B20.7] Отвязать конкретный голос от контакта. Зеркально confirm-flow:
  // после отвязки имя спикера в рекапе устарело → предлагаем regen.
  const onUnbindVoice = async (callSpeakerId: string) => {
    try {
      await unbindCallSpeaker(callSpeakerId);
      await refetchSpeakersAndContacts();
      setPendingRecapRegen(true);
    } catch (e) {
      actionError(e);
    }
  };

  const onRegenerateRecap = async () => {
    setBgBusy(true);
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
      toast.show({ message: t('callDetail.regenerateFailed', { error: humanError(e, t) }), tone: 'danger' });
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
    try {
      await regenerateTitle(callId);
    } catch (e) {
      toast.show({ message: t('callDetail.regenerateTitleFailed', { error: humanError(e, t) }), tone: 'danger' });
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
      actionError(e);
      return;
    }
    if (!dest) return; // cancel
    setExporting(true);
    try {
      await exportCallMarkdown(call.id, dest);
    } catch (e) {
      actionError(e);
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
      actionError(e);
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

  const title = call.title?.trim() || simpleDateTitle(call, t, locale);
  const hasFailedChunks = (chunks ?? []).some((c) => c.status === 'failed');

  return (
    // [B18.9] v2 IA: shared `.view-head` breadcrumb bar at the very top
    // (Звонки › <title> + kebab), then the existing two-column body below.
    // `.main` gives the flex column + relative positioning; negative margins
    // pull the bar full-bleed across the padded `.app-main` scroll viewport.
    <div className="main" style={{ margin: '-34px -44px', height: '100vh' }}>
      <div className="view-head" data-tauri-drag-region="deep">
        {/* Back to inbox — plain text button, no border/bg (prototype CallView). */}
        <button
          type="button"
          onClick={onBack}
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: 3,
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
          <Icon name="chevronLeft" size={14} />
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
          {import.meta.env.DEV && (
            <MenuItem
              icon="sparkle"
              onClick={() => {
                void localEngineEvalRecap(call.id)
                  .then((s) =>
                    window.alert(
                      `g-eval avg ${s.average.toFixed(2)}\ncoherence ${s.coherence} · faithfulness ${s.faithfulness} · relevance ${s.relevance} · conciseness ${s.conciseness}\n\n${s.justification}`,
                    ),
                  )
                  .catch(actionError);
              }}
            >
              g-eval (dev)
            </MenuItem>
          )}
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

      {/* [call-detail] Wotold v2 body (прототип CallView): .view-body (flex row)
          → doc-колонка (.content.doc-wrap > .doc-scroll > .doc, max-width 720 по
          центру, собственный скролл) с плеером .player-dock у низа + CallRail. */}
      <div className="view-body">
        <div className="content doc-wrap">
          <div className="doc-scroll scroll" ref={docScrollRef}>
            <div className="doc" style={{ paddingBottom: 104 }}>
              <h1 className="doc-title">{title}</h1>
              {/* Meta-чипы — время · длительность · движок · тип (прототип CallView). */}
              <div
                style={{
                  display: 'flex',
                  flexWrap: 'wrap',
                  gap: 6,
                  alignItems: 'center',
                  margin: '12px 0 4px',
                }}
              >
                <span className="chip">
                  <Icon name="clock" size={11} />
                  {fmtClock(call.started_at, locale)}
                </span>
                <span className="chip">
                  <Icon name="waveform" size={11} />
                  {formatDur(call.duration_sec ?? 0)}
                </span>
                {/* [B20.10] EngineChip убран из шапки — движок виден только в
                    Настройках/онбординге, юзеру в звонке это знать незачем. */}
                {/* [M14 T-11] Тип звонка (sales/standup/1:1/...). */}
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
            queued={queuedInfo}
            onRetryChunk={(idx) => {
              // [Tech-debt P0.2] retry_chunk fire-and-forget — status update
              // придёт через transcript:chunk_done event, ChunkProgressStrip
              // отжмёт "Повторяем…" автоматически.
              void retryChunk(call.id, idx).catch(actionError);
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
            {humanError(call.recap_failed_reason, t)}
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
          {(
            [
              'transcript',
              'recap',
              // [B24.5] Ассистент доступен только по готовому звонку (SPEC §3).
              ...(call.status === 'ready' ? (['assistant'] as Tab[]) : []),
            ] as Tab[]
          ).map((tabId) => (
            <Tabs.Trigger key={tabId} value={tabId}>
              {tabLabel(tabId, t)}
            </Tabs.Trigger>
          ))}
        </Tabs.List>

        <Tabs.Panel value="recap">
          {/* [M14 T-11] PrivacyDisclaimer для one_on_one — undismissable
              напоминание о приватности перед content (privacy-first). */}
          {call.call_type === 'one_on_one' && <PrivacyDisclaimer />}
          {/* [call-detail] Recap = Wotold v2 макет: rich/markdown toggle + copy.
              Структурные блоки (decisions/open-questions/tasks/evidence) сведены
              к markdown-документу по решению редизайна — данные продолжают
              извлекаться, но в этом табе не рендерятся отдельными секциями. */}
          <RecapView
            recap={recap}
            animate={justGenerated}
            generating={call.status === 'processing' || bgBusy}
            steps={recapSteps}
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
                    error: humanError(call.recap_failed_reason, t),
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
            ref={transcriptRef}
            rawSttJson={rawStt}
            fallbackMd={transcript}
            speakers={speakersLite}
            currentTime={audio.currentTime}
            generating={call.status === 'processing'}
            follow={follow}
            onSeek={(s) => {
              audio.seek(s);
              if (!audio.playing && audio.ready) audio.togglePlay();
            }}
          />
        </Tabs.Panel>
        {call.status === 'ready' && (
          <Tabs.Panel value="assistant">
            {/* [B24.5] Тред звонка (SPEC §3): интро при пустом треде; подсказки
                мока намеренно опущены (их банк был мок-специфичен). */}
            <div style={{ marginTop: 18, paddingBottom: 80 }}>
              {assistantThread.messages.length === 0 && !assistantThread.pending && (
                <div style={{ color: 'var(--text-3)', fontSize: 13.5, marginBottom: 14 }}>
                  {t('assistant.callEmptyDesc')}
                </div>
              )}
              <AskThread
                messages={assistantThread.messages}
                pending={assistantThread.pending}
                pendingText={t('assistant.pendingCall')}
                callId={callId}
                onOpenCall={onOpenCall}
                onSeek={(ms) => {
                  setTab('transcript');
                  audio.seek(ms / 1000);
                  if (!audio.playing && audio.ready) audio.togglePlay();
                }}
                onAskGlobal={onAskGlobal}
              />
            </div>
          </Tabs.Panel>
        )}
      </Tabs>
            </div>
          </div>
          {/* [V6.5] Плеер .player-dock пристыкован к низу .doc-wrap, поверх
              .doc-scroll. Включён и для failed: аудио сохранено локально, юзер
              должен иметь возможность послушать даже если транскрипт не получился.
              enabled=false (null) только когда нет ни одной дорожки.
              [B24.5] На вкладке «Ассистент» вместо плеера — композер вопроса. */}
          {tab === 'assistant' && call.status === 'ready' ? (
            <div className="composer-dock">
              <AssistantComposer
                placeholder={t('assistant.composerCall')}
                icon="sparkle"
                disabled={assistantThread.pending}
                onAsk={(q) => void assistantThread.ask(q)}
              />
            </div>
          ) : (
            <AudioScrubber
              audio={audio}
              seed={hashCallId(callId)}
              enabled
              onJump={onJumpToCurrent}
              followActive={follow}
            />
          )}
        </div>

        <CallRail
          call={call}
          speakers={speakersLite}
          onIdentify={(tag) => setConfirmingTag(tag)}
          samplesByTag={samplesByTag}
          onUnbind={(id) => void onUnbindVoice(id)}
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

// Время начала звонка для meta-чипа (HH:MM в локали интерфейса).
function fmtClock(iso: string, locale: string): string {
  try {
    return new Date(iso).toLocaleTimeString(
      bcp47(locale as Parameters<typeof bcp47>[0]),
      { hour: '2-digit', minute: '2-digit' },
    );
  } catch {
    return '';
  }
}

function tabLabel(tab: Tab, t: TFn): string {
  switch (tab) {
    case 'recap':
      return t('callDetail.tabRecap');
    case 'transcript':
      return t('callDetail.tabTranscript');
    case 'assistant':
      return t('assistant.title');
  }
}

