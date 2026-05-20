import { useEffect, useState, type FormEvent } from 'react';
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

export function ContactsPage() {
  const [contacts, setContacts] = useState<Contact[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const refresh = () => {
    listContacts()
      .then(setContacts)
      .catch((e: unknown) => setError(String(e)));
  };

  useEffect(refresh, []);

  const handleCreate = async (input: ContactInput) => {
    try {
      await createContact(input);
      setShowAddForm(false);
      setError(null);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleUpdate = async (id: string, input: ContactInput) => {
    try {
      await updateContact(id, input);
      setEditingId(null);
      setError(null);
      refresh();
    } catch (e) {
      setError(String(e));
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
      setError(String(e));
    }
  };

  if (error && !contacts) return <p className="error">{error}</p>;
  if (!contacts) return <p className="hint">Загрузка…</p>;

  return (
    <section className="contacts-list">
      <div className="contacts-header">
        <h2>Контакты</h2>
        <button
          type="button"
          className="primary"
          onClick={() => {
            setShowAddForm((v) => !v);
            setEditingId(null);
          }}
        >
          {showAddForm ? 'Отмена' : '+ Добавить'}
        </button>
      </div>

      {showAddForm && (
        <ContactForm
          submitLabel="Создать"
          onSubmit={handleCreate}
          onCancel={() => setShowAddForm(false)}
        />
      )}
      {error && <p className="error">{error}</p>}

      {contacts.length === 0 ? (
        <p className="hint">Контактов нет.</p>
      ) : (
        <ul>
          {contacts.map((c) =>
            editingId === c.id ? (
              <li key={c.id}>
                <ContactForm
                  submitLabel="Сохранить"
                  initial={c}
                  onSubmit={(input) => handleUpdate(c.id, input)}
                  onCancel={() => setEditingId(null)}
                />
              </li>
            ) : (
              <li key={c.id} className={c.is_owner ? 'contact owner' : 'contact'}>
                <div className="name">
                  <button
                    type="button"
                    className="name-text edit-trigger"
                    onClick={() => {
                      setEditingId(c.id);
                      setShowAddForm(false);
                    }}
                    title="Открыть для редактирования"
                  >
                    {c.display_name}
                  </button>
                  {c.is_owner && <span className="badge">владелец</span>}
                  {!c.is_owner && (
                    <button
                      type="button"
                      className="delete"
                      title="Удалить"
                      aria-label={`Удалить ${c.display_name}`}
                      onClick={() => handleDelete(c.id, c.display_name)}
                    >
                      ×
                    </button>
                  )}
                </div>
                {(c.org ?? c.role) && (
                  <div className="meta">
                    {c.role}
                    {c.role && c.org && ' · '}
                    {c.org}
                  </div>
                )}
                {c.identifiers.length > 0 && (
                  <ul className="identifiers">
                    {c.identifiers.map((id) => (
                      <li key={id.id}>
                        <span className="kind">{id.kind}:</span> {id.value}
                      </li>
                    ))}
                  </ul>
                )}
                {Object.keys(c.attributes).length > 0 && (
                  <ul className="attributes">
                    {Object.entries(c.attributes).map(([k, v]) => (
                      <li key={k}>
                        <span className="kind">{k}:</span> {String(v)}
                      </li>
                    ))}
                  </ul>
                )}
                {c.notes && <p className="notes">{c.notes}</p>}
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

function ContactForm({ submitLabel, initial, onSubmit, onCancel }: ContactFormProps) {
  const [displayName, setDisplayName] = useState(initial?.display_name ?? '');
  const [org, setOrg] = useState(initial?.org ?? '');
  const [role, setRole] = useState(initial?.role ?? '');
  const [notes, setNotes] = useState(initial?.notes ?? '');
  const [identifiers, setIdentifiers] = useState<ContactIdentifierInput[]>(
    initial?.identifiers.map((i) => ({ kind: i.kind, value: i.value })) ?? [],
  );
  const [attributes, setAttributes] = useState<Array<{ key: string; value: string }>>(() =>
    initial?.attributes
      ? Object.entries(initial.attributes).map(([k, v]) => ({ key: k, value: String(v) }))
      : [],
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
      <label>
        Имя
        <input
          type="text"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          autoFocus
          required
        />
      </label>
      <label>
        Должность / роль
        <input type="text" value={role} onChange={(e) => setRole(e.target.value)} />
      </label>
      <label>
        Организация
        <input type="text" value={org} onChange={(e) => setOrg(e.target.value)} />
      </label>
      <label>
        Заметки
        <textarea value={notes} onChange={(e) => setNotes(e.target.value)} rows={3} />
      </label>

      <div className="row-group">
        <div className="row-group-head">
          <span>Идентификаторы</span>
          <button type="button" className="row-add" onClick={addIdentifier}>
            + строку
          </button>
        </div>
        {identifiers.length === 0 && (
          <p className="row-empty">Телефоны, мейлы, мессенджеры.</p>
        )}
        {identifiers.map((it, idx) => (
          <div key={idx} className="row">
            <select
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
              type="text"
              placeholder="значение"
              value={it.value}
              onChange={(e) => setIdentifierValue(idx, e.target.value)}
            />
            <button type="button" className="row-remove" onClick={() => removeIdentifier(idx)}>
              ×
            </button>
          </div>
        ))}
      </div>

      <div className="row-group">
        <div className="row-group-head">
          <span>Расширяемые поля</span>
          <button type="button" className="row-add" onClick={addAttribute}>
            + строку
          </button>
        </div>
        {attributes.length === 0 && (
          <p className="row-empty">Любые ключ/значение — birthday, linkedin, любые.</p>
        )}
        {attributes.map((it, idx) => (
          <div key={idx} className="row">
            <input
              type="text"
              placeholder="ключ"
              value={it.key}
              onChange={(e) => setAttrKey(idx, e.target.value)}
            />
            <input
              type="text"
              placeholder="значение"
              value={it.value}
              onChange={(e) => setAttrValue(idx, e.target.value)}
            />
            <button type="button" className="row-remove" onClick={() => removeAttribute(idx)}>
              ×
            </button>
          </div>
        ))}
      </div>

      <div className="form-actions">
        <button type="submit">{submitLabel}</button>
        <button type="button" onClick={onCancel}>
          Отмена
        </button>
      </div>
    </form>
  );
}
