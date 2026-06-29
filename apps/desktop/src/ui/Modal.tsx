// [B18.6c] Wotold v2 uikit — modal dialog (.overlay/.modal from wk.css).

import { useId, useRef, type ReactNode } from 'react';
import { useFocusTrap } from '../hooks/useFocusTrap';

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  /** Accessible name when no visible `title` is rendered. */
  ariaLabel?: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: number | string;
}

export function Modal({ open, onClose, title, ariaLabel, children, footer, width }: ModalProps) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  useFocusTrap(ref, open, { onClose });

  if (!open) return null;

  return (
    <div className="overlay fade" onMouseDown={onClose}>
      <div
        ref={ref}
        className="modal fade-up"
        role="dialog"
        aria-modal="true"
        aria-labelledby={title ? titleId : undefined}
        aria-label={title ? undefined : ariaLabel}
        style={width ? { width } : undefined}
        onMouseDown={(e) => e.stopPropagation()}
      >
        {title && (
          <div className="modal-head">
            <div className="modal-title" id={titleId}>
              {title}
            </div>
          </div>
        )}
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}
