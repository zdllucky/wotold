import { useState, type FormEvent } from 'react';
import { humanError } from '../api/errors';

import { renameOwnerContact } from '../api/contacts';
import { setSetting, SETTINGS_KEYS } from '../api/settings';
import { Button, InputField } from '../ui';
import { PermissionsSection } from './PermissionsSection';

interface OnboardingPageProps {
  onComplete: () => void;
}

type Step = 1 | 2 | 3 | 4;

const STEPS: Array<{ id: Step; label: string }> = [
  { id: 1, label: 'Знакомство' },
  { id: 2, label: 'Разрешения' },
  { id: 3, label: 'Согласие' },
  { id: 4, label: 'Имя' },
];

export function OnboardingPage({ onComplete }: OnboardingPageProps) {
  const [step, setStep] = useState<Step>(1);
  const [name, setName] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return;
    setSaving(true);
    setError(null);
    try {
      await renameOwnerContact(trimmed);
      // [B16]: consent сохраняется здесь же — show on home flow убираем.
      await setSetting(SETTINGS_KEYS.RECORDING_CONSENT_AT, new Date().toISOString());
      await setSetting(SETTINGS_KEYS.ONBOARDING_DONE, '1');
      onComplete();
    } catch (err) {
      setError(humanError(err));
      setSaving(false);
    }
  };

  return (
    <section className="onboarding">
      <div className="onboarding-card">
        <ol className="onboarding-steps" aria-label="Прогресс настройки">
          {STEPS.map((s) => (
            <li
              key={s.id}
              className="onboarding-step-dot"
              data-active={s.id === step ? 'true' : 'false'}
              data-done={s.id < step ? 'true' : 'false'}
              title={s.label}
            />
          ))}
        </ol>

        {step === 1 && (
          <>
            <div className="onboarding-hero-icon" aria-hidden>
              <span className="onboarding-hero-emoji">🎙</span>
            </div>
            <h1>Wotold — твой диктофон со смыслом</h1>
            <p className="text-muted">
              Записывает звонки и встречи на твоём Mac, расшифровывает речь и
              кратко пересказывает что обсуждалось. Все звонки хранятся
              локально — в облако ничего не утекает.
            </p>
            <ul className="onboarding-features">
              <li>Запись микрофона и системного звука раздельно</li>
              <li>Расшифровка с распознаванием участников</li>
              <li>Авто-саммари и список задач</li>
              <li>Поиск по разговорам прямо в Claude через MCP</li>
            </ul>
            <div className="form-actions">
              <Button variant="primary" onClick={() => setStep(2)}>
                Дальше →
              </Button>
            </div>
          </>
        )}

        {step === 2 && (
          <>
            <h1>Разрешения системы</h1>
            <p className="text-muted">
              Wotold нужны два разрешения macOS, чтобы записывать звонки.
              Дай доступ — без него запись не пойдёт.
            </p>
            <PermissionsSection />
            <div className="form-actions">
              <Button variant="ghost" type="button" onClick={() => setStep(1)} disabled={saving}>
                Назад
              </Button>
              <Button variant="primary" onClick={() => setStep(3)}>
                Готово, дальше →
              </Button>
            </div>
          </>
        )}

        {step === 3 && (
          <>
            <h1>Согласие на запись</h1>
            <p className="text-muted">
              Wotold будет записывать твой микрофон и звук собеседника во время
              звонков. Перед началом убедись, что собеседник предупреждён о записи —
              по закону РФ/РК запись переговоров без уведомления другой стороны может
              быть нарушением (статьи о тайне коммуникаций / неприкосновенности
              частной жизни).
            </p>
            <p className="text-muted">
              Записываешь под свою ответственность. Wotold не модерирует и не
              хранит контент звонков на серверах — всё локально.
            </p>
            <div className="form-actions">
              <Button variant="ghost" type="button" onClick={() => setStep(2)} disabled={saving}>
                Назад
              </Button>
              <Button variant="primary" onClick={() => setStep(4)}>
                Принимаю →
              </Button>
            </div>
          </>
        )}

        {step === 4 && (
          <form onSubmit={submit}>
            <h1>Как тебя называть?</h1>
            <p className="hint">
              Имя владельца. Будет использоваться вместо «Я» в расшифровках и саммари.
            </p>
            <InputField
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
              required
              placeholder="Имя"
            />
            {error && <p className="error">{error}</p>}
            <div className="form-actions">
              <Button variant="ghost" type="button" onClick={() => setStep(3)} disabled={saving}>
                Назад
              </Button>
              <Button variant="primary" type="submit" disabled={saving || !name.trim()} busy={saving}>
                {saving ? 'Сохраняем…' : 'Готово'}
              </Button>
            </div>
          </form>
        )}
      </div>
    </section>
  );
}
