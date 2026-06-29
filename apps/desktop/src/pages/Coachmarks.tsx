// [B16] Coachmarks first-run — простой 4-step overlay рассказывающий
// где «Звонки», «Контакты», «Настройки». Не блокирует UI — modal с dim,
// dismiss через клик/ESC. Стоит на ONBOARDING_DONE=1 + COACHMARKS_SEEN!=1.
//
// [B18.7a] Wotold v2 (uikit) — .overlay + .modal per docs/design/wotold-v2.
// role/aria + focus-trap ref живут на внутреннем .modal (a11y fix).
// Эмодзи в заголовках убраны (handoff: text carries enough signal).

import { useEffect, useRef, useState } from 'react';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { useI18n } from '../i18n';

interface CoachStep {
  eyebrow: string;
  title: string;
  body: string;
}

export function Coachmarks() {
  const { t } = useI18n();
  const STEPS: CoachStep[] = [
    {
      eyebrow: t('coachmarks.step01'),
      title: t('coachmarks.homeTitle'),
      body: t('coachmarks.homeBody'),
    },
    {
      eyebrow: t('coachmarks.step02'),
      title: t('coachmarks.callsTitle'),
      body: t('coachmarks.callsBody'),
    },
    {
      eyebrow: t('coachmarks.step03'),
      title: t('coachmarks.contactsTitle'),
      body: t('coachmarks.contactsBody'),
    },
    {
      eyebrow: t('coachmarks.step04'),
      title: t('coachmarks.settingsTitle'),
      body: t('coachmarks.settingsBody'),
    },
  ];
  const [visible, setVisible] = useState(false);
  const [step, setStep] = useState(0);
  const ref = useRef<HTMLDivElement>(null);

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

  // [B17] Focus trap + ESC handled in useFocusTrap. Arrow nav остаётся
  // отдельным listener — ESC и focus trap идут через хук.
  useFocusTrap(ref, visible, { onClose: () => void dismiss() });

  useEffect(() => {
    if (!visible) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'ArrowRight' && step < STEPS.length - 1) setStep(step + 1);
      if (e.key === 'ArrowLeft' && step > 0) setStep(step - 1);
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [visible, step]);

  if (!visible) return null;
  const current = STEPS[step]!;
  const isLast = step === STEPS.length - 1;

  return (
    <div className="overlay">
      <div
        ref={ref}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="coach-title"
      >
        <div className="modal-body">
          <div className="small-caps" style={{ marginBottom: 10 }}>
            {t('coachmarks.stepOf', {
              step: current.eyebrow,
              total: STEPS.length,
            })}
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
              fontFamily: 'var(--font)',
              fontSize: 17,
              lineHeight: 1.55,
              color: 'var(--text-2)',
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
              borderTop: '1px solid var(--border-2)',
            }}
          >
            <button
              type="button"
              className="btn btn--quiet"
              onClick={() => void dismiss()}
            >
              {t('common.skip')}
            </button>
            {step > 0 && (
              <button
                type="button"
                className="btn btn--ghost"
                onClick={() => setStep(step - 1)}
              >
                {t('common.back')}
              </button>
            )}
            {!isLast ? (
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => setStep(step + 1)}
              >
                {t('common.next')}
              </button>
            ) : (
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => void dismiss()}
              >
                {t('common.gotIt')}
              </button>
            )}
            <div
              style={{
                marginLeft: 'auto',
                display: 'flex',
                gap: 6,
              }}
              aria-label={t('coachmarks.progressAria')}
            >
              {STEPS.map((_, i) => (
                <span
                  key={i}
                  className="dot"
                  style={{
                    background: i <= step ? 'var(--accent)' : 'var(--border)',
                  }}
                />
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
