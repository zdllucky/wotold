// [B24.4] Композер вопроса — `composer composer-ask ai-field` (одно из трёх
// ai-field-полей по SPEC §6). Управляемый локальный draft; submit по Enter
// и по кнопке send.
// [B27.7] Многострочность: textarea auto-grow 1→~6 строк (дальше scroll),
// Enter = отправка, Shift+Enter = перенос строки. field-sizing:content в
// WKWebView нет — высота пересчитывается по scrollHeight.

import { useRef, useState, type FormEvent } from 'react';

import { useI18n } from '../../i18n';
import { Icon, IconBtn, type IconName } from '../../ui';

export interface AssistantComposerProps {
  placeholder: string;
  /** search — раздел; sparkle — вкладка звонка (мок wk2-app/screens). */
  icon: Extract<IconName, 'search' | 'sparkle'>;
  disabled?: boolean;
  onAsk: (question: string) => void;
}

const TA_MAX_HEIGHT = 132; // ≈6 строк при line-height 1.5 × t-14

export function AssistantComposer({ placeholder, icon, disabled, onAsk }: AssistantComposerProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState('');
  const taRef = useRef<HTMLTextAreaElement | null>(null);

  const autoGrow = () => {
    const el = taRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, TA_MAX_HEIGHT)}px`;
  };

  // Единая точка отправки: form submit, Enter в textarea, кнопка send.
  const submitDraft = () => {
    const q = draft.trim();
    if (!q || disabled) return;
    onAsk(q);
    setDraft('');
    const el = taRef.current;
    if (el) el.style.height = 'auto';
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
      <textarea
        ref={taRef}
        rows={1}
        placeholder={placeholder}
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
          autoGrow();
        }}
        onKeyDown={(e) => {
          // Enter отправляет (у textarea implicit submission нет вовсе),
          // Shift+Enter — нативный перенос. IME-набор (isComposing) не шлём.
          if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
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
