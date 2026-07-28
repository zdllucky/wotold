// [TD-49] Омни-бар инбокса: текстовый ввод, фасет-токены и меню фасетов.
//
// Выделено из `InboxView.tsx` (820 строк при лимите 800, правило 8). Граница
// естественная: это самодостаточный контрол над `Facets`, который сам список
// звонков не знает вовсе. Разметка и логика перенесены символ-в-символ —
// визуально ничего не менялось.

import { useState } from 'react';

import { Dropdown, MenuItem, MenuLabel, MenuSep } from '../ui';
import { Icon } from '../ui/Icon';
import { useI18n } from '../i18n';

type TFn = ReturnType<typeof useI18n>['t'];
import {
  FACETS_EMPTY,
  hasRange,
  setRange,
  toggleFacet,
  facetCount,
  STR_FACET_KEYS,
  type FacetDef,
  type Facets,
  type StrFacetKey,
} from './inboxData';

// ── Omni-bar (text + facet tokens + suggestions) ──

interface OmniBarProps {
  facets: Facets;
  setFacets: (next: Facets) => void;
  text: string;
  setText: (v: string) => void;
  defs: FacetDef[];
  t: TFn;
}

export function OmniBar({ facets, setFacets, text, setText, defs, t }: OmniBarProps) {
  const [draft, setDraft] = useState('');
  const [focus, setFocus] = useState(false);

  const labelOf = (k: StrFacetKey, v: string) =>
    defs.find((d) => d.key === k)?.values.find((x) => x.v === v)?.label ?? v;
  const iconOf = (k: StrFacetKey) => defs.find((d) => d.key === k)?.icon ?? 'bolt';

  const tokens: { k: StrFacetKey; v: string }[] = [];
  STR_FACET_KEYS.forEach((k) => facets[k].forEach((v) => tokens.push({ k, v })));

  const allTok = defs.flatMap((d) =>
    d.values.map((val) => ({ k: d.key, v: val.v, label: val.label, fl: d.label, icon: d.icon })),
  );
  const q = draft.trim().toLowerCase();
  const sugg = (q
    ? allTok.filter((x) => x.label.toLowerCase().includes(q) || x.fl.toLowerCase().includes(q))
    : allTok
  )
    .filter((x) => !(facets[x.k] as string[]).includes(x.v))
    .slice(0, 5);

  const add = (tok: { k: StrFacetKey; v: string }) => {
    setFacets(toggleFacet(facets, tok.k, tok.v));
    setDraft('');
  };
  const rm = (k: StrFacetKey, v: string) => setFacets(toggleFacet(facets, k, v));
  const rangeOn = hasRange(facets);
  const hasAny = tokens.length > 0 || !!text || rangeOn;
  const rangeChip = rangeOn
    ? [facets.range.from, facets.range.to].filter(Boolean).join(' – ')
    : '';

  return (
    <div
      className="omni"
      data-focus={focus ? 'true' : undefined}
      data-tauri-drag-region="false"
      style={{ flex: 1, minWidth: 0 }}
    >
      <Icon name="search" size={15} style={{ color: 'var(--text-faint)', flex: '0 0 auto' }} />
      <div className="omni-row">
        {tokens.map((tok) => (
          <span
            key={tok.k + tok.v}
            className="chip chip--accent"
            style={{ gap: 4, flex: '0 0 auto' }}
          >
            <Icon name={iconOf(tok.k)} size={11} />
            {labelOf(tok.k, tok.v)}
            <button
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                rm(tok.k, tok.v);
              }}
              style={{ display: 'inline-flex', color: 'inherit' }}
              aria-label={t('inbox.removeFilter', { label: labelOf(tok.k, tok.v) })}
            >
              <Icon name="x" size={11} />
            </button>
          </span>
        ))}
        {text && (
          <span className="chip chip--line" style={{ gap: 4, flex: '0 0 auto' }}>
            «{text}»
            <button
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                setText('');
              }}
              style={{ display: 'inline-flex', color: 'inherit' }}
              aria-label={t('inbox.removeText', { q: text })}
            >
              <Icon name="x" size={11} />
            </button>
          </span>
        )}
        {rangeOn && (
          <span className="chip chip--accent" style={{ gap: 4, flex: '0 0 auto' }}>
            <Icon name="calendar" size={11} />
            {rangeChip}
            <button
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                setFacets(setRange(facets, { from: null, to: null }));
              }}
              style={{ display: 'inline-flex', color: 'inherit' }}
              aria-label={t('inbox.removeRange')}
            >
              <Icon name="x" size={11} />
            </button>
          </span>
        )}
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={hasAny ? '' : t('inbox.searchPlaceholder')}
          aria-label={t('inbox.searchPlaceholder')}
          onFocus={() => setFocus(true)}
          onBlur={() => setTimeout(() => setFocus(false), 160)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              if (q && sugg[0]) add(sugg[0]);
              else if (draft.trim()) {
                setText(draft.trim());
                setDraft('');
              }
            }
            if (e.key === 'Backspace' && !draft) {
              if (text) setText('');
              else if (tokens.length) {
                const last = tokens[tokens.length - 1]!;
                rm(last.k, last.v);
              }
            }
          }}
        />
      </div>
      {hasAny && (
        <button
          type="button"
          className="iconbtn"
          data-size="sm"
          onMouseDown={(e) => {
            e.preventDefault();
            setFacets({ ...FACETS_EMPTY });
            setText('');
          }}
          aria-label={t('inbox.clearAll')}
          style={{ flex: '0 0 auto' }}
        >
          <Icon name="x" size={14} />
        </button>
      )}
      {focus && sugg.length > 0 && (
        <div className="menu" style={{ left: 0, right: 0, top: 'calc(100% + 5px)', width: 'auto' }}>
          <div className="menu-label">{q ? t('inbox.addFilter') : t('inbox.quickFilters')}</div>
          {sugg.map((s) => (
            <button
              key={s.k + s.v}
              type="button"
              className="menu-item"
              onMouseDown={(e) => {
                e.preventDefault();
                add({ k: s.k, v: s.v });
              }}
            >
              <span className="mi-ico">
                <Icon name={s.icon} size={15} />
              </span>
              <span style={{ flex: 1 }}>
                <span className="u-faint">{s.fl}: </span>
                {s.label}
              </span>
            </button>
          ))}
          {q && (
            <button
              type="button"
              className="menu-item"
              onMouseDown={(e) => {
                e.preventDefault();
                setText(draft.trim());
                setDraft('');
              }}
            >
              <span className="mi-ico">
                <Icon name="search" size={15} />
              </span>
              <span>{t('inbox.searchInTitles', { q: draft.trim() })}</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// ── Facet button (dropdown checkboxes — same facet defs as the omni-bar) ──

interface FacetButtonProps {
  facets: Facets;
  setFacets: (next: Facets) => void;
  defs: FacetDef[];
  t: TFn;
}

export function FacetButton({ facets, setFacets, defs, t }: FacetButtonProps) {
  const count = facetCount(facets);
  return (
    <Dropdown
      width={232}
      trigger={({ toggle }) => (
        <button
          type="button"
          className="btn btn--default"
          onClick={toggle}
          style={
            count > 0 ? { borderColor: 'var(--accent)', color: 'var(--accent-text)' } : undefined
          }
        >
          <Icon name="filter" size={14} />
          {t('inbox.filter')}
          {count > 0 ? ` · ${count}` : ''}
        </button>
      )}
    >
      {defs.map((def, i) => (
        <div key={def.key}>
          {i > 0 && <MenuSep />}
          <MenuLabel>{def.label}</MenuLabel>
          {def.values.map((val) => {
            const on = (facets[def.key] as string[]).includes(val.v);
            return (
              <button
                key={val.v}
                type="button"
                className="menu-item"
                data-active={on ? 'true' : undefined}
                onClick={(e) => {
                  e.stopPropagation();
                  setFacets(toggleFacet(facets, def.key, val.v));
                }}
              >
                <span className="chk" data-done={on ? 'true' : undefined} style={{ width: 15, height: 15 }}>
                  <Icon name="check" size={11} />
                </span>
                <span style={{ flex: 1 }}>{val.label}</span>
              </button>
            );
          })}
        </div>
      ))}
      <MenuSep />
      {/* Custom date range — stopPropagation so the inputs don't close the menu. */}
      <div style={{ padding: '2px 4px 4px' }} onClick={(e) => e.stopPropagation()}>
        <MenuLabel>{t('inbox.periodCustom')}</MenuLabel>
        <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '6px 8px', padding: '4px 8px 2px', alignItems: 'center' }}>
          <span className="u-faint" style={{ fontSize: 12 }}>
            {t('inbox.periodFrom')}
          </span>
          <input
            type="date"
            className="input"
            data-size="sm"
            value={facets.range.from ?? ''}
            max={facets.range.to ?? undefined}
            aria-label={t('inbox.periodFrom')}
            onChange={(e) => setFacets(setRange(facets, { from: e.target.value || null }))}
          />
          <span className="u-faint" style={{ fontSize: 12 }}>
            {t('inbox.periodTo')}
          </span>
          <input
            type="date"
            className="input"
            data-size="sm"
            value={facets.range.to ?? ''}
            min={facets.range.from ?? undefined}
            aria-label={t('inbox.periodTo')}
            onChange={(e) => setFacets(setRange(facets, { to: e.target.value || null }))}
          />
        </div>
      </div>
      {count > 0 && (
        <>
          <MenuSep />
          <MenuItem icon="x" onClick={() => setFacets({ ...FACETS_EMPTY })}>
            {t('inbox.clearAll')}
          </MenuItem>
        </>
      )}
    </Dropdown>
  );
}
