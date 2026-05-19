import { useEffect, useState, type FormEvent } from 'react';
import { ask } from '@tauri-apps/plugin-dialog';

import {
  createContact,
  deleteContact,
  listContacts,
  type Contact,
  type ContactInput,
} from '../api/contacts';

export function ContactsPage() {
  const [contacts, setContacts] = useState<Contact[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);

  const refresh = () => {
    listContacts()
      .then(setContacts)
      .catch((e: unknown) => setError(String(e)));
  };

  useEffect(refresh, []);

  const handleCreate = async (input: ContactInput) => {
    try {
      await createContact(input);
      setShowForm(false);
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
        <button type="button" className="primary" onClick={() => setShowForm((v) => !v)}>
          {showForm ? 'Отмена' : '+ Добавить'}
        </button>
      </div>

      {showForm && <ContactForm onSubmit={handleCreate} onCancel={() => setShowForm(false)} />}
      {error && <p className="error">{error}</p>}

      {contacts.length === 0 ? (
        <p className="hint">Контактов нет.</p>
      ) : (
        <ul>
          {contacts.map((c) => (
            <li key={c.id} className={c.is_owner ? 'contact owner' : 'contact'}>
              <div className="name">
                <span className="name-text">{c.display_name}</span>
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
              {c.notes && <p className="notes">{c.notes}</p>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

interface ContactFormProps {
  onSubmit: (input: ContactInput) => void;
  onCancel: () => void;
}

function ContactForm({ onSubmit, onCancel }: ContactFormProps) {
  const [displayName, setDisplayName] = useState('');
  const [org, setOrg] = useState('');
  const [role, setRole] = useState('');
  const [notes, setNotes] = useState('');

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const trimmed = displayName.trim();
    if (!trimmed) return;
    onSubmit({
      display_name: trimmed,
      org: org.trim() || undefined,
      role: role.trim() || undefined,
      notes: notes.trim() || undefined,
    });
  };

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
      <div className="form-actions">
        <button type="submit">Создать</button>
        <button type="button" onClick={onCancel}>
          Отмена
        </button>
      </div>
    </form>
  );
}
