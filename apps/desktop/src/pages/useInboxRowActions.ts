// [B20.4/B20.5] Row-menu actions инбокса — вынесены из InboxView (800-line
// guard). Зеркалят kebab на CallDetailPage: reprocess/export/delete. Используются
// и таблицей (kebab + ПКМ), и календарными видами (ПКМ context-menu).

import { ask, save } from '@tauri-apps/plugin-dialog';
import type { Call } from '../api/recording';
import { deleteCall, exportCallMarkdown, reprocessCall } from '../api/calls';
import { humanError } from '../api/errors';
import type { useI18n } from '../i18n';
import type { useToast } from '../ui';

type TFn = ReturnType<typeof useI18n>['t'];
type Toast = ReturnType<typeof useToast>;

interface RowActionDeps {
  t: TFn;
  toast: Toast;
  refresh: () => void;
  markActive: (callId: string) => void;
}

export interface InboxRowActions {
  onRowReprocess: (call: Call) => void;
  onRowExport: (call: Call) => void;
  onRowDelete: (call: Call) => void;
}

export function useInboxRowActions({ t, toast, refresh, markActive }: RowActionDeps): InboxRowActions {
  const onRowReprocess = (call: Call) => {
    void (async () => {
      try {
        await reprocessCall(call.id);
        markActive(call.id);
        toast.show({ tone: 'success', message: t('inbox.reprocessStarted') });
      } catch (e) {
        toast.show({ tone: 'danger', message: humanError(e, t) });
      }
    })();
  };

  const onRowExport = (call: Call) => {
    void (async () => {
      const base = call.title?.trim() || `wotold-${call.id.slice(0, 8)}`;
      const defaultPath = `${base.replace(/[^\p{L}\p{N}_.-]/gu, '_')}.md`;
      let dest: string | null = null;
      try {
        dest = (await save({
          defaultPath,
          filters: [{ name: 'Markdown', extensions: ['md'] }],
          title: t('callDetail.exportTitle'),
        })) as string | null;
      } catch (e) {
        toast.show({ tone: 'danger', message: humanError(e, t) });
        return;
      }
      if (!dest) return; // cancelled
      try {
        await exportCallMarkdown(call.id, dest);
        toast.show({ tone: 'success', message: t('inbox.exported') });
      } catch (e) {
        toast.show({ tone: 'danger', message: humanError(e, t) });
      }
    })();
  };

  const onRowDelete = (call: Call) => {
    void (async () => {
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
      try {
        await deleteCall(call.id);
        refresh();
        toast.show({ tone: 'success', message: t('inbox.deleted') });
      } catch (e) {
        toast.show({ tone: 'danger', message: humanError(e, t) });
      }
    })();
  };

  return { onRowReprocess, onRowExport, onRowDelete };
}
