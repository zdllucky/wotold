import { useEffect, useState, type FormEvent } from 'react';
import { humanError } from '../api/errors';
import { ask } from '@tauri-apps/plugin-dialog';

import {
  createContact,
  deleteContact,
  listContacts,
  updateContact,
  IDENTIFIER_KINDS,
  type Contact,
  type ContactIdentifierInput,
  type ContactInput,
} from '../api/contacts';
import { Badge, Button, Empty, InputField, TextareaField } from '../ui';
import { VoiceSamplesSection } from './VoiceSamplesSection';

function initials(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return '·';
  const parts = trimmed.split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]?.toUpperCase() ?? '').join('') || '·';
}

function avatarColor(idx: number): string {
  return `var(--sp-${(idx % 5) + 1})`;
}

export function ContactsPage() {
  const [contacts, setContacts] = useState<Contact[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  // [B16 audit P1] Search по имени/identifiers/org/role. На 50+ контактах
  // scroll становится бесполезным.
  const [search, setSearch] = useState('');

  const refresh = () => {
    listContacts()
      .then(setContacts)
      .catch((e: unknown) => setError(humanError(e)));
  };

  useEffect(refresh, []);

  const handleCreate = async (input: ContactInput) => {
    try {
      await createContact(input);
      setShowAddForm(false);
      setError(null);
      refresh();
    } catch (e) {
      setError(humanError(e));
    }
  };

  const handleUpdate = async (id: string, input: ContactInput) => {
    try {
      await updateContact(id, input);
      setEditingId(null);
      setError(null);
      refresh();
    } catch (e) {
      setError(humanError(e));
    }
  };

  const handleDelete = async (id: string, name: string) => {
    const ok = await ask(`Удалить контакт «${name}»?`, {
      title: 'Wotold',
      kind: 'warning',
      okLabel: 'Удалить',
      cancelLabel: 'Отмена',
    });
    if (!ok) return;
    try {
      await deleteContact(id);
      setError(null);
      refresh();
    } catch (e) {
      setError(humanError(e));
    }
  };

  if (error && !contacts) return <p className="error">{error}</p>;
  if (!contacts) return <p className="hint">Загрузка…</p>;

  // [B16] Search фильтр — name / org / role / identifiers / notes.
  const q = search.trim().toLowerCase();
  const filtered = !q
    ? contacts
    : contacts.filter((c) => {
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

  return (
    <section>
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-end',
          gap: 24,
          marginBottom: 26,
          flexWrap: 'wrap',
        }}
      >
        <h1 className="title" style={{ fontSize: 36, margin: 0 }}>
          Контакты
        </h1>
        <div style={{ flex: 1 }} />
        <button
          type="button"
          className={`btn ${showAddForm ? 'btn--ghost' : 'btn--primary'}`}
          onClick={() => {
            setShowAddForm((v) => !v);
            setEditingId(null);
          }}
        >
          {showAddForm ? 'Отмена' : '+ Добавить'}
        </button>
      </div>

      {error && (
        <p
          style={{
            color: 'var(--signal)',
            fontFamily: 'var(--font-sans)',
            marginBottom: 14,
          }}
        >
          {error}
        </p>
      )}

      {showAddForm && (
        <div className="card" style={{ marginBottom: 24 }}>
          <ContactForm
            submitLabel="Создать"
            onSubmit={handleCreate}
            onCancel={() => setShowAddForm(false)}
          />
        </div>
      )}

      {contacts.length > 5 && (
        <div style={{ marginBottom: 24, display: 'flex', alignItems: 'baseline', gap: 16 }}>
          <input
            className="input"
            type="search"
            placeholder="Поиск по имени, организации, телефону…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            aria-label="Поиск контактов"
            style={{ flex: 1 }}
          />
          {q && (
            <span className="small-caps">
              {filtered.length} из {contacts.length}
            </span>
          )}
        </div>
      )}

      {contacts.length === 0 ? (
        <Empty
          title="Контактов нет"
          description="Добавь первый контакт — кнопка «+ Добавить» сверху справа. Контакты помогают Wotold подписывать спикеров в расшифровках."
        />
      ) : filtered.length === 0 ? (
        <Empty
          title="Ничего не нашлось"
          description={`По запросу «${search.trim()}» нет контактов.`}
        />
      ) : (
        <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
          {filtered.map((c, idx) =>
            editingId === c.id ? (
              <li key={c.id} style={{ marginBottom: 18 }}>
                <div className="card">
                  <ContactForm
                    submitLabel="Сохранить"
                    initial={c}
                    onSubmit={(input) => handleUpdate(c.id, input)}
                    onCancel={() => setEditingId(null)}
                  />
                </div>
              </li>
            ) : (
              <li
                key={c.id}
                style={{
                  display: 'grid',
                  gridTemplateColumns: '44px 1fr auto',
                  gap: 16,
                  padding: '16px 0',
                  borderTop: idx === 0 ? 'none' : '1px solid var(--line-soft)',
                  alignItems: 'start',
                }}
              >
                <button
                  type="button"
                  className="sp-avatar"
                  style={{
                    background: c.is_owner ? 'var(--sp-1)' : avatarColor(idx),
                    width: 38,
                    height: 38,
                    fontSize: 12,
                    border: 'none',
                    cursor: 'pointer',
                  }}
                  onClick={() => {
                    setEditingId(c.id);
                    setShowAddForm(false);
                  }}
                  title="Открыть"
                >
                  {initials(c.display_name)}
                </button>
                <div style={{ minWidth: 0 }}>
                  <div
                    style={{
                      display: 'flex',
                      gap: 10,
                      alignItems: 'baseline',
                      flexWrap: 'wrap',
                    }}
                  >
                    <button
                      type="button"
                      onClick={() => {
                        setEditingId(c.id);
                        setShowAddForm(false);
                      }}
                      style={{
                        background: 'none',
                        border: 'none',
                        padding: 0,
                        cursor: 'pointer',
                        fontFamily: 'var(--font-serif)',
                        fontSize: 18,
                        color: 'var(--ink)',
                        textAlign: 'left',
                      }}
                      title="Открыть для редактирования"
                    >
                      {c.display_name}
                    </button>
                    {c.is_owner && <Badge tone="accent">владелец</Badge>}
                  </div>
                  {(c.org ?? c.role) && (
                    <div
                      className="muted"
                      style={{
                        fontSize: 13,
                        marginTop: 2,
                      }}
                    >
                      {c.role}
                      {c.role && c.org && ' · '}
                      {c.org}
                    </div>
                  )}
                  {(c.identifiers.length > 0 ||
                    Object.keys(c.attributes).length > 0) && (
                    <div
                      style={{
                        display: 'flex',
                        gap: 14,
                        flexWrap: 'wrap',
                        marginTop: 6,
                        fontSize: 12,
                      }}
                    >
                      {c.identifiers.map((id) => (
                        <span key={id.id} className="muted">
                          <span className="small-caps" style={{ fontSize: 10 }}>
                            {id.kind}
                          </span>{' '}
                          <span className="mono">{id.value}</span>
                        </span>
                      ))}
                      {Object.entries(c.attributes).map(([k, v]) => (
                        <span key={k} className="muted">
                          <span className="small-caps" style={{ fontSize: 10 }}>
                            {k}
                          </span>{' '}
                          {String(v)}
                        </span>
                      ))}
                    </div>
                  )}
                  {c.notes && (
                    <p
                      className="muted"
                      style={{
                        fontStyle: 'italic',
                        fontFamily: 'var(--font-serif)',
                        marginTop: 8,
                        fontSize: 14,
                      }}
                    >
                      {c.notes}
                    </p>
                  )}
                </div>
                {!c.is_owner && (
                  <button
                    type="button"
                    className="btn btn--quiet"
                    title="Удалить"
                    aria-label={`Удалить ${c.display_name}`}
                    onClick={() => handleDelete(c.id, c.display_name)}
                    style={{ alignSelf: 'start' }}
                  >
                    ×
                  </button>
                )}
              </li>
            ),
          )}
        </ul>
      )}
    </section>
  );
}

interface ContactFormProps {
  submitLabel: string;
  initial?: Contact;
  onSubmit: (input: ContactInput) => void;
  onCancel: () => void;
}

// C2 (#40): ключ в contact.attributes для opt-in на накопление voice samples.
const CONSENT_VOICE_KEY = 'consent_voice';

function ContactForm({ submitLabel, initial, onSubmit, onCancel }: ContactFormProps) {
  const [displayName, setDisplayName] = useState(initial?.display_name ?? '');
  const [org, setOrg] = useState(initial?.org ?? '');
  const [role, setRole] = useState(initial?.role ?? '');
  const [notes, setNotes] = useState(initial?.notes ?? '');
  const [identifiers, setIdentifiers] = useState<ContactIdentifierInput[]>(
    initial?.identifiers.map((i) => ({ kind: i.kind, value: i.value })) ?? [],
  );
  // C2 (#40): био-opt-in отделён от свободных attributes, чтобы юзер видел его
  // как 1st-class флаг с правовым контекстом, не как «ещё одна пара ключ/значение».
  const initialAttributes = initial?.attributes ?? {};
  const [consentVoice, setConsentVoice] = useState<boolean>(
    String(initialAttributes[CONSENT_VOICE_KEY] ?? '') === 'true',
  );
  const [attributes, setAttributes] = useState<Array<{ key: string; value: string }>>(() =>
    Object.entries(initialAttributes)
      .filter(([k]) => k !== CONSENT_VOICE_KEY)
      .map(([k, v]) => ({ key: k, value: String(v) })),
  );

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const trimmed = displayName.trim();
    if (!trimmed) return;

    const attrs: Record<string, string> = {};
    for (const { key, value } of attributes) {
      const k = key.trim();
      const v = value.trim();
      if (k && v) attrs[k] = v;
    }
    if (consentVoice) {
      attrs[CONSENT_VOICE_KEY] = 'true';
    }

    onSubmit({
      display_name: trimmed,
      org: org.trim() || undefined,
      role: role.trim() || undefined,
      notes: notes.trim() || undefined,
      identifiers: identifiers.filter((i) => i.kind.trim() && i.value.trim()),
      attributes: attrs,
    });
  };

  const addIdentifier = () =>
    setIdentifiers((arr) => [...arr, { kind: 'phone', value: '' }]);
  const removeIdentifier = (idx: number) =>
    setIdentifiers((arr) => arr.filter((_, i) => i !== idx));
  const setIdentifierKind = (idx: number, kind: string) =>
    setIdentifiers((arr) => arr.map((it, i) => (i === idx ? { ...it, kind } : it)));
  const setIdentifierValue = (idx: number, value: string) =>
    setIdentifiers((arr) => arr.map((it, i) => (i === idx ? { ...it, value } : it)));

  const addAttribute = () => setAttributes((arr) => [...arr, { key: '', value: '' }]);
  const removeAttribute = (idx: number) =>
    setAttributes((arr) => arr.filter((_, i) => i !== idx));
  const setAttrKey = (idx: number, key: string) =>
    setAttributes((arr) => arr.map((it, i) => (i === idx ? { ...it, key } : it)));
  const setAttrValue = (idx: number, value: string) =>
    setAttributes((arr) => arr.map((it, i) => (i === idx ? { ...it, value } : it)));

  return (
    <form className="contact-form" onSubmit={submit}>
      <InputField
        label="Имя"
        type="text"
        value={displayName}
        onChange={(e) => setDisplayName(e.target.value)}
        autoFocus
        required
      />
      <InputField
        label="Должность / роль"
        type="text"
        value={role}
        onChange={(e) => setRole(e.target.value)}
      />
      <InputField
        label="Организация"
        type="text"
        value={org}
        onChange={(e) => setOrg(e.target.value)}
      />
      <TextareaField
        label="Заметки"
        value={notes}
        onChange={(e) => setNotes(e.target.value)}
        rows={3}
      />

      {/* C2 (#40): био-opt-in. Без этого флага matching pipeline (#25/#26) не пишет
          voice_samples даже после ручного подтверждения спикера. */}
      <label className="consent-row">
        <input
          type="checkbox"
          checked={consentVoice}
          onChange={(e) => setConsentVoice(e.target.checked)}
        />
        <span className="consent-row-text">
          <strong>Запоминать голос для авто-определения</strong>
          <span className="consent-row-hint">
            При подтверждении этого человека в звонке Wotold сохранит
            короткий образец голоса — чтобы в будущем определять его
            автоматически. Сними галку, чтобы отключить.
          </span>
        </span>
      </label>

      <div className="row-group">
        <div className="row-group-head">
          <span className="row-group-title">Идентификаторы</span>
          <Button type="button" size="sm" variant="ghost" onClick={addIdentifier}>
            + строку
          </Button>
        </div>
        {identifiers.length === 0 && (
          <p className="row-empty">Телефоны, мейлы, мессенджеры.</p>
        )}
        {identifiers.map((it, idx) => (
          <div key={idx} className="row">
            <select
              className="ds-select"
              value={it.kind}
              onChange={(e) => setIdentifierKind(idx, e.target.value)}
            >
              {IDENTIFIER_KINDS.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </select>
            <input
              className="ds-input"
              type="text"
              placeholder="значение"
              value={it.value}
              onChange={(e) => setIdentifierValue(idx, e.target.value)}
            />
            <Button
              type="button"
              variant="danger"
              size="sm"
              onClick={() => removeIdentifier(idx)}
              aria-label="Удалить строку"
            >
              ×
            </Button>
          </div>
        ))}
      </div>

      <div className="row-group">
        <div className="row-group-head">
          <span className="row-group-title">Расширяемые поля</span>
          <Button type="button" size="sm" variant="ghost" onClick={addAttribute}>
            + строку
          </Button>
        </div>
        {attributes.length === 0 && (
          <p className="row-empty">Любые ключ/значение — birthday, linkedin, любые.</p>
        )}
        {attributes.map((it, idx) => (
          <div key={idx} className="row">
            <input
              className="ds-input"
              type="text"
              placeholder="ключ"
              value={it.key}
              onChange={(e) => setAttrKey(idx, e.target.value)}
            />
            <input
              className="ds-input"
              type="text"
              placeholder="значение"
              value={it.value}
              onChange={(e) => setAttrValue(idx, e.target.value)}
            />
            <Button
              type="button"
              variant="danger"
              size="sm"
              onClick={() => removeAttribute(idx)}
              aria-label="Удалить строку"
            >
              ×
            </Button>
          </div>
        ))}
      </div>

      {/* #45 (M3.6): voice samples view + manual delete — только в режиме
          редактирования существующего контакта. У нового контакта семплов
          ещё нет. */}
      {initial && <VoiceSamplesSection contactId={initial.id} alwaysShow={consentVoice} />}

      <div className="form-actions">
        <Button type="button" variant="ghost" onClick={onCancel}>
          Отмена
        </Button>
        <Button type="submit" variant="primary">
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}
