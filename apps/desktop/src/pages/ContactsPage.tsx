// [B17] ContactsPage — exact match per docs/design/atelier-v2/_reference/atelier-2.jsx §8.
//
// Two-column layout:
//   - Left 320px: title + "+" btn--quiet, search input, alphabet-grouped list
//     (А — М / Н — Я). Items: 30×30 sp-avatar + serif 15 name + muted role.
//     Active row: background var(--bg-2).
//   - Right (flex): "Контакт" small-caps + 76×76 avatar + display 38 name +
//     subtitle role; 3-stat row; 2-col contact fields grid; voice samples
//     table.

import { useEffect, useMemo, useState } from 'react';
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
import { Badge, Empty } from '../ui';
import { ContactForm } from './ContactForm';
import { VoiceSamplesSection } from './VoiceSamplesSection';

const SP_COLORS = ['#3D5BAB', '#2E8C5F', '#B86842', '#7958C7', '#3D87A4'];

type Mode = { kind: 'view'; contactId: string } | { kind: 'add' } | { kind: 'edit'; contactId: string } | { kind: 'empty' };

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

function alphabetBucket(name: string): 'a-m' | 'n-z' | 'other' {
  const ch = name.trim().charAt(0).toUpperCase();
  if (!ch) return 'other';
  // Cyrillic А..М (U+0410..U+041C) — split point.
  if (ch >= 'А' && ch <= 'М') return 'a-m';
  if (ch >= 'Н' && ch <= 'Я') return 'n-z';
  // Latin fallback
  if (ch >= 'A' && ch <= 'M') return 'a-m';
  if (ch >= 'N' && ch <= 'Z') return 'n-z';
  return 'other';
}

export function ContactsPage() {
  const [contacts, setContacts] = useState<Contact[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>({ kind: 'empty' });
  const [search, setSearch] = useState('');

  const refresh = () => {
    listContacts()
      .then((cs) => {
        setContacts(cs);
        // Auto-select first contact if нет active mode.
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

  const handleCreate = async (input: ContactInput) => {
    try {
      const created = await createContact(input);
      setError(null);
      const fresh = await listContacts();
      setContacts(fresh);
      setMode({ kind: 'view', contactId: created.id });
    } catch (e) {
      setError(humanError(e));
    }
  };

  const handleUpdate = async (id: string, input: ContactInput) => {
    try {
      await updateContact(id, input);
      setError(null);
      const fresh = await listContacts();
      setContacts(fresh);
      setMode({ kind: 'view', contactId: id });
    } catch (e) {
      setError(humanError(e));
    }
  };

  const handleDelete = async (c: Contact) => {
    const ok = await ask(`Удалить контакт «${c.display_name}»?`, {
      title: 'Wotold',
      kind: 'warning',
      okLabel: 'Удалить',
      cancelLabel: 'Отмена',
    });
    if (!ok) return;
    try {
      await deleteContact(c.id);
      setError(null);
      const fresh = await listContacts();
      setContacts(fresh);
      setMode(
        fresh.length > 0
          ? { kind: 'view', contactId: fresh[0]!.id }
          : { kind: 'empty' },
      );
    } catch (e) {
      setError(humanError(e));
    }
  };

  const filtered = useMemo(() => {
    if (!contacts) return [];
    const q = search.trim().toLowerCase();
    if (!q) return contacts;
    return contacts.filter((c) => {
      const hay = [
        c.display_name,
        c.org ?? '',
        c.role ?? '',
        c.notes ?? '',
        ...c.identifiers.map((i) => i.value),
      ]
        .join(' ')
        .toLowerCase();
      return hay.includes(q);
    });
  }, [contacts, search]);

  const groups = useMemo(() => {
    const am: Contact[] = [];
    const nz: Contact[] = [];
    const other: Contact[] = [];
    for (const c of filtered) {
      const bucket = alphabetBucket(c.display_name);
      if (bucket === 'a-m') am.push(c);
      else if (bucket === 'n-z') nz.push(c);
      else other.push(c);
    }
    return { am, nz, other };
  }, [filtered]);

  if (error && !contacts) {
    return (
      <p role="alert" style={{ color: 'var(--signal)', fontFamily: 'var(--font-sans)' }}>
        {error}
      </p>
    );
  }
  if (!contacts) return <p className="muted">Загрузка…</p>;

  const activeContact =
    mode.kind === 'view' || mode.kind === 'edit'
      ? contacts.find((c) => c.id === mode.contactId)
      : null;
  const activeId = activeContact?.id ?? null;

  return (
    <section
      style={{
        margin: '-34px -44px',
        display: 'flex',
        height: 'calc(100vh - 0px)',
        // Override .app-main padding to span edge-to-edge.
      }}
    >
      {/* ── List ── */}
      <div
        style={{
          width: 320,
          borderRight: '1px solid var(--line-soft)',
          padding: '32px 24px',
          overflowY: 'auto',
          flexShrink: 0,
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'baseline',
            justifyContent: 'space-between',
            marginBottom: 18,
          }}
        >
          <div className="title" style={{ fontSize: 24 }}>
            Контакты
          </div>
          <button
            type="button"
            className={`btn btn--quiet${mode.kind === 'add' ? '' : ''}`}
            onClick={() => setMode({ kind: 'add' })}
            aria-label="Добавить контакт"
            style={{ padding: 0, fontSize: 18, lineHeight: 1 }}
          >
            +
          </button>
        </div>
        <input
          className="input"
          type="search"
          placeholder="Поиск…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ marginBottom: 20, fontSize: 14 }}
        />

        {contacts.length === 0 ? (
          <Empty
            title="Контактов нет"
            description="Жми «+» — добавь первого."
          />
        ) : filtered.length === 0 ? (
          <Empty
            title="Ничего не нашлось"
            description={`По запросу «${search.trim()}» нет контактов.`}
          />
        ) : (
          <>
            <ContactGroup
              label="А — М"
              items={groups.am}
              activeId={activeId}
              onSelect={(id) => setMode({ kind: 'view', contactId: id })}
            />
            <ContactGroup
              label="Н — Я"
              items={groups.nz}
              activeId={activeId}
              onSelect={(id) => setMode({ kind: 'view', contactId: id })}
            />
            {groups.other.length > 0 && (
              <ContactGroup
                label="Прочее"
                items={groups.other}
                activeId={activeId}
                onSelect={(id) => setMode({ kind: 'view', contactId: id })}
              />
            )}
          </>
        )}
      </div>

      {/* ── Detail / Add / Edit pane ── */}
      <div
        style={{
          flex: 1,
          padding: '32px 44px',
          overflowY: 'auto',
          background: 'var(--paper)',
        }}
      >
        {error && (
          <p
            role="alert"
            style={{
              color: 'var(--signal)',
              fontFamily: 'var(--font-sans)',
              marginBottom: 14,
            }}
          >
            {error}
          </p>
        )}

        {mode.kind === 'add' && (
          <>
            <div className="small-caps" style={{ marginBottom: 12 }}>
              Новый контакт
            </div>
            <h1 className="display" style={{ fontSize: 38, marginBottom: 20 }}>
              Добавить.
            </h1>
            <div style={{ maxWidth: 640 }}>
              <ContactForm
                submitLabel="Создать"
                onSubmit={handleCreate}
                onCancel={() =>
                  setMode(
                    contacts.length > 0
                      ? { kind: 'view', contactId: contacts[0]!.id }
                      : { kind: 'empty' },
                  )
                }
              />
            </div>
          </>
        )}

        {mode.kind === 'edit' && activeContact && (
          <>
            <div className="small-caps" style={{ marginBottom: 12 }}>
              Редактирование
            </div>
            <h1 className="display" style={{ fontSize: 38, marginBottom: 20 }}>
              {activeContact.display_name}
            </h1>
            <div style={{ maxWidth: 640 }}>
              <ContactForm
                submitLabel="Сохранить"
                initial={activeContact}
                onSubmit={(input) => handleUpdate(activeContact.id, input)}
                onCancel={() =>
                  setMode({ kind: 'view', contactId: activeContact.id })
                }
              />
            </div>
          </>
        )}

        {mode.kind === 'view' && activeContact && (
          <ContactView
            contact={activeContact}
            colorIdx={
              contacts.findIndex((c) => c.id === activeContact.id) % SP_COLORS.length
            }
            onEdit={() => setMode({ kind: 'edit', contactId: activeContact.id })}
            onDelete={() => void handleDelete(activeContact)}
          />
        )}

        {mode.kind === 'empty' && contacts.length === 0 && (
          <Empty
            title="Контактов нет"
            description="Добавь первый — кнопка «+» слева сверху. Контакты помогают Wotold подписывать спикеров в расшифровках."
          />
        )}
      </div>
    </section>
  );
}

// ── List group ── -----------------------------------------------------------

interface ContactGroupProps {
  label: string;
  items: Contact[];
  activeId: string | null;
  onSelect: (id: string) => void;
}

function ContactGroup({ label, items, activeId, onSelect }: ContactGroupProps) {
  if (items.length === 0) return null;
  return (
    <>
      <div
        className="small-caps"
        style={{ marginBottom: 10, marginTop: 12 }}
      >
        {label}
      </div>
      {items.map((c, i) => {
        const isActive = c.id === activeId;
        return (
          <button
            key={c.id}
            type="button"
            onClick={() => onSelect(c.id)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 12,
              padding: '10px 8px',
              width: '100%',
              border: 'none',
              background: isActive ? 'var(--bg-2)' : 'transparent',
              borderRadius: 6,
              textAlign: 'left',
              marginBottom: 2,
              cursor: 'pointer',
              color: 'inherit',
            }}
          >
            <span
              className="sp-avatar"
              style={{
                background: c.is_owner ? SP_COLORS[0] : avatarColor(i),
                width: 30,
                height: 30,
                fontSize: 11,
              }}
            >
              {initials(c.display_name)}
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div
                style={{
                  fontFamily: 'var(--font-serif)',
                  fontSize: 15,
                  color: 'var(--ink)',
                  letterSpacing: '-0.01em',
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
              >
                {c.display_name}
              </div>
              <div
                className="muted"
                style={{
                  fontSize: 11,
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
              >
                {c.role ?? c.org ?? (c.is_owner ? 'владелец' : '—')}
              </div>
            </div>
          </button>
        );
      })}
    </>
  );
}

// ── Detail view ── ----------------------------------------------------------

interface ContactViewProps {
  contact: Contact;
  colorIdx: number;
  onEdit: () => void;
  onDelete: () => void;
}

function ContactView({ contact, colorIdx, onEdit, onDelete }: ContactViewProps) {
  const color = contact.is_owner ? SP_COLORS[0] : SP_COLORS[colorIdx % SP_COLORS.length];
  const consentVoice = String(contact.attributes['consent_voice'] ?? '') === 'true';
  return (
    <div>
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          justifyContent: 'space-between',
          gap: 12,
          marginBottom: 12,
        }}
      >
        <div className="small-caps">Контакт</div>
        <div style={{ display: 'flex', gap: 8 }}>
          <button type="button" className="btn btn--ghost btn--sm" onClick={onEdit}>
            Редактировать
          </button>
          {!contact.is_owner && (
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={onDelete}
              style={{ color: 'var(--signal)' }}
            >
              Удалить
            </button>
          )}
        </div>
      </div>

      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 22,
          marginBottom: 28,
        }}
      >
        <span
          className="sp-avatar"
          style={{
            background: color,
            width: 76,
            height: 76,
            fontSize: 22,
            borderRadius: 16,
          }}
        >
          {initials(contact.display_name)}
        </span>
        <div style={{ minWidth: 0 }}>
          <div
            className="display"
            style={{ fontSize: 38, marginBottom: 6, lineHeight: 1.05 }}
          >
            {contact.display_name}
          </div>
          <div
            className="subtitle"
            style={{ fontSize: 15, fontStyle: 'normal' }}
          >
            {contact.role ?? contact.org ?? (contact.is_owner ? 'Владелец' : '—')}
            {contact.org && contact.role && ` · ${contact.org}`}
          </div>
        </div>
        {contact.is_owner && <Badge tone="accent">владелец</Badge>}
      </div>

      <div style={{ display: 'flex', gap: 18, marginBottom: 32 }}>
        <div className="stat" style={{ padding: '0 24px 0 0' }}>
          <span className="stat-value">—</span>
          <span className="stat-label">Звонков</span>
        </div>
        <div className="stat">
          <span className="stat-value">—</span>
          <span className="stat-label">Записано</span>
        </div>
        <div className="stat">
          <span className="stat-value">
            {consentVoice ? <span style={{ color: 'var(--accent)' }}>opt-in</span> : '—'}
          </span>
          <span className="stat-label">Голосовые семплы</span>
        </div>
      </div>

      {(contact.identifiers.length > 0 ||
        Object.keys(contact.attributes).filter((k) => k !== 'consent_voice')
          .length > 0) && (
        <div style={{ marginBottom: 28 }}>
          <div className="small-caps" style={{ marginBottom: 14 }}>
            Контакты
          </div>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: '14px 32px',
            }}
          >
            {contact.identifiers.map((id) => (
              <div key={id.id}>
                <div className="small-caps" style={{ marginBottom: 2 }}>
                  {id.kind}
                </div>
                <div
                  style={{
                    fontFamily: 'var(--font-serif)',
                    fontSize: 15,
                    color: 'var(--ink)',
                  }}
                >
                  {id.value}
                </div>
              </div>
            ))}
            {Object.entries(contact.attributes)
              .filter(([k]) => k !== 'consent_voice')
              .map(([k, v]) => (
                <div key={k}>
                  <div className="small-caps" style={{ marginBottom: 2 }}>
                    {k}
                  </div>
                  <div
                    style={{
                      fontFamily: 'var(--font-serif)',
                      fontSize: 15,
                      color: 'var(--ink)',
                    }}
                  >
                    {String(v)}
                  </div>
                </div>
              ))}
          </div>
        </div>
      )}

      {contact.notes && (
        <div style={{ marginBottom: 28 }}>
          <div className="small-caps" style={{ marginBottom: 10 }}>
            Заметки
          </div>
          <p
            style={{
              fontFamily: 'var(--font-serif)',
              fontStyle: 'italic',
              fontSize: 15,
              color: 'var(--ink-2)',
              lineHeight: 1.55,
              margin: 0,
            }}
          >
            {contact.notes}
          </p>
        </div>
      )}

      <VoiceSamplesSection contactId={contact.id} alwaysShow={consentVoice} />
    </div>
  );
}
