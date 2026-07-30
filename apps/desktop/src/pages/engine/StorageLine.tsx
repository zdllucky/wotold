// Настройки → Обработка: место на диске.
//
// [design-gate] Surface: pages/engine/StorageLine
// Reference: docs/design/wotold-v2/_reference/wk-settings.jsx (SecEngine footer)
// Tokens: --text-2, --text-3, --s*, --t-*
// Classes: .set-hint, .mono, .btn (через Button), .overlay/.modal (ConfirmModal)
// New tokens: нет
// A11y: подтверждение — role="alertdialog" с focus-trap внутри ConfirmModal;
//   кнопка получает aria-disabled через disabled, состояние озвучено текстом.
//
// Вместо таблицы моделей — одна строка и одна кнопка. Таблица показывала
// двенадцать строк с датами последнего использования и крестиками у каждой;
// по ней принимались решения, которые сводятся к одному: «удалить модели
// размеров, которыми я не пользуюсь». Авто-удаления при смене размера нет
// намеренно (R12-bis) — гигабайты удаляет только явное действие.

import { useState } from 'react';

import { ConfirmModal } from '../../components/ConfirmModal';
import { Button, Icon } from '../../ui';
import { useI18n } from '../../i18n';
import { formatBytes } from './formatBytes';

interface StorageLineProps {
  usedBytes: number;
  reclaimableBytes: number;
  onFreeSpace: () => Promise<number>;
}

export function StorageLine({ usedBytes, reclaimableBytes, onFreeSpace }: StorageLineProps) {
  const { t } = useI18n();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [freed, setFreed] = useState<number | null>(null);

  const canFree = reclaimableBytes > 0;

  return (
    <div style={{ marginTop: 18, display: 'flex', alignItems: 'center', gap: 12 }}>
      <p className="set-hint" style={{ margin: 0 }}>
        {t('localEngine.storageUsed', { size: formatBytes(usedBytes) })}
        {freed != null && ` · ${t('localEngine.freeSpaceDone', { size: formatBytes(freed) })}`}
      </p>
      {canFree && (
        <Button
          variant="ghost"
          size="sm"
          style={{ marginLeft: 'auto' }}
          leading={<Icon name="trash" size={13} />}
          onClick={() => setConfirmOpen(true)}
        >
          {t('localEngine.freeSpaceCta', { size: formatBytes(reclaimableBytes) })}
        </Button>
      )}

      <ConfirmModal
        open={confirmOpen}
        title={t('localEngine.freeSpaceConfirmTitle')}
        body={t('localEngine.freeSpaceConfirmBody', { size: formatBytes(reclaimableBytes) })}
        confirmLabel={t('localEngine.freeSpaceConfirm')}
        cancelLabel={t('common.cancel')}
        danger
        busy={busy}
        onConfirm={async () => {
          setBusy(true);
          try {
            setFreed(await onFreeSpace());
          } finally {
            setBusy(false);
            setConfirmOpen(false);
          }
        }}
        onCancel={() => setConfirmOpen(false)}
      />
    </div>
  );
}
