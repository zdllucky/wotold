// [B18.2b] Shared inbox row bits — used by the list view (InboxView) and the
// calendar views (InboxCalendarViews). Extracted from InboxView to keep that
// file under the 800-line guard and avoid duplicating the status/avatar UI.

import type { Call } from '../api/recording';
import type { CallState } from '../types/callState';
import { SP_COLORS, deriveCallState } from './inboxData';

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
