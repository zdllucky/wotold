// [B18.5c] Reusable confirm dialog on uikit .overlay/.modal + focus-trap hook.
//
// Mirrors the ARIA/focus pattern of DeleteModelConfirm but uses the v2 uikit
// .overlay/.modal classes (NOT legacy .modal-backdrop/.index-card). No raw
// colors — variants come from .btn--danger / .btn--primary.

import { useRef, type ReactNode } from 'react';
import { useFocusTrap } from '../hooks/useFocusTrap';

interface ConfirmModalProps {
  open: boolean;
  title: string;
  body: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}

const titleId = 'confirm-modal-title';

export function ConfirmModal({
  open,
  title,
  body,
  confirmLabel,
  cancelLabel,
  danger,
  busy,
  onConfirm,
  onCancel,
}: ConfirmModalProps) {
  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, open, { onClose: onCancel });

  if (!open) return null;

  return (
    <div className="overlay" onMouseDown={onCancel}>
      <div
        ref={ref}
        className="modal"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <div className="modal-title" id={titleId}>
            {title}
          </div>
        </div>
        <div className="modal-body">{body}</div>
        <div className="modal-foot">
          <button
            type="button"
            className="btn btn--ghost"
            data-size="sm"
            onClick={onCancel}
            disabled={busy}
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            className={danger ? 'btn btn--danger' : 'btn btn--primary'}
            data-size="sm"
            onClick={() => void onConfirm()}
            disabled={busy}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
