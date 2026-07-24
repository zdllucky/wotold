// [B18.1c] ⌘K command palette — Wotold v2. Port of docs/design/wotold-v2/_reference
// wk-app.jsx Palette. Global launcher: actions (record / inbox / contacts /
// settings) + call search over `recent` (title). Keyboard: ↑/↓ move, Enter
// run, Esc / overlay-click close. Focus-trapped. Only triggers existing
// App-level callbacks — no recording/nav logic here.

import { useEffect, useMemo, useRef, useState } from 'react';
import { Icon, type IconName } from '../ui/Icon';
import { Kbd } from '../ui';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { bcp47, useI18n } from '../i18n';
import { getAssistantIndexStats } from '../api/assistant';
import type { Call } from '../api/recording';
import type { RailView } from './AppSidebar';

interface CommandPaletteProps {
  onClose: () => void;
  onNav: (v: RailView) => void;
  onOpenCall: (id: string) => void;
  onRecord: () => void;
  /** [B24.6] Fallback «Спросить ассистента»: новый глобальный чат с запросом. */
  onAsk?: (question: string) => void;
  recent: Call[];
}

interface FlatItem {
  key: string;
  kind: 'action' | 'call';
  icon: IconName;
  label: string;
  kbd?: string;
  meta?: string;
  run: () => void;
}

function shortDate(iso: string, locale: string): string {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return '';
  try {
    return d.toLocaleDateString(bcp47(locale as Parameters<typeof bcp47>[0]), {
      day: 'numeric',
      month: 'short',
    });
  } catch {
    return '';
  }
}

export function CommandPalette({
  onClose,
  onNav,
  onOpenCall,
  onRecord,
  onAsk,
  recent,
}: CommandPaletteProps) {
  const { t, locale } = useI18n();
  const ref = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [q, setQ] = useState('');
  const [sel, setSel] = useState(0);

  useFocusTrap(ref, true, { onClose });
  useEffect(() => {
    inputRef.current?.focus();
  }, []);
  // [B24.6] N для подстроки fallback («поиск по N звонкам»). Ошибка не критична.
  const [indexedCalls, setIndexedCalls] = useState<number | null>(null);
  useEffect(() => {
    getAssistantIndexStats()
      .then((s) => setIndexedCalls(s.indexedCalls))
      .catch(() => {});
  }, []);
  useEffect(() => {
    setSel(0);
  }, [q]);

  const ql = q.trim().toLowerCase();

  const actions = useMemo<FlatItem[]>(
    () => [
      { key: 'a:rec', kind: 'action', icon: 'record', label: t('rail.record'), kbd: '⌘⇧R', run: onRecord },
      { key: 'a:inbox', kind: 'action', icon: 'inbox', label: t('palette.allCalls'), run: () => onNav('inbox') },
      { key: 'a:contacts', kind: 'action', icon: 'users', label: t('nav.contacts'), run: () => onNav('contacts') },
      { key: 'a:assistant', kind: 'action', icon: 'chat', label: t('assistant.paletteCommand'), run: () => onNav('assistant') },
      { key: 'a:settings', kind: 'action', icon: 'settings', label: t('nav.settings'), run: () => onNav('settings') },
    ],
    [t, onRecord, onNav],
  );

  const filteredActions = useMemo(
    () => actions.filter((a) => a.label.toLowerCase().includes(ql)),
    [actions, ql],
  );

  const filteredCalls = useMemo<FlatItem[]>(() => {
    return recent
      .filter((c) => (c.title ?? c.id).toLowerCase().includes(ql))
      .slice(0, 8)
      .map((c) => ({
        key: `c:${c.id}`,
        kind: 'call' as const,
        icon: 'doc' as IconName,
        label: c.title ?? c.id.slice(0, 8),
        meta: shortDate(c.started_at, locale),
        run: () => onOpenCall(c.id),
      }));
  }, [recent, ql, locale, onOpenCall]);

  // [B24.6] ⌘K-fallback (SPEC §5): ни команд, ни звонков → «Спросить ассистента».
  const askFallback = useMemo<FlatItem | null>(() => {
    if (!onAsk || !ql || filteredActions.length > 0 || filteredCalls.length > 0) return null;
    const question = q.trim();
    return {
      key: 'ask:fallback',
      kind: 'action',
      icon: 'chat',
      label: t('assistant.paletteFallbackLabel'),
      kbd: '↵',
      run: () => onAsk(question),
    };
  }, [onAsk, ql, q, filteredActions.length, filteredCalls.length, t]);

  const flat = useMemo(
    () => [...filteredActions, ...filteredCalls, ...(askFallback ? [askFallback] : [])],
    [filteredActions, filteredCalls, askFallback],
  );

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSel((s) => Math.min(s + 1, flat.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSel((s) => Math.max(s - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      flat[sel]?.run();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <div className="overlay fade" onMouseDown={onClose}>
      <div
        ref={ref}
        className="palette fade-up ai-field"
        role="dialog"
        aria-modal="true"
        aria-label={t('palette.placeholder')}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="palette-input">
          <Icon name="search" size={18} style={{ color: 'var(--text-faint)' }} />
          <input
            ref={inputRef}
            placeholder={t('palette.placeholder')}
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onKeyDown}
            aria-label={t('palette.placeholder')}
            /* [B24.7 a11y M2] combobox+listbox: фокус в инпуте, «выбранный»
               пункт транслируется AT через activedescendant. */
            role="combobox"
            aria-expanded={flat.length > 0}
            aria-controls="palette-listbox"
            aria-activedescendant={flat.length > 0 ? `palette-opt-${sel}` : undefined}
          />
          <Kbd>esc</Kbd>
        </div>
        <div className="palette-list scroll" role="listbox" id="palette-listbox">
          {filteredActions.length > 0 && (
            <div className="menu-label">{t('palette.commands')}</div>
          )}
          {filteredActions.map((it) => {
            const idx = flat.findIndex((f) => f.key === it.key);
            return (
              <button
                key={it.key}
                type="button"
                className="menu-item"
                id={`palette-opt-${idx}`}
                role="option"
                aria-selected={idx === sel}
                tabIndex={-1}
                data-active={idx === sel ? 'true' : undefined}
                onMouseMove={() => setSel(idx)}
                onClick={() => it.run()}
              >
                <span className="mi-ico">
                  <Icon name={it.icon} size={16} />
                </span>
                {it.label}
                {it.kbd && (
                  <span className="mi-end">
                    <Kbd>{it.kbd}</Kbd>
                  </span>
                )}
              </button>
            );
          })}

          {filteredCalls.length > 0 && (
            <div className="menu-label">{t('palette.calls')}</div>
          )}
          {filteredCalls.map((it) => {
            const idx = flat.findIndex((f) => f.key === it.key);
            return (
              <button
                key={it.key}
                type="button"
                className="menu-item"
                id={`palette-opt-${idx}`}
                role="option"
                aria-selected={idx === sel}
                tabIndex={-1}
                data-active={idx === sel ? 'true' : undefined}
                onMouseMove={() => setSel(idx)}
                onClick={() => it.run()}
              >
                <span className="mi-ico">
                  <Icon name={it.icon} size={16} />
                </span>
                <span className="u-trunc" style={{ flex: 1, minWidth: 0 }}>
                  {it.label}
                </span>
                {it.meta && <span className="mi-end u-faint" style={{ fontSize: 11 }}>{it.meta}</span>}
              </button>
            );
          })}

          {askFallback && (
            <>
              <div className="menu-label">{t('assistant.paletteNotFound')}</div>
              <button
                type="button"
                className="menu-item"
                id={`palette-opt-${flat.length - 1}`}
                role="option"
                aria-selected={sel === flat.length - 1}
                tabIndex={-1}
                data-active={sel === flat.length - 1 ? 'true' : undefined}
                onMouseMove={() => setSel(flat.length - 1)}
                onClick={() => askFallback.run()}
              >
                <span className="mi-ico">
                  <Icon name="chat" size={16} />
                </span>
                <span style={{ flex: 1, minWidth: 0 }}>
                  <span style={{ display: 'block', fontWeight: 550 }}>{askFallback.label}</span>
                  <span className="u-faint" style={{ display: 'block', fontSize: 11.5 }}>
                    {t('assistant.paletteFallbackHint', { q: q.trim(), n: indexedCalls ?? recent.length })}
                  </span>
                </span>
                <span className="mi-end">
                  <Kbd>↵</Kbd>
                </span>
              </button>
            </>
          )}

          {flat.length === 0 && (
            <div className="u-muted" style={{ padding: '18px 12px', fontSize: 13 }}>
              {t('palette.empty')}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
