import { useState, type FormEvent } from 'react';

import { renameOwnerContact } from '../api/contacts';
import { setSetting, SETTINGS_KEYS } from '../api/settings';
import { Button, InputField } from '../ui';

interface OnboardingPageProps {
  onComplete: () => void;
}

export function OnboardingPage({ onComplete }: OnboardingPageProps) {
  const [step, setStep] = useState<1 | 2>(1);
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
      await setSetting(SETTINGS_KEYS.ONBOARDING_DONE, '1');
      onComplete();
    } catch (err) {
      setError(String(err));
      setSaving(false);
    }
  };

  return (
    <section className="onboarding">
      <div className="onboarding-card">
        {step === 1 && (
          <>
            <h1>Wotold</h1>
            <p className="text-muted">
              Десктоп-утилита для записи звонков с транскрипцией и диаризацией. Всё локально на
              твоём устройстве.
            </p>
            <ul className="onboarding-features">
              <li>Запись микрофона и системного звука раздельно</li>
              <li>Транскрипт с распознаванием спикеров</li>
              <li>Авто-рекап и список задач</li>
              <li>Локальный MCP для подключения к Claude</li>
            </ul>
            <div className="form-actions">
              <Button variant="primary" onClick={() => setStep(2)}>
                Начнём
              </Button>
            </div>
          </>
        )}
        {step === 2 && (
          <form onSubmit={submit}>
            <h1>Как тебя называть?</h1>
            <p className="hint">Имя владельца. Прикрепляется к твоей дорожке в каждой записи.</p>
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
              <Button variant="ghost" type="button" onClick={() => setStep(1)} disabled={saving}>
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
