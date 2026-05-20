// [B16] Coachmarks first-run — простой 3-step overlay рассказывающий
// где «Звонки», «Контакты», «Настройки». Не блокирует UI — modal с dim,
// dismiss через клик/ESC. Стоит на ONBOARDING_DONE=1 + COACHMARKS_SEEN!=1.

import { useEffect, useState } from 'react';
import { Button } from '../ui';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';

interface CoachStep {
  title: string;
  body: string;
}

const STEPS: CoachStep[] = [
  {
    title: '🎙 Главная — твоё рабочее место',
    body: 'Здесь жмёшь «Начать запись» когда созваниваешься. Hotkey ⌘⇧R, если кнопка не на виду. После остановки звонок попадает во вкладку «Звонки».',
  },
  {
    title: '📞 Звонки — архив с расшифровкой',
    body: 'Все записи группируются по датам. Внутри каждого звонка — три вкладки: Саммари (что обсудили), Расшифровка (полный текст), Задачи (что делать дальше).',
  },
  {
    title: '👥 Контакты — кто говорит',
    body: 'Добавляешь людей сюда, потом в звонках подтверждаешь «этот спикер = Иван». Wotold запоминает голос и сам подсказывает в следующий раз. Биометрия — только с твоего opt-in.',
  },
  {
    title: '⚙ Настройки — тонкая регулировка',
    body: 'Тут переключаешь STT/LLM-провайдеров, привязываешь свои API-ключи, видишь квоты бесплатного тарифа и можешь стереть все данные одной кнопкой.',
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
      className="coachmarks-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="coach-title"
    >
      <div className="coachmarks-card">
        <h3 id="coach-title" className="coachmarks-title">{current.title}</h3>
        <p className="coachmarks-body">{current.body}</p>

        <div className="coachmarks-progress" aria-label="Прогресс">
          {STEPS.map((_, i) => (
            <span
              key={i}
              className="coachmarks-dot"
              data-active={i === step ? 'true' : 'false'}
              data-done={i < step ? 'true' : 'false'}
            />
          ))}
        </div>

        <div className="coachmarks-actions">
          <Button variant="ghost" size="sm" onClick={() => void dismiss()}>
            Пропустить
          </Button>
          {step > 0 && (
            <Button variant="ghost" size="sm" onClick={() => setStep(step - 1)}>
              ← Назад
            </Button>
          )}
          {!isLast ? (
            <Button variant="primary" size="sm" onClick={() => setStep(step + 1)}>
              Дальше →
            </Button>
          ) : (
            <Button variant="primary" size="sm" onClick={() => void dismiss()}>
              Понятно ✓
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
