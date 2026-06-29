// [B18.6c] Thin wrapper over the v2 ConfirmModal (.overlay/.modal + focus-trap).
// Always-open here — the caller renders it conditionally. Focus-trap and the
// danger button variant are handled by ConfirmModal; this file only assembles
// the localized body.

import { useI18n } from '../i18n';
import { ConfirmModal } from './ConfirmModal';

interface DeleteModelConfirmProps {
  modelRole: string;
  currentPreset: string;
  fallbackPreset: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function DeleteModelConfirm({
  modelRole,
  currentPreset,
  fallbackPreset,
  onConfirm,
  onCancel,
}: DeleteModelConfirmProps) {
  const { t } = useI18n();

  const bodyText = t('localEngine.storageConfirm.body').replace('{fallback}', fallbackPreset);

  const body = (
    <>
      <div
        style={{
          fontFamily: 'var(--mono)',
          fontSize: 10,
          textTransform: 'uppercase',
          letterSpacing: '0.1em',
          color: 'var(--text-3)',
          marginBottom: 6,
        }}
      >
        {modelRole}
      </div>
      <div>{bodyText}</div>
      <div style={{ color: 'var(--text-3)', fontSize: 12, marginTop: 4 }}>{currentPreset}</div>
    </>
  );

  return (
    <ConfirmModal
      open
      danger
      title={t('localEngine.storageConfirm.title')}
      body={body}
      confirmLabel={t('localEngine.storageConfirm.confirm')}
      cancelLabel={t('localEngine.storageConfirm.cancel')}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}
