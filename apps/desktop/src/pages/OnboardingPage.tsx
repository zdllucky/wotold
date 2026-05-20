// [B17] Atelier v2 redesign per docs/design/atelier-v2/MIGRATION.md §8.
// Centred 3-4 step flow поверх `.modal-backdrop`. .display headline + .subtitle
// lede + .input boxed fields. Прогресс — дотс в правом нижнем углу.

import { useRef, useState, type FormEvent } from 'react';
import { humanError } from '../api/errors';

import { renameOwnerContact } from '../api/contacts';
import { setSetting, SETTINGS_KEYS } from '../api/settings';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { PermissionsSection } from './PermissionsSection';

interface OnboardingPageProps {
  onComplete: () => void;
}

type Step = 1 | 2 | 3 | 4;

const STEP_TOTAL = 4;
const STEP_LABEL: Record<Step, string> = {
  1: 'Знакомство',
  2: 'Разрешения',
  3: 'Согласие',
  4: 'Имя',
};

export function OnboardingPage({ onComplete }: OnboardingPageProps) {
  const [step, setStep] = useState<Step>(1);
  const [name, setName] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  // [B17] Wizard — без onClose, ESC ничего не делает (нельзя dismiss
  // onboarding в первый запуск). Focus trap чтобы Tab крутился внутри.
  useFocusTrap(rootRef, true);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return;
    setSaving(true);
    setError(null);
    try {
      await renameOwnerContact(trimmed);
      await setSetting(SETTINGS_KEYS.RECORDING_CONSENT_AT, new Date().toISOString());
      await setSetting(SETTINGS_KEYS.ONBOARDING_DONE, '1');
      onComplete();
    } catch (err) {
      setError(humanError(err));
      setSaving(false);
    }
  };

  return (
    <div
      ref={rootRef}
      className="modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="onboarding-title"
    >
      <div
        style={{
          width: 560,
          maxWidth: '90vw',
          padding: '40px 4px',
        }}
      >
        <div className="small-caps" style={{ marginBottom: 14 }}>
          Шаг 0{step} из 0{STEP_TOTAL} · {STEP_LABEL[step]}
        </div>

        {step === 1 && (
          <>
            <h1 id="onboarding-title" className="display" style={{ marginBottom: 14 }}>
              Диктофон со смыслом.
            </h1>
            <p className="subtitle" style={{ marginBottom: 30, maxWidth: 480 }}>
              Wotold записывает звонки и встречи на Mac, расшифровывает речь и
              кратко пересказывает что обсуждалось. Всё хранится локально — в
              облако ничего не утекает.
            </p>
            <ul
              style={{
                listStyle: 'none',
                padding: 0,
                margin: 0,
                display: 'flex',
                flexDirection: 'column',
                gap: 8,
                fontFamily: 'var(--font-serif)',
                fontSize: 16,
                color: 'var(--ink-2)',
              }}
            >
              <li>— Запись микрофона и системного звука раздельно</li>
              <li>— Расшифровка с распознаванием участников</li>
              <li>— Авто-саммари и список задач</li>
              <li>— Поиск по разговорам прямо в Claude через MCP</li>
            </ul>
            <FooterRow step={step} onNext={() => setStep(2)} />
          </>
        )}

        {step === 2 && (
          <>
            <h1 id="onboarding-title" className="display" style={{ marginBottom: 14 }}>
              Разрешения системы.
            </h1>
            <p className="subtitle" style={{ marginBottom: 24, maxWidth: 480 }}>
              Wotold нужны два разрешения macOS, чтобы записывать звонки. Дай
              доступ — без него запись не пойдёт.
            </p>
            <div style={{ marginBottom: 8 }}>
              <PermissionsSection />
            </div>
            <FooterRow
              step={step}
              onBack={() => setStep(1)}
              onNext={() => setStep(3)}
            />
          </>
        )}

        {step === 3 && (
          <>
            <h1 id="onboarding-title" className="display" style={{ marginBottom: 14 }}>
              Согласие на запись.
            </h1>
            <p
              className="subtitle"
              style={{
                marginBottom: 16,
                maxWidth: 480,
                fontStyle: 'normal',
              }}
            >
              Wotold будет записывать твой микрофон и звук собеседника во время
              звонков. Перед началом убедись, что собеседник предупреждён о
              записи — по закону РФ/РК запись переговоров без уведомления
              другой стороны может быть нарушением.
            </p>
            <p className="muted" style={{ marginBottom: 16, maxWidth: 480 }}>
              Записываешь под свою ответственность. Wotold не модерирует и не
              хранит контент звонков на серверах — всё локально.
            </p>
            <FooterRow
              step={step}
              onBack={() => setStep(2)}
              onNext={() => setStep(4)}
              nextLabel="Принимаю →"
            />
          </>
        )}

        {step === 4 && (
          <form onSubmit={submit}>
            <h1 id="onboarding-title" className="display" style={{ marginBottom: 14 }}>
              Как тебя называть?
            </h1>
            <p className="subtitle" style={{ marginBottom: 24, maxWidth: 480 }}>
              Имя владельца. Будет использоваться вместо «Я» в расшифровках и
              саммари.
            </p>
            <div className="field" style={{ marginBottom: 24 }}>
              <label className="field-label">Имя</label>
              <input
                type="text"
                className="input"
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoFocus
                required
                placeholder="например, Дамир"
              />
            </div>
            {error && (
              <p
                style={{
                  color: 'var(--signal)',
                  fontFamily: 'var(--font-sans)',
                  marginBottom: 16,
                }}
              >
                {error}
              </p>
            )}
            <FooterRow
              step={step}
              onBack={() => setStep(3)}
              submitDisabled={saving || !name.trim()}
              submitLabel={saving ? 'Сохраняем…' : 'Готово'}
              submit
            />
          </form>
        )}
      </div>
    </div>
  );
}

interface FooterRowProps {
  step: Step;
  onBack?: () => void;
  onNext?: () => void;
  nextLabel?: string;
  submit?: boolean;
  submitDisabled?: boolean;
  submitLabel?: string;
}

function FooterRow({
  step,
  onBack,
  onNext,
  nextLabel = 'Дальше →',
  submit,
  submitDisabled,
  submitLabel,
}: FooterRowProps) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 14,
        marginTop: 32,
        paddingTop: 24,
        borderTop: '1px solid var(--line-soft)',
      }}
    >
      {onBack && (
        <button type="button" className="btn btn--quiet" onClick={onBack}>
          ← Назад
        </button>
      )}
      {submit ? (
        <button
          type="submit"
          className="btn btn--primary"
          disabled={submitDisabled}
        >
          {submitLabel ?? 'Готово'}
        </button>
      ) : onNext ? (
        <button type="button" className="btn btn--primary" onClick={onNext}>
          {nextLabel}
        </button>
      ) : null}
      <div
        style={{
          marginLeft: 'auto',
          display: 'flex',
          gap: 6,
        }}
        aria-label={`Шаг ${step} из ${STEP_TOTAL}`}
      >
        {[1, 2, 3, 4].map((i) => (
          <span
            key={i}
            className="dot"
            style={{
              background:
                i <= step ? 'var(--accent)' : 'var(--line)',
            }}
          />
        ))}
      </div>
    </div>
  );
}
