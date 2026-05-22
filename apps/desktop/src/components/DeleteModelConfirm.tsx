import { useRef } from 'react';
import { useI18n } from '../i18n';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { Button } from '../ui';

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
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, true, { onClose: onCancel });

  const body = t('localEngine.storageConfirm.body').replace('{fallback}', fallbackPreset);

  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <div
        ref={ref}
        className="index-card"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="delete-model-confirm-title"
        style={{ maxWidth: 420 }}
        onClick={(e) => e.stopPropagation()}
      >
        <p
          className="muted"
          style={{ fontFamily: 'var(--font-mono)', fontSize: 10, textTransform: 'uppercase', letterSpacing: '0.1em', marginBottom: 6 }}
        >
          {modelRole}
        </p>
        <h2
          id="delete-model-confirm-title"
          style={{ fontFamily: 'var(--font-serif)', fontSize: 18, marginBottom: 8 }}
        >
          {t('localEngine.storageConfirm.title')}
        </h2>
        <p className="muted" style={{ fontSize: 13, lineHeight: 1.5, marginBottom: 4 }}>
          {body}
        </p>
        <p className="muted" style={{ fontSize: 12, marginBottom: 20 }}>
          {currentPreset}
        </p>
        <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t('localEngine.storageConfirm.cancel')}
          </Button>
          <Button
            variant="primary"
            size="sm"
            style={{ background: 'var(--signal)' }}
            onClick={onConfirm}
          >
            {t('localEngine.storageConfirm.confirm')}
          </Button>
        </div>
      </div>
    </div>
  );
}
