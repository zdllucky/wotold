// [TD-49] Действия страницы звонка: переобработка, отмена, регенерации,
// экспорт, удаление.
//
// Выделено из `CallDetailPage.tsx` (844 строки при лимите 800, правило 8) по
// тому же образцу, что `useInboxRowActions`. Граница естественная: это
// команды над звонком со своими busy-флагами, разметка о них знает только
// через возвращаемые обработчики. Логика перенесена символ-в-символ.

import { useState } from 'react';
import { ask, save } from '@tauri-apps/plugin-dialog';

import { humanError } from '../api/errors';
import {
  cancelReprocess,
  deleteCall,
  exportCallMarkdown,
  regenerateRecap,
  regenerateTitle,
  reprocessCall,
} from '../api/calls';
import { unbindCallSpeaker } from '../api/speakers';
import type { Call } from '../api/recording';
import { useI18n } from '../i18n';
import { useToast } from '../ui';

interface CallDetailActionsDeps {
  callId: string;
  call: Call | null;
  setCall: (updater: Call | ((prev: Call | null) => Call | null)) => void;
  refetchAll: () => Promise<void>;
  refetchSpeakersAndContacts: () => Promise<void>;
  setBgBusy: (v: boolean) => void;
  setRecapElapsedSec: (v: number | null) => void;
  setPendingRecapRegen: (v: boolean) => void;
  onBack: () => void;
}

export function useCallDetailActions({
  callId,
  call,
  setCall,
  refetchAll,
  refetchSpeakersAndContacts,
  setBgBusy,
  setRecapElapsedSec,
  setPendingRecapRegen,
  onBack,
}: CallDetailActionsDeps) {
  const { t } = useI18n();
  const toast = useToast();
  const [deleting, setDeleting] = useState(false);
  const [reprocessing, setReprocessing] = useState(false);
  const [exporting, setExporting] = useState(false);

  // [TD-24] Сбой несмертельного действия (экспорт, регенерация, отвязка) —
  // в тост, а не в общий error-state.
  const actionError = (e: unknown) =>
    toast.show({ message: humanError(e, t), tone: 'danger' });

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

  return {
    deleting,
    reprocessing,
    exporting,
    actionError,
    onReprocess,
    onCancelReprocess,
    onUnbindVoice,
    onRegenerateRecap,
    onRegenerateTitle,
    onExportMarkdown,
    onDelete,
  };
}
