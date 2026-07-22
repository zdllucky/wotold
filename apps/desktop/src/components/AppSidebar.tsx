// [B18.1a] Wotold v2 shell rail — collapsible Sidebar (256px) + MiniRail (56px).
// Port of ~/Downloads/Wotold v2/wk-app.jsx (Sidebar/MiniRail), wired to real
// data: i18n labels, RecordingContext status, pipeline badge, recent calls.
// Uses raw uikit classes from wk.css (.rail/.minirail/.navitem/.btn/...) +
// <Icon/>. Логика записи/навигации — в App.tsx; здесь только presentation.

import { Icon } from '../ui/Icon';
import { IconBtn, Kbd, NavItem } from '../ui';
import { StatusCell } from '../pages/inboxBits';
import { formatElapsed } from '../recording/RecordingContext';
import { LiveRecEq } from '../recording/LiveRecEq';
import { QueueMonitor } from './QueueMonitor';
import type { Call } from '../api/recording';
import type { QueueState } from '../api/queue';
import { useI18n } from '../i18n';

export type RailView = 'inbox' | 'call' | 'contacts' | 'assistant' | 'settings' | 'ds';
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
  /** [Q] Снапшот очередей ресурсов для QueueMonitor (null до первого фетча). */
  queue: QueueState | null;
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
    queue,
    onRecord,
    onPause,
    onNav,
    onOpenCall,
    onSearch,
    onCollapse,
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
      {/* [window] .rail-head — header рейла; на data-chrome=open лого едет
          вправо, освобождая угол под кастомный светофор (global.css). */}
      <div data-tauri-drag-region="deep" className="rail-head">
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
          /* [recording] Компактный ряд (без переполнения 256px): danger-кнопка =
             живая дорожка + таймер + стоп (она же индикатор), пауза = icon-кнопка. */
          <div style={{ display: 'flex', gap: 6 }}>
            <button
              type="button"
              className="btn btn--danger"
              onClick={onRecord}
              disabled={busy}
              aria-label={t('recording.stopAction')}
              style={{ flex: 1, gap: 8, justifyContent: 'flex-start' }}
            >
              <LiveRecEq paused={paused} inherit />
              <span className="mono" style={{ fontWeight: 600 }}>
                {formatElapsed(elapsed)}
              </span>
              <span style={{ flex: 1 }} />
              <Icon name="stop" size={15} />
            </button>
            <IconBtn
              icon={paused ? 'play' : 'pause'}
              label={paused ? t('recording.resumeAction') : t('recording.pauseAction')}
              onClick={onPause}
              disabled={busy}
            />
          </div>
        ) : (
          <button
            type="button"
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
        {/* [B24.6] «Найти или спросить» — одно из трёх ai-field-полей (SPEC §6). */}
        <button
          className="input ai-field ai-field--panel"
          onClick={onSearch}
          style={{ cursor: 'pointer', height: 30, color: 'var(--text-3)' }}
        >
          <Icon name="sparkle" size={15} className="iico" />
          <span
            style={{
              flex: 1,
              textAlign: 'left',
              fontSize: 13,
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}
          >
            {t('assistant.findOrAsk')}
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
        <NavItem
          icon="chat"
          label={t('assistant.title')}
          active={view === 'assistant'}
          current={view === 'assistant'}
          onClick={() => onNav('assistant')}
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
          {/* [Q] Монитор очередей ресурсов — занял место theme-toggle
              (тема переключается в Настройки → Оформление). */}
          <QueueMonitor queue={queue} calls={recent} />
        </div>
      </div>
      <div
        className="rail-resize"
        data-tauri-drag-region="false"
        onMouseDown={onResizeStart}
      />
    </aside>
  );
}

export function MiniRail(props: RailProps) {
  const { t } = useI18n();
  const {
    view,
    recKind,
    elapsed,
    busy,
    recent,
    queue,
    onRecord,
    onPause,
    onNav,
    onSearch,
    onExpand,
    onResizeStart,
  } = props;
  const recording = recKind !== 'idle';
  const paused = recKind === 'paused';

  return (
    <aside className="minirail" data-tauri-drag-region="deep">
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
          {/* [recording] Стоп-кнопка = только живая дорожка (белая, индикатор
              записи; клик = стоп через aria-label). Таймер — под паузой. */}
          <button
            className="mr-rec"
            data-rec="true"
            aria-label={t('recording.stopAction')}
            onClick={onRecord}
            disabled={busy}
          >
            <LiveRecEq paused={paused} inherit />
          </button>
          <IconBtn
            icon={paused ? 'play' : 'pause'}
            size="sm"
            iconSize={16}
            label={paused ? t('recording.resumeAction') : t('recording.pauseAction')}
            onClick={onPause}
            disabled={busy}
          />
          <span
            className="mono"
            style={{ fontSize: 10.5, fontWeight: 600, color: 'var(--text-3)' }}
          >
            {formatElapsed(elapsed)}
          </span>
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
        label={t('assistant.findOrAsk')}
        title={`${t('assistant.findOrAsk')} ⌘K`}
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
      {/* [Q] Монитор очередей — вместо theme-toggle (тема в Настройках). */}
      <QueueMonitor queue={queue} calls={recent} iconSize={18} />
      <IconBtn
        icon="settings"
        iconSize={18}
        label={t('nav.settings')}
        title={t('nav.settings')}
        active={view === 'settings'}
        onClick={() => onNav('settings')}
      />
      <div
        className="rail-resize"
        data-tauri-drag-region="false"
        onMouseDown={onResizeStart}
      />
    </aside>
  );
}
