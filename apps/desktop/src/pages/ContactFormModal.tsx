// [B17, B23] Contact create/edit form — канон v2 AddContactModal
// (wk-extra.jsx:54-83): Modal 480px, grid gap 14, парные поля 1fr/1fr, footer
// ghost «Отмена» / primary submit (disabled без имени, form-атрибут — footer
// живёт вне <form> в Modal DOM). C2 (#40) consent_voice flag handled here;
// submit-shape (attrs.consent_voice='true', trim, фильтрация) сохранён
// байт-в-байт — на него завязан SpeakerConfirmModal quick-create.

import { useId, useState, type FormEvent, type ReactNode } from 'react';
import {
  IDENTIFIER_KINDS,
  type Contact,
  type ContactIdentifierInput,
  type ContactInput,
} from '../api/contacts';
import { useI18n } from '../i18n';
import {
  Button,
  Icon,
  IconBtn,
  InputField,
  Modal,
  Select,
  SettingRow,
  Switch,
  TextareaField,
} from '../ui';

interface ContactFormModalProps {
  /** null/undefined = создание нового. */
  contact?: Contact | null;
  /** [B23-fix] Ошибка сохранения — рендерится ВНУТРИ диалога (панель под
   *  оверлеем не видна). */
  error?: string | null;
  /** Сохранение в полёте — submit заблокирован (анти double-submit). */
  busy?: boolean;
  onClose: () => void;
  onSubmit: (input: ContactInput) => void;
}

const CONSENT_VOICE_KEY = 'consent_voice';

export function ContactFormModal({
  contact,
  error,
  busy,
  onClose,
  onSubmit,
}: ContactFormModalProps) {
  const { t } = useI18n();
  const formId = useId();
  const [displayName, setDisplayName] = useState(contact?.display_name ?? '');
  const [org, setOrg] = useState(contact?.org ?? '');
  const [role, setRole] = useState(contact?.role ?? '');
  const [notes, setNotes] = useState(contact?.notes ?? '');
  const [identifiers, setIdentifiers] = useState<ContactIdentifierInput[]>(
    contact?.identifiers.map((i) => ({
      kind: i.kind,
      value: i.value,
      // [B23] label проносим насквозь — diff-preserve сохранит его.
      ...(i.label != null ? { label: i.label } : {}),
    })) ?? [],
  );
  const initialAttributes = contact?.attributes ?? {};
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
    <Modal
      open
      onClose={onClose}
      title={contact ? t('contacts.editTitle') : t('contacts.newContact')}
      width={480}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            type="submit"
            form={formId}
            leading={<Icon name="check" size={14} />}
            disabled={!displayName.trim() || busy}
            busy={busy}
          >
            {contact ? t('contacts.submitSave') : t('contacts.submitCreate')}
          </Button>
        </>
      }
    >
      <form id={formId} onSubmit={submit} style={{ display: 'grid', gap: 14 }}>
        {error && (
          <p role="alert" style={{ color: 'var(--danger)', margin: 0, fontSize: 13 }}>
            {error}
          </p>
        )}
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
          <InputField
            label={t('contacts.fieldName')}
            type="text"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            autoFocus
            required
            containerStyle={{ margin: 0 }}
          />
          <InputField
            label={t('contacts.fieldRole')}
            type="text"
            value={role}
            onChange={(e) => setRole(e.target.value)}
            containerStyle={{ margin: 0 }}
          />
        </div>
        <InputField
          label={t('contacts.fieldOrg')}
          type="text"
          value={org}
          onChange={(e) => setOrg(e.target.value)}
          containerStyle={{ margin: 0 }}
        />
        <TextareaField
          label={t('contacts.fieldNotes')}
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          rows={3}
          containerStyle={{ margin: 0 }}
        />

        <RowGroup
          title={t('contacts.identifiers')}
          emptyHint={t('contacts.identifiersHint')}
          count={identifiers.length}
          onAdd={addIdentifier}
        >
          {identifiers.map((it, idx) => (
            <div key={idx} style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <div style={{ flex: '0 0 140px' }}>
                <Select
                  value={it.kind}
                  options={IDENTIFIER_KINDS.map((k) => ({
                    value: k,
                    label: t(`contacts.kind.${k}`),
                  }))}
                  onChange={(v) => setIdentifierKind(idx, v)}
                />
              </div>
              <input
                className="input input--box"
                style={{ flex: 1 }}
                type="text"
                placeholder={t('contacts.identifierValue')}
                value={it.value}
                onChange={(e) => setIdentifierValue(idx, e.target.value)}
              />
              <IconBtn
                icon="x"
                size="sm"
                label={t('contacts.removeRowAria')}
                title={t('contacts.removeRowTitle')}
                onClick={() => removeIdentifier(idx)}
              />
            </div>
          ))}
        </RowGroup>

        <RowGroup
          title={t('contacts.customFields')}
          emptyHint={t('contacts.customFieldsHint')}
          count={attributes.length}
          onAdd={addAttribute}
        >
          {attributes.map((it, idx) => (
            <div key={idx} style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <input
                className="input input--box"
                style={{ flex: '0 0 140px' }}
                type="text"
                placeholder={t('contacts.identifierKey')}
                value={it.key}
                onChange={(e) => setAttrKey(idx, e.target.value)}
              />
              <input
                className="input input--box"
                style={{ flex: 1 }}
                type="text"
                placeholder={t('contacts.identifierValue')}
                value={it.value}
                onChange={(e) => setAttrValue(idx, e.target.value)}
              />
              <IconBtn
                icon="x"
                size="sm"
                label={t('contacts.removeRowAria')}
                title={t('contacts.removeRowTitle')}
                onClick={() => removeAttribute(idx)}
              />
            </div>
          ))}
        </RowGroup>

        <SettingRow
          label={t('contacts.rememberVoiceTitle')}
          hint={t('contacts.rememberVoiceHint')}
          align="top"
          last
          control={
            <Switch
              checked={consentVoice}
              onChange={setConsentVoice}
              label={t('contacts.rememberVoiceTitle')}
            />
          }
        />
      </form>
    </Modal>
  );
}

interface RowGroupProps {
  title: string;
  emptyHint: string;
  count: number;
  onAdd: () => void;
  children: ReactNode;
}

function RowGroup({ title, emptyHint, count, onAdd, children }: RowGroupProps) {
  const { t } = useI18n();
  return (
    <div>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'baseline',
          marginBottom: 8,
        }}
      >
        {/* [B23-fix] .field-label вместо .small-caps — единый шрифт
            с лейблами остальных полей формы. */}
        <span className="field-label" style={{ marginBottom: 0 }}>
          {title}
        </span>
        <Button variant="ghost" size="sm" leading={<Icon name="plus" size={13} />} onClick={onAdd}>
          {t('contacts.addRow')}
        </Button>
      </div>
      {count === 0 ? (
        <p className="u-faint" style={{ fontSize: 12.5, margin: 0 }}>
          {emptyHint}
        </p>
      ) : (
        <div style={{ display: 'grid', gap: 8 }}>{children}</div>
      )}
    </div>
  );
}
