// [B24.4] Композер вопроса — `composer composer-ask ai-field` (одно из трёх
// ai-field-полей по SPEC §6). Управляемый локальный draft; submit по Enter
// и по кнопке send.

import { useState, type FormEvent } from 'react';

import { useI18n } from '../../i18n';
import { Icon, IconBtn, type IconName } from '../../ui';

export interface AssistantComposerProps {
  placeholder: string;
  /** search — раздел; sparkle — вкладка звонка (мок wk2-app/screens). */
  icon: Extract<IconName, 'search' | 'sparkle'>;
  disabled?: boolean;
  onAsk: (question: string) => void;
}

export function AssistantComposer({ placeholder, icon, disabled, onAsk }: AssistantComposerProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState('');

  // Единая точка отправки: form submit, Enter в input, кнопка send.
  const submitDraft = () => {
    const q = draft.trim();
    if (!q || disabled) return;
    onAsk(q);
    setDraft('');
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    submitDraft();
  };

  return (
    <form className="composer composer-ask ai-field" onSubmit={onSubmit}>
      <Icon
        name={icon}
        size={16}
        style={{
          color: icon === 'sparkle' ? 'var(--accent-text)' : 'var(--text-3)',
          flex: '0 0 auto',
        }}
      />
      <input
        placeholder={placeholder}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          // В форме нет type=submit кнопки (IconBtn = type=button) —
          // implicit submission не гарантирован, шлём Enter вручную.
          // IME-набор (isComposing) не отправляем.
          if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
            e.preventDefault();
            submitDraft();
          }
        }}
        disabled={disabled}
      />
      <IconBtn
        icon="send"
        active={draft.trim().length > 0}
        label={t('assistant.sendLabel')}
        disabled={disabled}
        onClick={submitDraft}
      />
    </form>
  );
}
