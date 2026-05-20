// [B16] Coachmarks first-run — простой 4-step overlay рассказывающий
// где «Звонки», «Контакты», «Настройки». Не блокирует UI — modal с dim,
// dismiss через клик/ESC. Стоит на ONBOARDING_DONE=1 + COACHMARKS_SEEN!=1.
//
// [B17] Atelier v2 — .modal-backdrop + .index-card per docs/design/atelier-v2.
// Эмодзи в заголовках убраны (handoff: text carries enough signal).

import { useEffect, useState } from 'react';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';

interface CoachStep {
  eyebrow: string;
  title: string;
  body: string;
}

const STEPS: CoachStep[] = [
  {
    eyebrow: 'Шаг 01',
    title: 'Главная',
    body: 'Жмёшь красный кружок когда созваниваешься. Hotkey ⌘⇧R, если кнопка не на виду. После остановки звонок попадает во вкладку «Звонки».',
  },
  {
    eyebrow: 'Шаг 02',
    title: 'Звонки',
    body: 'Все записи группируются по датам. Внутри каждого звонка — четыре вкладки: Саммари, Расшифровка, Задачи, Участники.',
  },
  {
    eyebrow: 'Шаг 03',
    title: 'Контакты',
    body: 'Добавляешь людей сюда, потом в звонках подтверждаешь «этот спикер = Иван». Wotold запоминает голос и подсказывает в следующий раз. Биометрия — только с opt-in.',
  },
  {
    eyebrow: 'Шаг 04',
    title: 'Настройки',
    body: 'Переключаешь STT/LLM-провайдеров, привязываешь свои ключи, видишь квоты тарифа и можешь стереть все данные одной кнопкой. Там же — тема и акцент.',
  },
];

export function Coachmarks() {
  const [visible, setVisible] = useState(false);
  const [step, setStep] = useState(0);

  useEffect(() => {
    (async () => {
      try {
        const onboardingDone = await getSetting(SETTINGS_KEYS.ONBOARDING_DONE);
        const coachmarksSeen = await getSetting(SETTINGS_KEYS.COACHMARKS_SEEN);
        if (onboardingDone === '1' && coachmarksSeen !== '1') {
          setVisible(true);
        }
      } catch (e) {
        console.warn('coachmarks check failed', e);
      }
    })();
  }, []);

  const dismiss = async () => {
    setVisible(false);
    try {
      await setSetting(SETTINGS_KEYS.COACHMARKS_SEEN, '1');
    } catch (e) {
      console.warn('coachmarks save failed', e);
    }
  };

  useEffect(() => {
    if (!visible) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') void dismiss();
      if (e.key === 'ArrowRight' && step < STEPS.length - 1) setStep(step + 1);
      if (e.key === 'ArrowLeft' && step > 0) setStep(step - 1);
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, step]);

  if (!visible) return null;
  const current = STEPS[step]!;
  const isLast = step === STEPS.length - 1;

  return (
    <div
      className="modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="coach-title"
    >
      <div className="index-card">
        <div className="small-caps" style={{ marginBottom: 10 }}>
          {current.eyebrow} из 0{STEPS.length}
        </div>
        <h3
          id="coach-title"
          className="title"
          style={{ fontSize: 28, marginBottom: 14 }}
        >
          {current.title}
        </h3>
        <p
          style={{
            fontFamily: 'var(--font-serif)',
            fontSize: 17,
            lineHeight: 1.55,
            color: 'var(--ink-2)',
            marginBottom: 28,
          }}
        >
          {current.body}
        </p>

        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            paddingTop: 20,
            borderTop: '1px solid var(--line-soft)',
          }}
        >
          <button
            type="button"
            className="btn btn--quiet"
            onClick={() => void dismiss()}
          >
            Пропустить
          </button>
          {step > 0 && (
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => setStep(step - 1)}
            >
              ← Назад
            </button>
          )}
          {!isLast ? (
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => setStep(step + 1)}
            >
              Дальше →
            </button>
          ) : (
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => void dismiss()}
            >
              Понятно ✓
            </button>
          )}
          <div
            style={{
              marginLeft: 'auto',
              display: 'flex',
              gap: 6,
            }}
            aria-label="Прогресс"
          >
            {STEPS.map((_, i) => (
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
      </div>
    </div>
  );
}
