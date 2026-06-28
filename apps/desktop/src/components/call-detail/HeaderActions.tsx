// Action overflow menu — kebab top-right с reprocess + regenerate-recap
// + export + delete. Click outside / Escape закрывает overlay. Все четыре
// action'а disabled пока любой из них выполняется (предотвращает race'ы).

import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../../i18n';
import { MenuItem } from './MenuItem';

interface HeaderActionsProps {
  onReprocess: () => void;
  onRegenerateRecap: () => void;
  onRegenerateTitle: () => void;
  onExport: () => void;
  onDelete: () => void;
  reprocessing: boolean;
  regenerating: boolean;
  regenerateDisabled: boolean;
  regeneratingTitle: boolean;
  regenerateTitleDisabled: boolean;
  exporting: boolean;
  deleting: boolean;
  /** [P1.3] Elapsed seconds во время local LLM recap regen — рендерим в
   *  кнопке как «Пересоздаём… {sec}s». `null` пока первый periodic event
   *  не пришёл, либо cloud engine (не emit'ит). */
  recapElapsedSec?: number | null;
  /** [P11.3] Reprocess блокирован пока есть failed chunks — иначе user
   *  re-обрабатывает с потерянным контентом. Tooltip объясняет почему.
   *  Defaults `false` для backward-compat callsites. */
  hasFailedChunks?: boolean;
}

export function HeaderActions({
  onReprocess,
  onRegenerateRecap,
  onRegenerateTitle,
  onExport,
  onDelete,
  reprocessing,
  regenerating,
  regenerateDisabled,
  regeneratingTitle,
  regenerateTitleDisabled,
  exporting,
  deleting,
  recapElapsedSec = null,
  hasFailedChunks = false,
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
        // [P1.3] Allow opening menu во время regenerating/regeneratingTitle —
        // user должен видеть «Пересоздаём… {sec}s» прогресс. Individual
        // MenuItem'ы остаются disabled, double-click prevention intact.
        disabled={reprocessing || deleting || exporting}
        style={{
          width: 32,
          height: 32,
          borderRadius: 'var(--r-xs)',
          border: 'none',
          background: open ? 'var(--sunken)' : 'transparent',
          color: 'var(--text-3)',
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
            background: 'var(--panel)',
            border: '1px solid var(--border)',
            borderRadius: 'var(--r-sm)',
            boxShadow: 'var(--shadow)',
            padding: 4,
            minWidth: 180,
          }}
        >
          <MenuItem
            onClick={() => {
              setOpen(false);
              onReprocess();
            }}
            disabled={
              reprocessing ||
              deleting ||
              exporting ||
              regenerating ||
              regeneratingTitle ||
              hasFailedChunks
            }
            title={
              hasFailedChunks
                ? t('chunkProgress.resumeBlockedHint')
                : undefined
            }
          >
            {reprocessing ? t('callDetail.reprocessing') : t('callDetail.reprocess')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onRegenerateRecap();
            }}
            disabled={
              regenerating ||
              regenerateDisabled ||
              reprocessing ||
              deleting ||
              exporting ||
              regeneratingTitle
            }
            title={regenerateDisabled ? t('callDetail.regenerateNoTranscript') : undefined}
          >
            {regenerating
              ? recapElapsedSec !== null
                ? t('callDetail.regeneratingWithElapsed', { sec: recapElapsedSec })
                : t('callDetail.regenerating')
              : t('callDetail.regenerateRecap')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onRegenerateTitle();
            }}
            disabled={
              regeneratingTitle ||
              regenerateTitleDisabled ||
              regenerating ||
              reprocessing ||
              deleting ||
              exporting
            }
            title={
              regenerateTitleDisabled
                ? t('callDetail.regenerateTitleNoTranscript')
                : undefined
            }
          >
            {regeneratingTitle
              ? t('callDetail.regeneratingTitle')
              : t('callDetail.regenerateTitle')}
          </MenuItem>
          <MenuItem
            onClick={() => {
              setOpen(false);
              onExport();
            }}
            disabled={exporting || reprocessing || deleting || regenerating || regeneratingTitle}
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
