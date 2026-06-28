// [B17] SpeakerConfirmModal — inline-popup из транскрипта когда юзер
// кликает «Кто это?» возле не-определённого спикера. Re-uses SpeakerCard
// один-в-один (та же calling-card как в табе Участники).

import { useEffect, useRef, useState } from 'react';

import { humanError } from '../api/errors';
import {
  confirmCallSpeaker,
  type CallSpeakerView,
} from '../api/speakers';
import { createContact, type Contact } from '../api/contacts';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { useI18n } from '../i18n';

import { SpeakerCard, type SpeakerSample } from './SpeakerCard';

export interface SpeakerConfirmModalProps {
  speaker: CallSpeakerView;
  contacts: Contact[];
  sample: SpeakerSample | null;
  onClose: () => void;
  /** Вызывается после успешного confirm — родитель refresh'ит данные. */
  onConfirmed: () => void;
}

export function SpeakerConfirmModal({
  speaker,
  contacts,
  sample,
  onClose,
  onConfirmed,
}: SpeakerConfirmModalProps) {
  const { t } = useI18n();
  const [pickedContactId, setPickedContactId] = useState<string>(
    speaker.suggestion_contact_id ?? '',
  );
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState('');
  const [newConsent, setNewConsent] = useState(false);
  const [busyAdd, setBusyAdd] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ref = useRef<HTMLDivElement>(null);
  useFocusTrap(ref, true, { onClose });

  useEffect(() => {
    // Перезатягиваем suggestion если speaker сменился (на случай если
    // модал переоткрывается под другой speaker).
    setPickedContactId(speaker.suggestion_contact_id ?? '');
    setAdding(false);
    setNewName('');
    setNewConsent(false);
    setError(null);
  }, [speaker.id, speaker.suggestion_contact_id]);

  const handleConfirm = async (contactId?: string) => {
    const picked = contactId ?? pickedContactId;
    if (!picked) {
      setError(t('speakers.needContactSelect'));
      return;
    }
    try {
      await confirmCallSpeaker(speaker.id, picked);
      onConfirmed();
      onClose();
    } catch (e) {
      setError(humanError(e));
    }
  };

  const handleSubmitNewContact = async () => {
    const trimmed = newName.trim();
    if (!trimmed) {
      setError(t('speakers.needContactName'));
      return;
    }
    setBusyAdd(true);
    setError(null);
    try {
      const contact = await createContact({
        display_name: trimmed,
        identifiers: [],
        attributes: newConsent ? { consent_voice: 'true' } : {},
      });
      await confirmCallSpeaker(speaker.id, contact.id);
      onConfirmed();
      onClose();
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusyAdd(false);
    }
  };

  return (
    <div
      className="overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={ref}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={t('speakers.confirmModalAria')}
        style={{ width: 'min(560px, 90vw)' }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        {error && (
          <p
            role="alert"
            style={{
              color: 'var(--danger)',
              fontFamily: 'var(--font)',
              marginBottom: 12,
              background: 'var(--danger-soft)',
              padding: '8px 14px',
              borderRadius: 'var(--r-xs)',
            }}
          >
            {error}
          </p>
        )}
        <SpeakerCard
          speaker={speaker}
          idx={0}
          total={1}
          contacts={contacts}
          sample={sample}
          pickedContactId={pickedContactId}
          onPick={setPickedContactId}
          onConfirm={(contactId) => void handleConfirm(contactId)}
          onReject={() => {
            setPickedContactId('');
          }}
          adding={adding}
          newName={newName}
          newConsent={newConsent}
          busyAdd={busyAdd}
          onStartAdd={() => {
            setAdding(true);
            setNewName('');
            setNewConsent(false);
          }}
          onCancelAdd={() => {
            setAdding(false);
            setNewName('');
            setNewConsent(false);
          }}
          onChangeNewName={setNewName}
          onChangeNewConsent={setNewConsent}
          onSubmitNewContact={() => void handleSubmitNewContact()}
        />
      </div>
    </div>
  );
}
