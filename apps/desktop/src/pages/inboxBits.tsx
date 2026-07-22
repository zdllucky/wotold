// [B18.2b / B18.9] Shared inbox row bits — the status dot, avatar group, and the
// v2 database `.trow` table row. Used by the list view (InboxView) and the
// calendar views (InboxCalendarViews). Extracted from InboxView to keep that
// file under the 800-line guard and avoid duplicating the status/avatar UI.

import { useState, type ReactNode } from 'react';
import type { Call } from '../api/recording';
import { type CallState, pipelineStepKey } from '../types/callState';
import { bcp47, type useI18n } from '../i18n';
import {
  Chip,
  ContextMenu,
  type ContextMenuPos,
  Dropdown,
  IconBtn,
  MenuItem,
  MenuSep,
} from '../ui';
import { Icon } from '../ui/Icon';
import {
  SP_COLORS,
  callHasRecap,
  deriveCallState,
  formatDuration,
  inferSpeakers,
} from './inboxData';

type TFn = ReturnType<typeof useI18n>['t'];

export const STATE_COLOR: Record<CallState, string> = {
  live: 'var(--danger)',
  error: 'var(--danger)',
  ready: 'var(--ok)',
  uploading: 'var(--accent)',
  processing: 'var(--accent)',
  queued: 'var(--text-faint)',
};

export function statusColor(state: CallState): string {
  return STATE_COLOR[state];
}

export function StatusCell({ call, busy = false }: { call: Call; busy?: boolean }) {
  const state: CallState = busy ? 'processing' : deriveCallState(call);
  const pulse = state === 'processing' || state === 'uploading' || state === 'live';
  return (
    <span
      className={`dot${pulse ? ' dot--pulse' : ''}`}
      style={{ background: STATE_COLOR[state] }}
      aria-hidden
    />
  );
}

export function AvatarGroup({ list }: { list: string[] }) {
  return (
    <span className="avatar-grp" data-on="bg" style={{ alignItems: 'center' }}>
      {list.slice(0, 3).map((s, i) => (
        <span
          key={i}
          className="avatar"
          style={{
            background: SP_COLORS[i % SP_COLORS.length],
            width: 24,
            height: 24,
            fontSize: 9,
          }}
        >
          {s}
        </span>
      ))}
      {list.length > 3 && (
        <span className="u-faint mono" style={{ fontSize: 11, marginLeft: 6 }}>
          +{list.length - 3}
        </span>
      )}
    </span>
  );
}

// ── Table row (.trow) — Wotold v2 database list ──

/** Short, locale-aware day · month label for the date column. */
function formatShortDate(iso: string, locale: string): string {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return iso;
  return d.toLocaleDateString(bcp47(locale as Parameters<typeof bcp47>[0]), {
    day: 'numeric',
    month: 'short',
  });
}

/**
 * Row status chip — preserves the original deriveCallState / busy semantics:
 * a ready call with an active background task (regen) shows the «processing»
 * indicator even though its DB status stays 'ready'.
 */
function RowStatusChip({
  call,
  busy,
  state,
  t,
}: {
  call: Call;
  busy: boolean;
  state: CallState;
  t: TFn;
}): ReactNode {
  if (busy) {
    return (
      <Chip size="sm" tone="accent">
        {t('calls.secondaryBusy')}
      </Chip>
    );
  }
  if (state === 'error' || state === 'live') {
    return (
      <Chip size="sm" tone="danger">
        {t(`callState.${state}`)}
      </Chip>
    );
  }
  if (state === 'processing' || state === 'uploading' || state === 'queued') {
    const label =
      state === 'processing' || state === 'uploading'
        ? t(pipelineStepKey(call.pipeline_step))
        : t(`callState.${state}`);
    return (
      <Chip size="sm" tone="accent">
        {label}
      </Chip>
    );
  }
  return null;
}

interface TableRowProps {
  call: Call;
  onOpen: (id: string) => void;
  /** Row-menu actions — when omitted the item is disabled (no-op). */
  onReprocess?: (call: Call) => void;
  onExport?: (call: Call) => void;
  onDelete?: (call: Call) => void;
  speakers?: string[];
  isActive?: boolean;
  locale: string;
  t: TFn;
}

/** [B20.5] Что доступно для звонка в row/context-меню (общее для kebab в
 *  таблице и ПКМ-меню календарных видов). */
export function rowCaps(
  call: Call,
  busy: boolean,
): { canReprocess: boolean; canExport: boolean } {
  const uiState = deriveCallState(call);
  return {
    // Reprocess: ready or failed calls, not while a task is running.
    canReprocess: !busy && (uiState === 'ready' || uiState === 'error'),
    // Export needs a finished transcript/recap → ready only.
    canExport: uiState === 'ready',
  };
}

interface CallMenuItemsProps {
  call: Call;
  busy: boolean;
  onOpen: (id: string) => void;
  onReprocess?: (call: Call) => void;
  onExport?: (call: Call) => void;
  onDelete?: (call: Call) => void;
  t: TFn;
}

/** [B20.5] Пункты row-меню звонка — общие для kebab-Dropdown и ПКМ ContextMenu. */
export function CallMenuItems({
  call,
  busy,
  onOpen,
  onReprocess,
  onExport,
  onDelete,
  t,
}: CallMenuItemsProps) {
  const { canReprocess, canExport } = rowCaps(call, busy);
  return (
    <>
      <MenuItem icon="doc" onClick={() => onOpen(call.id)}>
        {t('inbox.rowOpen')}
      </MenuItem>
      <MenuItem icon="refresh" disabled={!canReprocess} onClick={() => onReprocess?.(call)}>
        {t('inbox.rowReprocess')}
      </MenuItem>
      <MenuItem icon="download" disabled={!canExport} onClick={() => onExport?.(call)}>
        {t('inbox.rowExport')}
      </MenuItem>
      <MenuSep />
      <MenuItem icon="trash" danger onClick={() => onDelete?.(call)}>
        {t('common.delete')}
      </MenuItem>
    </>
  );
}

export function TableRow({
  call,
  onOpen,
  onReprocess,
  onExport,
  onDelete,
  speakers,
  isActive,
  locale,
  t,
}: TableRowProps) {
  const list = speakers && speakers.length > 0 ? speakers : inferSpeakers(call);
  const uiState = deriveCallState(call);
  const busy = call.status === 'ready' && isActive === true;
  const hasTag = busy || uiState !== 'ready';
  const title = call.title ?? t('calls.fallbackCallTitle', { short: call.id.slice(0, 8) });
  // [B20.5] ПКМ на строке = то же меню что и kebab, у курсора.
  const [ctxPos, setCtxPos] = useState<ContextMenuPos | null>(null);

  return (
    <div
      role="button"
      tabIndex={0}
      className="trow"
      onClick={() => onOpen(call.id)}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setCtxPos({ x: e.clientX, y: e.clientY });
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onOpen(call.id);
        }
      }}
    >
      <StatusCell call={call} busy={busy} />
      <span className="t-title u-trunc">
        <span className="u-trunc" title={title}>
          {title}
        </span>
        {callHasRecap(call) && !hasTag && (
          <Icon name="sparkle" size={12} style={{ color: 'var(--text-faint)', flex: '0 0 auto' }} />
        )}
        <RowStatusChip call={call} busy={busy} state={uiState} t={t} />
      </span>
      <span>
        <AvatarGroup list={list} />
      </span>
      <span className="t-cell mono">{formatDuration(call.duration_sec)}</span>
      <span className="t-cell">{formatShortDate(call.started_at, locale)}</span>
      <span className="t-more" onClick={(e) => e.stopPropagation()}>
        <Dropdown
          align="right"
          width={190}
          trigger={({ toggle }) => (
            <IconBtn icon="dots" size="sm" onClick={toggle} label={t('inbox.rowActions')} />
          )}
        >
          <CallMenuItems
            call={call}
            busy={busy}
            onOpen={onOpen}
            onReprocess={onReprocess}
            onExport={onExport}
            onDelete={onDelete}
            t={t}
          />
        </Dropdown>
      </span>
      {ctxPos && (
        <ContextMenu pos={ctxPos} onClose={() => setCtxPos(null)}>
          <CallMenuItems
            call={call}
            busy={busy}
            onOpen={onOpen}
            onReprocess={onReprocess}
            onExport={onExport}
            onDelete={onDelete}
            t={t}
          />
        </ContextMenu>
      )}
    </div>
  );
}
