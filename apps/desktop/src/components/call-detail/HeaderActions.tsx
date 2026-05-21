// Action overflow menu — kebab top-right с reprocess + regenerate-recap
// + export + delete. Click outside / Escape закрывает overlay. Все четыре
// action'а disabled пока любой из них выполняется (предотвращает race'ы).

import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../../i18n';
import { MenuItem } from './MenuItem';

interface HeaderActionsProps {
  onReprocess: () => void;
  onRegenerateRecap: () => void;
  onExport: () => void;
  onDelete: () => void;
  reprocessing: boolean;
  regenerating: boolean;
  regenerateDisabled: boolean;
  exporting: boolean;
  deleting: boolean;
}

export function HeaderActions({
  onReprocess,
  onRegenerateRecap,
  onExport,
  onDelete,
  reprocessing,
  regenerating,
  regenerateDisabled,
  exporting,
  deleting,
}: HeaderActionsProps) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!containerRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', handler);
    return () => window.removeEventListener('mousedown', handler);
  }, [open]);

  return (
    <div
      ref={containerRef}
      style={{
        position: 'absolute',
        top: 0,
        right: 0,
      }}
    >
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-label={t('callDetail.actionsAria')}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={reprocessing || deleting || exporting || regenerating}
        style={{
          width: 32,
          height: 32,
          borderRadius: 'var(--radius-sm)',
          border: 'none',
          background: open ? 'var(--bg-2)' : 'transparent',
          color: 'var(--muted)',
          cursor: 'pointer',
          fontSize: 18,
          lineHeight: 1,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
        }}
        title={t('callDetail.actionsTitle')}
      >
        ⋯
      </button>
      {open && (
        <div
          role="menu"
          style={{
            position: 'absolute',
            top: 'calc(100% + 6px)',
            right: 0,
            zIndex: 30,
            background: 'var(--paper)',
            border: '1px solid var(--line)',
            borderRadius: 'var(--radius-md)',
            boxShadow: 'var(--shadow-2)',
            padding: 4,
            minWidth: 180,
          }}
        >
          <MenuItem
            onClick={() => {
              setOpen(false);
              onReprocess();
            }}
            disabled={reprocessing || deleting || exporting || regenerating}
          >
            {reprocessing ? t('callDetail.reprocessing') : t('callDetail.reprocess')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onRegenerateRecap();
            }}
            disabled={regenerating || regenerateDisabled || reprocessing || deleting || exporting}
            title={regenerateDisabled ? t('callDetail.regenerateNoTranscript') : undefined}
          >
            {regenerating ? t('callDetail.regenerating') : t('callDetail.regenerateRecap')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onExport();
            }}
            disabled={exporting || reprocessing || deleting || regenerating}
          >
            {exporting ? t('callDetail.exporting') : t('callDetail.exportMd')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onDelete();
            }}
            disabled={deleting || reprocessing || exporting}
            danger
          >
            {deleting ? t('common.deleting') : t('common.delete')}
          </MenuItem>
        </div>
      )}
    </div>
  );
}
