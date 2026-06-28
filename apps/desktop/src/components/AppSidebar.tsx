// [B18.1a] Wotold v2 shell rail — collapsible Sidebar (256px) + MiniRail (56px).
// Port of ~/Downloads/Wotold v2/wk-app.jsx (Sidebar/MiniRail), wired to real
// data: i18n labels, RecordingContext status, pipeline badge, recent calls.
// Uses raw uikit classes from wk.css (.rail/.minirail/.navitem/.btn/...) +
// <Icon/>. Логика записи/навигации — в App.tsx; здесь только presentation.

import { Icon } from '../ui/Icon';
import { IconBtn, Kbd, NavItem } from '../ui';
import { StatusCell } from '../pages/inboxBits';
import { formatElapsed } from '../recording/RecordingContext';
import type { Call } from '../api/recording';
import { useI18n } from '../i18n';

export type RailView = 'inbox' | 'call' | 'contacts' | 'settings' | 'ds';
export type RailRecKind = 'idle' | 'recording' | 'paused';

interface RailHandlers {
  /** Toggle: idle→start (consent-gated), recording→stop+navigate, paused→resume. */
  onRecord: () => void;
  /** Toggle pause↔resume (no-op when idle). */
  onPause: () => void;
  onNav: (v: RailView) => void;
  onOpenCall: (id: string) => void;
  /** Open the ⌘K command palette. */
  onSearch: () => void;
  onCollapse: () => void;
  onExpand: () => void;
  onToggleTheme: () => void;
  onResizeStart: (e: React.MouseEvent) => void;
}

interface RailProps extends RailHandlers {
  view: RailView;
  recKind: RailRecKind;
  elapsed: number;
  busy: boolean;
  pipelineCount: number;
  recent: Call[];
  /** Total calls / contacts — shown as nav count badges. */
  callsCount: number;
  contactsCount: number;
  /** Currently-open call id (view==='call') — highlights its recent row. */
  activeCallId: string | null;
  isDev: boolean;
  resolvedTheme: 'light' | 'dark';
}

function fmtDur(sec: number | null): string {
  if (sec == null) return '—';
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}

function Brand() {
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 9,
        fontWeight: 700,
        fontSize: 15,
        letterSpacing: '-.01em',
      }}
    >
      <svg
        width="29"
        height="20"
        viewBox="0 0 72 50"
        fill="none"
        aria-hidden="true"
        style={{ display: 'block', flex: '0 0 auto' }}
      >
        <rect x="3" y="6" width="56" height="17" rx="8.5" fill="var(--text)" />
        <rect x="13" y="27" width="56" height="17" rx="8.5" fill="var(--text-3)" />
      </svg>
      Wotold
    </span>
  );
}

/** Status dot used in recording rows. */
function RecDot({ paused }: { paused: boolean }) {
  return (
    <span
      className={`dot${paused ? '' : ' dot--pulse'}`}
      style={{ background: 'var(--danger)' }}
      aria-hidden
    />
  );
}

export function Sidebar(props: RailProps) {
  const { t } = useI18n();
  const {
    view,
    recKind,
    elapsed,
    busy,
    pipelineCount,
    recent,
    callsCount,
    contactsCount,
    activeCallId,
    isDev,
    resolvedTheme,
    onRecord,
    onPause,
    onNav,
    onOpenCall,
    onSearch,
    onCollapse,
    onToggleTheme,
    onResizeStart,
  } = props;
  const recording = recKind !== 'idle';
  const paused = recKind === 'paused';
  const recentTop = recent.slice(0, 5);
  const onInbox = view === 'inbox' || view === 'call';

  // Inbox meta: live processing badge when calls are in the pipeline, else the
  // total call count (prototype shows a count badge on each primary nav row).
  const inboxMeta =
    pipelineCount > 0 ? (
      <span
        style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}
        title={t('nav.processingTitle', {
          n: pipelineCount,
          plural: pipelineCount === 1 ? t('nav.callsPluralOne') : t('nav.callsPluralMany'),
        })}
      >
        <span className="dot dot--pulse" style={{ background: 'var(--accent)' }} aria-hidden />
        {pipelineCount}
      </span>
    ) : callsCount > 0 ? (
      callsCount
    ) : undefined;

  return (
    <aside className="rail" aria-label={t('nav.main')}>
      <div
        data-tauri-drag-region
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '12px 12px 8px',
        }}
      >
        <Brand />
        <IconBtn
          icon="sidebar"
          size="sm"
          iconSize={16}
          label={t('rail.collapse')}
          title={`${t('rail.collapse')} ⌘\\`}
          onClick={onCollapse}
        />
      </div>

      {/* Record / recording status */}
      <div style={{ padding: '0 10px 8px' }}>
        {recording ? (
          <div style={{ display: 'grid', gap: 6 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '2px 4px' }}>
              <RecDot paused={paused} />
              <span style={{ color: 'var(--danger)', fontWeight: 600, fontSize: 12.5 }}>
                {paused ? t('recording.stripPaused') : t('recording.stripRecording')}
              </span>
              <span className="mono" style={{ marginLeft: 'auto', fontSize: 13, fontWeight: 600 }}>
                {formatElapsed(elapsed)}
              </span>
            </div>
            <div style={{ display: 'flex', gap: 6 }}>
              <button
                className="btn btn--default"
                style={{ flex: 1 }}
                onClick={onPause}
                disabled={busy}
              >
                <Icon name={paused ? 'play' : 'pause'} size={15} />
                {paused ? t('recording.resumeAction') : t('recording.pauseAction')}
              </button>
              <button className="btn btn--danger" onClick={onRecord} disabled={busy}>
                <Icon name="stop" size={15} />
                {t('recording.stopAction')}
              </button>
            </div>
          </div>
        ) : (
          <button
            className="btn btn--primary"
            data-block="true"
            onClick={onRecord}
            disabled={busy}
          >
            <Icon name="mic" size={16} />
            {t('rail.record')}
          </button>
        )}
      </div>

      <div style={{ padding: '0 10px 8px' }}>
        <button
          className="input"
          onClick={onSearch}
          style={{
            cursor: 'pointer',
            height: 30,
            color: 'var(--text-faint)',
            borderColor: 'var(--border-2)',
          }}
        >
          <Icon name="command" size={15} className="iico" />
          <span style={{ flex: 1, textAlign: 'left', fontSize: 13 }}>
            {t('palette.commands')}
          </span>
          <Kbd>⌘K</Kbd>
        </button>
      </div>

      <nav className="scroll" style={{ flex: 1, minHeight: 0, padding: 10 }}>
        <NavItem
          icon="inbox"
          label={t('nav.calls')}
          active={onInbox}
          current={onInbox}
          meta={inboxMeta}
          onClick={() => onNav('inbox')}
        />
        <NavItem
          icon="users"
          label={t('nav.contacts')}
          active={view === 'contacts'}
          current={view === 'contacts'}
          meta={contactsCount > 0 ? contactsCount : undefined}
          onClick={() => onNav('contacts')}
        />

        {recentTop.length > 0 && (
          <>
            <div style={{ height: 8 }} />
            <div className="sec-label">{t('rail.recent')}</div>
            {recentTop.map((c) => {
              const label = c.title ?? c.id.slice(0, 8);
              const openHere = view === 'call' && activeCallId === c.id;
              return (
                <NavItem
                  key={c.id}
                  label={label}
                  title={label}
                  active={openHere}
                  current={openHere}
                  leading={
                    <span className="nav-ico">
                      <StatusCell call={c} />
                    </span>
                  }
                  meta={fmtDur(c.duration_sec)}
                  onClick={() => onOpenCall(c.id)}
                />
              );
            })}
          </>
        )}
      </nav>

      <div style={{ borderTop: '1px solid var(--border)', padding: 8 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <NavItem
            icon="settings"
            label={t('nav.settings')}
            active={view === 'settings'}
            current={view === 'settings'}
            style={{ flex: 1 }}
            onClick={() => onNav('settings')}
          />
          {isDev && (
            <IconBtn
              icon="code"
              size="sm"
              iconSize={16}
              active={view === 'ds'}
              label={t('rail.designSystem')}
              title={t('rail.designSystem')}
              onClick={() => onNav('ds')}
            />
          )}
          <IconBtn
            icon={resolvedTheme === 'dark' ? 'sun' : 'moon'}
            size="sm"
            iconSize={16}
            label={t('nav.settings')}
            title={resolvedTheme === 'dark' ? 'Light' : 'Dark'}
            onClick={onToggleTheme}
          />
        </div>
      </div>
      <div className="rail-resize" onMouseDown={onResizeStart} />
    </aside>
  );
}

export function MiniRail(props: RailProps) {
  const { t } = useI18n();
  const {
    view,
    recKind,
    busy,
    onRecord,
    onPause,
    onNav,
    onSearch,
    onExpand,
    onToggleTheme,
    onResizeStart,
    resolvedTheme,
  } = props;
  const recording = recKind !== 'idle';
  const paused = recKind === 'paused';

  return (
    <aside className="minirail" data-tauri-drag-region>
      <IconBtn
        icon="sidebar"
        iconSize={18}
        label={t('rail.expand')}
        title={`${t('rail.expand')} ⌘\\`}
        onClick={onExpand}
      />
      <div className="minirail-sep" />
      {recording ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6, alignItems: 'center' }}>
          <button
            className="mr-rec"
            data-rec="true"
            aria-label={t('recording.stopAction')}
            onClick={onRecord}
            disabled={busy}
          >
            <Icon name="stop" size={18} />
          </button>
          <IconBtn
            icon={paused ? 'play' : 'pause'}
            size="sm"
            iconSize={16}
            label={paused ? t('recording.resumeAction') : t('recording.pauseAction')}
            onClick={onPause}
            disabled={busy}
          />
        </div>
      ) : (
        <button
          className="mr-rec"
          aria-label={t('rail.record')}
          title={t('rail.record')}
          onClick={onRecord}
          disabled={busy}
        >
          <Icon name="mic" size={19} />
        </button>
      )}
      <IconBtn
        icon="command"
        iconSize={18}
        label={t('palette.commands')}
        title={`${t('palette.commands')} ⌘K`}
        onClick={onSearch}
      />
      <div className="minirail-sep" />
      <IconBtn
        icon="inbox"
        iconSize={18}
        label={t('nav.calls')}
        title={t('nav.calls')}
        active={view === 'inbox' || view === 'call'}
        onClick={() => onNav('inbox')}
      />
      <IconBtn
        icon="users"
        iconSize={18}
        label={t('nav.contacts')}
        title={t('nav.contacts')}
        active={view === 'contacts'}
        onClick={() => onNav('contacts')}
      />
      <div className="mr-spacer" />
      <IconBtn
        icon={resolvedTheme === 'dark' ? 'sun' : 'moon'}
        iconSize={18}
        label="Theme"
        onClick={onToggleTheme}
      />
      <IconBtn
        icon="settings"
        iconSize={18}
        label={t('nav.settings')}
        title={t('nav.settings')}
        active={view === 'settings'}
        onClick={() => onNav('settings')}
      />
      <div className="rail-resize" onMouseDown={onResizeStart} />
    </aside>
  );
}
