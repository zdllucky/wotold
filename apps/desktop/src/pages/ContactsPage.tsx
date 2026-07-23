// [B18.4] ContactsPage — Wotold v2 two-pane reskin (port of wk-extra.jsx
// ContactsView). Left flat .lrow search-list + right .doc detail (avatar / name
// / voice-confirm / identifier chips / recent calls / voice samples). CRUD +
// stats logic preserved 1-to-1 from the Atelier version; alphabet grouping
// dropped for the v2 flat list.

import { useEffect, useMemo, useState, type CSSProperties, type ReactNode } from 'react';
import { humanError } from '../api/errors';
import { ask } from '@tauri-apps/plugin-dialog';

import {
  createContact,
  deleteContact,
  listContacts,
  updateContact,
  type Contact,
  type ContactInput,
} from '../api/contacts';
import { listCalls, type Call } from '../api/recording';
import { listCallSpeakers } from '../api/speakers';
import { SP_COLORS } from './CallDetailUtils';
import { Button, Empty, IconBtn, Skeleton, ViewHead } from '../ui';
import { useResizablePanel } from '../hooks/useResizablePanel';
import { Icon, type IconName } from '../ui/Icon';
import { bcp47, useI18n } from '../i18n';
import { ContactFormModal } from './ContactFormModal';
import { VoiceSamplesSection } from './VoiceSamplesSection';

interface ContactStats {
  callCount: number;
  totalSec: number;
}

// [B23] Add/edit живут в ContactFormModal (канон v2) — панель только view.
type Mode = { kind: 'view'; contactId: string } | { kind: 'empty' };

function initials(name: string): string {
  return (
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('') || '·'
  );
}

function avatarColor(idx: number): string {
  return SP_COLORS[idx % SP_COLORS.length]!;
}

const ID_ICON: Record<string, IconName> = {
  phone: 'phone',
  email: 'external',
  telegram: 'send',
  whatsapp: 'link',
  signal: 'shield',
  slack: 'link',
};

function statusColor(status: string): string {
  if (status === 'ready') return 'var(--ok)';
  if (status === 'failed') return 'var(--danger)';
  return 'var(--accent)';
}

interface ContactsPageProps {
  onOpenCall?: (callId: string) => void;
}

export function ContactsPage({ onOpenCall }: ContactsPageProps = {}) {
  const { t } = useI18n();
  const [contacts, setContacts] = useState<Contact[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>({ kind: 'empty' });
  // [B23] Открытая модалка формы: null = закрыта, contact=null = создание.
  const [form, setForm] = useState<{ contact: Contact | null } | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [formBusy, setFormBusy] = useState(false);
  const [search, setSearch] = useState('');
  // [B29.5b] Панель списка: drag-resize + collapse до полосы аватаров.
  const panel = useResizablePanel({
    min: 200,
    max: 400,
    defaultWidth: 240,
    collapseAt: 170,
    widthKey: 'wk-ctw',
    collapsedKey: 'wk-ct-collapsed',
  });
  const [statsByContact, setStatsByContact] = useState<Map<string, ContactStats>>(new Map());
  const [callsByContact, setCallsByContact] = useState<Map<string, Call[]>>(new Map());

  const refresh = () => {
    listContacts()
      .then((cs) => {
        setContacts(cs);
        setMode((prev) => {
          if (prev.kind === 'empty' && cs.length > 0) {
            return { kind: 'view', contactId: cs[0]!.id };
          }
          return prev;
        });
      })
      .catch((e: unknown) => setError(humanError(e)));
  };

  useEffect(refresh, []);

  // Aggregate stats + recent calls per confirmed contact. Heavy (N+1) on first
  // mount, cached until reload.
  useEffect(() => {
    if (!contacts || contacts.length === 0) return;
    void (async () => {
      try {
        const calls = await listCalls();
        const speakerLists = await Promise.allSettled(calls.map((c) => listCallSpeakers(c.id)));
        const stats = new Map<string, ContactStats>();
        const byContact = new Map<string, Call[]>();
        speakerLists.forEach((r, i) => {
          if (r.status !== 'fulfilled') return;
          const call = calls[i]!;
          const seen = new Set<string>();
          for (const s of r.value) {
            if (s.confirmed && s.contact_id && !seen.has(s.contact_id)) {
              seen.add(s.contact_id);
              const prev = stats.get(s.contact_id) ?? { callCount: 0, totalSec: 0 };
              stats.set(s.contact_id, {
                callCount: prev.callCount + 1,
                totalSec: prev.totalSec + (call.duration_sec ?? 0),
              });
              const list = byContact.get(s.contact_id) ?? [];
              list.push(call);
              byContact.set(s.contact_id, list);
            }
          }
        });
        setStatsByContact(stats);
        setCallsByContact(byContact);
      } catch (e) {
        console.warn('contact stats aggregate failed', e);
      }
    })();
  }, [contacts]);

  const handleCreate = async (input: ContactInput) => {
    setFormBusy(true);
    try {
      const created = await createContact(input);
      setFormError(null);
      setContacts(await listContacts());
      setMode({ kind: 'view', contactId: created.id });
      setForm(null);
    } catch (e) {
      // [B23-fix] Модалка остаётся открытой; ошибка рендерится ВНУТРИ неё
      // (панель под оверлеем не видна).
      setFormError(humanError(e));
    } finally {
      setFormBusy(false);
    }
  };

  const handleUpdate = async (id: string, input: ContactInput) => {
    setFormBusy(true);
    try {
      await updateContact(id, input);
      setFormError(null);
      setContacts(await listContacts());
      setMode({ kind: 'view', contactId: id });
      setForm(null);
    } catch (e) {
      setFormError(humanError(e));
    } finally {
      setFormBusy(false);
    }
  };

  const handleDelete = async (c: Contact) => {
    const ok = await ask(t('contacts.deleteConfirmBody', { name: c.display_name }), {
      title: 'Wotold',
      kind: 'warning',
      okLabel: t('common.delete'),
      cancelLabel: t('common.cancel'),
    });
    if (!ok) return;
    try {
      await deleteContact(c.id);
      setError(null);
      const fresh = await listContacts();
      setContacts(fresh);
      setMode(fresh.length > 0 ? { kind: 'view', contactId: fresh[0]!.id } : { kind: 'empty' });
    } catch (e) {
      setError(humanError(e));
    }
  };

  const filtered = useMemo(() => {
    if (!contacts) return [];
    const q = search.trim().toLowerCase();
    if (!q) return contacts;
    return contacts.filter((c) =>
      [c.display_name, c.org ?? '', c.role ?? '', c.notes ?? '', ...c.identifiers.map((i) => i.value)]
        .join(' ')
        .toLowerCase()
        .includes(q),
    );
  }, [contacts, search]);

  if (error && !contacts) {
    return (
      <p role="alert" style={{ color: 'var(--danger)', fontFamily: 'var(--font)' }}>
        {error}
      </p>
    );
  }
  if (!contacts) {
    return (
      <section aria-busy="true">
        <Skeleton width="9ch" height="2rem" style={{ marginBottom: 18 }} />
        <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
          {Array.from({ length: 6 }, (_, i) => (
            <li
              key={i}
              style={{ display: 'grid', gridTemplateColumns: '32px 1fr', gap: 12, padding: '11px 0' }}
            >
              <Skeleton width="32px" height="32px" radius="50%" />
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                <Skeleton width="12rem" height="1em" />
                <Skeleton width="8rem" height="0.75em" />
              </div>
            </li>
          ))}
        </ul>
      </section>
    );
  }

  const activeContact =
    mode.kind === 'view' ? contacts.find((c) => c.id === mode.contactId) : null;
  const activeId = activeContact?.id ?? null;

  return (
    <div className="main" style={{ margin: '-34px -44px', height: '100vh' }}>
      <ViewHead icon="users" title={t('contacts.title')} count={contacts.length} countTone="line">
        <div style={{ flex: '1 1 auto', maxWidth: 300, marginLeft: 'var(--s2)' }}>
          <div className="input" style={{ height: 32 }}>
            <Icon name="search" size={15} className="iico" />
            <input
              type="search"
              placeholder={t('contacts.searchPlaceholder')}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              aria-label={t('contacts.searchPlaceholder')}
            />
          </div>
        </div>
        <div style={{ flex: 1 }} />
        <Button
          variant="primary"
          size="sm"
          leading={<Icon name="plus" size={14} />}
          onClick={() => setForm({ contact: null })}
          aria-label={t('contacts.addAria')}
        >
          {t('common.add')}
        </Button>
      </ViewHead>

      <div className="view-body">
        {/* ── List ── [B29.4] Канонная панель .side-list (прозрачный фон,
            240px, resize+collapse — как чаты ассистента). */}
        <aside
          className="side-list"
          data-collapsed={panel.collapsed || undefined}
          style={{ ['--side-w' as string]: `${panel.width}px` } as CSSProperties}
        >
          {panel.collapsed ? (
            // [B29.5b] Свёрнуто: expand + полоса аватаров (клик = открыть).
            <div className="side-list-mini scroll">
              <IconBtn
                icon="chevronRight"
                label={t('contacts.expandPanel')}
                tip={t('contacts.expandPanel')}
                tipSide="right"
                onClick={() => panel.setCollapsed(false)}
              />
              {filtered.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  className="avatar"
                  title={c.display_name}
                  aria-label={c.display_name}
                  aria-current={c.id === activeId ? 'true' : undefined}
                  onClick={() => setMode({ kind: 'view', contactId: c.id })}
                  style={{
                    background: c.is_owner
                      ? SP_COLORS[0]
                      : avatarColor(contacts.findIndex((x) => x.id === c.id)),
                    width: 30,
                    height: 30,
                    fontSize: 11,
                    flex: '0 0 auto',
                    cursor: 'pointer',
                    boxShadow: c.id === activeId ? '0 0 0 2px var(--accent)' : undefined,
                  }}
                >
                  {initials(c.display_name)}
                </button>
              ))}
            </div>
          ) : (
            <>
          <div className="scroll" style={{ flex: 1, minHeight: 0, padding: 6 }}>
          {filtered.length === 0 ? (
            <div className="u-faint" style={{ padding: 16, fontSize: 13, textAlign: 'center' }}>
              {contacts.length === 0 ? t('contacts.emptyTitle') : t('contacts.notFoundTitle')}
            </div>
          ) : (
            filtered.map((c, i) => {
              const primaryId = c.identifiers[0]?.value;
              const secondary = c.role ?? c.org ?? primaryId ?? (c.is_owner ? t('contacts.owner') : t('contacts.roleNone'));
              const noVoice = String(c.attributes['consent_voice'] ?? '') !== 'true' && !c.is_owner;
              return (
                <button
                  key={c.id}
                  type="button"
                  className="lrow"
                  data-active={c.id === activeId ? 'true' : undefined}
                  onClick={() => setMode({ kind: 'view', contactId: c.id })}
                >
                  <span
                    className="avatar"
                    style={{
                      background: c.is_owner ? SP_COLORS[0] : avatarColor(i),
                      width: 32,
                      height: 32,
                      fontSize: 12,
                      flex: '0 0 auto',
                    }}
                  >
                    {initials(c.display_name)}
                  </span>
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div className="u-trunc" style={{ fontWeight: 550 }}>
                      {c.display_name}
                    </div>
                    <div className="u-faint u-trunc" style={{ fontSize: 11.5 }}>
                      {secondary}
                    </div>
                  </div>
                  {noVoice && (
                    <span className="dot" style={{ background: 'var(--warn)' }} aria-hidden />
                  )}
                </button>
              );
            })
          )}
          </div>
          {/* [B30.3] Collapse — в футере (единый паттерн всех панелей). */}
          <div className="side-list-foot">
            <IconBtn
              icon="chevronLeft"
              size="sm"
              label={t('contacts.collapsePanel')}
              tip={t('contacts.collapsePanel')}
              onClick={() => panel.setCollapsed(true)}
            />
          </div>
            </>
          )}
          {!panel.collapsed && (
            <div className="panel-resize" onMouseDown={panel.onResizeStart} aria-hidden="true" />
          )}
        </aside>

        {/* ── Detail / Add / Edit ── */}
        <div className="scroll" style={{ flex: 1, minWidth: 0, padding: '32px 44px' }}>
        {error && (
          <p role="alert" style={{ color: 'var(--danger)', marginBottom: 14, fontFamily: 'var(--font)' }}>
            {error}
          </p>
        )}

        {mode.kind === 'view' && activeContact && (
          <ContactView
            contact={activeContact}
            colorIdx={contacts.findIndex((c) => c.id === activeContact.id) % SP_COLORS.length}
            stats={statsByContact.get(activeContact.id) ?? null}
            recentCalls={callsByContact.get(activeContact.id) ?? []}
            onEdit={() => setForm({ contact: activeContact })}
            onDelete={() => void handleDelete(activeContact)}
            onOpenCall={onOpenCall}
          />
        )}

        {mode.kind === 'empty' && contacts.length === 0 && (
          <Empty title={t('contacts.emptyTitle')} description={t('contacts.emptyAddCue')} />
        )}
        </div>
      </div>

      {/* [B23] Add/Edit — канонная модалка; key сбрасывает state per contact. */}
      {form && (
        <ContactFormModal
          key={form.contact?.id ?? 'new'}
          contact={form.contact}
          error={formError}
          busy={formBusy}
          onClose={() => {
            setForm(null);
            setFormError(null);
          }}
          onSubmit={(input) =>
            form.contact ? void handleUpdate(form.contact.id, input) : void handleCreate(input)
          }
        />
      )}
    </div>
  );
}

// ── Detail view ──

interface ContactViewProps {
  contact: Contact;
  colorIdx: number;
  stats: ContactStats | null;
  recentCalls: Call[];
  onEdit: () => void;
  onDelete: () => void;
  onOpenCall?: (callId: string) => void;
}

function ContactView({
  contact,
  colorIdx,
  stats,
  recentCalls,
  onEdit,
  onDelete,
  onOpenCall,
}: ContactViewProps) {
  const { t, locale } = useI18n();
  const color = contact.is_owner ? SP_COLORS[0] : SP_COLORS[colorIdx % SP_COLORS.length];
  const consentVoice = String(contact.attributes['consent_voice'] ?? '') === 'true';
  const recent = [...recentCalls]
    .sort((a, b) => +new Date(b.started_at) - +new Date(a.started_at))
    .slice(0, 5);

  return (
    <div style={{ maxWidth: 720 }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 18 }}>
        <span
          className="avatar"
          style={{ background: color, width: 56, height: 56, fontSize: 20, flex: '0 0 auto' }}
        >
          {initials(contact.display_name)}
        </span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h1 className="doc-title" style={{ fontSize: 22 }}>
            {contact.display_name}
          </h1>
          <div className="u-muted" style={{ fontSize: 13 }}>
            {contact.role ?? contact.org ?? (contact.is_owner ? t('contacts.owner') : t('contacts.roleNone'))}
            {contact.org && contact.role ? ` · ${contact.org}` : ''}
          </div>
        </div>
        {consentVoice ? (
          <span className="chip chip--ok">
            <Icon name="check" size={11} />
            {t('contacts.voiceConfirmed')}
          </span>
        ) : null}
        <Button variant="ghost" size="sm" leading={<Icon name="edit" size={14} />} onClick={onEdit}>
          {t('common.edit')}
        </Button>
        {!contact.is_owner && (
          <Button
            variant="danger-ghost"
            size="sm"
            leading={<Icon name="trash" size={14} />}
            onClick={onDelete}
          >
            {t('common.delete')}
          </Button>
        )}
      </div>

      {/* Identifier chips */}
      {contact.identifiers.length > 0 && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginBottom: 18 }}>
          {contact.identifiers.map((id) => (
            <span key={id.id} className="chip chip--line" data-selectable="">
              <Icon name={ID_ICON[id.kind] ?? 'link'} size={11} />
              {id.value}
            </span>
          ))}
        </div>
      )}

      {/* Stats */}
      <div style={{ display: 'flex', gap: 24, marginBottom: 24 }}>
        <Stat value={String(stats?.callCount ?? 0)} label={t('contacts.statCalls')} />
        <Stat value={formatRecordedDuration(stats?.totalSec ?? 0, t)} label={t('contacts.statRecorded')} />
      </div>

      {/* Recent calls */}
      {recent.length > 0 && (
        <div style={{ marginBottom: 8 }}>
          <div className="rrail-sec" style={{ marginTop: 0 }}>
            {t('contacts.recentCalls')}
          </div>
          {recent.map((call) => (
            <button
              key={call.id}
              type="button"
              className="lrow"
              onClick={() => onOpenCall?.(call.id)}
              disabled={!onOpenCall}
              style={{ cursor: onOpenCall ? 'pointer' : 'default' }}
            >
              <span className="dot" style={{ background: statusColor(call.status), flex: '0 0 auto' }} aria-hidden />
              <span className="u-trunc" style={{ flex: 1, minWidth: 0 }}>
                {call.title ?? call.id.slice(0, 8)}
              </span>
              <span className="u-faint mono" style={{ fontSize: 11.5, whiteSpace: 'nowrap' }}>
                {fmtDay(call.started_at, locale)}
              </span>
            </button>
          ))}
        </div>
      )}

      {contact.notes && (
        <div style={{ margin: '20px 0' }}>
          <div className="rrail-sec" style={{ marginTop: 0 }}>
            {t('contacts.notes')}
          </div>
          <p data-selectable="" style={{ fontSize: 14, color: 'var(--text-2)', lineHeight: 1.55, margin: 0 }}>
            {contact.notes}
          </p>
        </div>
      )}

      <div className="rrail-sec">{t('contacts.statVoiceSamples')}</div>
      <VoiceSamplesSection contactId={contact.id} alwaysShow={consentVoice} />
    </div>
  );
}

function Stat({ value, label }: { value: ReactNode; label: string }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <span style={{ fontSize: 28, fontWeight: 600 }}>{value}</span>
      <span className="u-faint" style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: '.05em' }}>
        {label}
      </span>
    </div>
  );
}

type TFn = ReturnType<typeof useI18n>['t'];

function fmtDay(iso: string, locale: string): string {
  try {
    return new Date(iso).toLocaleDateString(bcp47(locale as Parameters<typeof bcp47>[0]), {
      day: 'numeric',
      month: 'short',
    });
  } catch {
    return iso;
  }
}

function formatRecordedDuration(sec: number, t: TFn): ReactNode {
  if (sec === 0) return t('contacts.durationZero');
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  if (h === 0) return t('contacts.durationM', { m });
  if (m === 0) {
    return (
      <>
        {h}
        <span style={{ fontSize: 16, marginLeft: 4 }}>{t('contacts.durationH')}</span>
      </>
    );
  }
  return (
    <>
      {h}
      <span style={{ fontSize: 16, marginLeft: 4 }}>{t('contacts.durationHM', { m })}</span>
    </>
  );
}
