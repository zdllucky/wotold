// [B17] Contact create/edit form — extracted from ContactsPage. C2 (#40)
// consent_voice flag handled here.

import {
  useState,
  type FormEvent,
  type ReactNode,
} from 'react';
import {
  IDENTIFIER_KINDS,
  type Contact,
  type ContactIdentifierInput,
  type ContactInput,
} from '../api/contacts';
import { InputField, Select, TextareaField } from '../ui';
import { VoiceSamplesSection } from './VoiceSamplesSection';

interface ContactFormProps {
  submitLabel: string;
  initial?: Contact;
  onSubmit: (input: ContactInput) => void;
  onCancel: () => void;
}

const CONSENT_VOICE_KEY = 'consent_voice';

export function ContactForm({
  submitLabel,
  initial,
  onSubmit,
  onCancel,
}: ContactFormProps) {
  const [displayName, setDisplayName] = useState(initial?.display_name ?? '');
  const [org, setOrg] = useState(initial?.org ?? '');
  const [role, setRole] = useState(initial?.role ?? '');
  const [notes, setNotes] = useState(initial?.notes ?? '');
  const [identifiers, setIdentifiers] = useState<ContactIdentifierInput[]>(
    initial?.identifiers.map((i) => ({ kind: i.kind, value: i.value })) ?? [],
  );
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
    if (consentVoice) attrs[CONSENT_VOICE_KEY] = 'true';
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
    <form onSubmit={submit} style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
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

      <label
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          gap: 10,
          padding: '10px 0',
          margin: '6px 0',
        }}
      >
        <input
          type="checkbox"
          checked={consentVoice}
          onChange={(e) => setConsentVoice(e.target.checked)}
          style={{ marginTop: 4 }}
        />
        <span
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 4,
            fontSize: 14,
            color: 'var(--ink)',
          }}
        >
          <strong style={{ fontWeight: 500 }}>
            Запоминать голос для авто-определения
          </strong>
          <span className="muted" style={{ fontSize: 12, lineHeight: 1.45 }}>
            При подтверждении этого человека в звонке Wotold сохранит короткий
            образец голоса — чтобы в будущем определять его автоматически. Сними
            галку, чтобы отключить.
          </span>
        </span>
      </label>

      <RowGroup
        title="Идентификаторы"
        emptyHint="Телефоны, мейлы, мессенджеры."
        items={identifiers}
        onAdd={addIdentifier}
        renderItem={(it, idx) => (
          <>
            <div style={{ flex: '0 0 140px' }}>
              <Select
                value={it.kind}
                options={IDENTIFIER_KINDS.map((k) => ({ value: k, label: k }))}
                onChange={(v) => setIdentifierKind(idx, v)}
              />
            </div>
            <input
              className="input input--box"
              style={{ flex: 1 }}
              type="text"
              placeholder="значение"
              value={it.value}
              onChange={(e) => setIdentifierValue(idx, e.target.value)}
            />
            <button
              type="button"
              className="btn btn--quiet"
              onClick={() => removeIdentifier(idx)}
              aria-label="Удалить строку"
              title="Удалить"
            >
              ×
            </button>
          </>
        )}
      />

      <RowGroup
        title="Расширяемые поля"
        emptyHint="Любые ключ/значение — birthday, linkedin, любые."
        items={attributes}
        onAdd={addAttribute}
        renderItem={(it, idx) => (
          <>
            <input
              className="input input--box"
              style={{ flex: '0 0 140px' }}
              type="text"
              placeholder="ключ"
              value={it.key}
              onChange={(e) => setAttrKey(idx, e.target.value)}
            />
            <input
              className="input input--box"
              style={{ flex: 1 }}
              type="text"
              placeholder="значение"
              value={it.value}
              onChange={(e) => setAttrValue(idx, e.target.value)}
            />
            <button
              type="button"
              className="btn btn--quiet"
              onClick={() => removeAttribute(idx)}
              aria-label="Удалить строку"
              title="Удалить"
            >
              ×
            </button>
          </>
        )}
      />

      {initial && (
        <VoiceSamplesSection contactId={initial.id} alwaysShow={consentVoice} />
      )}

      <div
        style={{
          display: 'flex',
          gap: 10,
          marginTop: 18,
          paddingTop: 14,
          borderTop: '1px solid var(--line-soft)',
        }}
      >
        <button type="button" className="btn btn--ghost" onClick={onCancel}>
          Отмена
        </button>
        <button type="submit" className="btn btn--primary">
          {submitLabel}
        </button>
      </div>
    </form>
  );
}

interface RowGroupProps<T> {
  title: string;
  emptyHint: string;
  items: T[];
  onAdd: () => void;
  renderItem: (it: T, idx: number) => ReactNode;
}

function RowGroup<T>({ title, emptyHint, items, onAdd, renderItem }: RowGroupProps<T>) {
  return (
    <div style={{ marginTop: 12 }}>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'baseline',
          marginBottom: 8,
        }}
      >
        <span className="small-caps">{title}</span>
        <button
          type="button"
          className="btn btn--quiet btn--sm"
          onClick={onAdd}
          style={{ padding: '4px 8px' }}
        >
          + строку
        </button>
      </div>
      {items.length === 0 && (
        <p
          className="muted"
          style={{
            fontStyle: 'italic',
            fontFamily: 'var(--font-serif)',
            fontSize: 14,
            margin: 0,
          }}
        >
          {emptyHint}
        </p>
      )}
      {items.map((it, idx) => (
        <div
          key={idx}
          style={{
            display: 'flex',
            gap: 8,
            alignItems: 'center',
            marginTop: 8,
          }}
        >
          {renderItem(it, idx)}
        </div>
      ))}
    </div>
  );
}
