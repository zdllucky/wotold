// [B17] Onboarding — exact match per docs/design/atelier-v2/_reference/atelier.jsx §1.
//
// Centred 540px column poверх .modal-backdrop:
//   - .eyebrow "Шаг 0N из 03 · {step.label}"
//   - .display 2-line headline
//   - .subtitle lede
//   - Form fields per step
//   - Footer: btn--primary Дальше + btn--quiet Пропустить + 3 dots progress
//
// 3 steps:
//   01 — Знакомство (intro features)
//   02 — Владелец (Имя · Роль · Краткое представление)
//   03 — Разрешения + Согласие (permissions section + consent text + Готово)

import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { humanError } from '../api/errors';

import {
  listContacts,
  updateContact,
  type Contact,
} from '../api/contacts';
import { localEngineHwProbe } from '../api/local-engine';
import { setSetting, SETTINGS_KEYS } from '../api/settings';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { useI18n } from '../i18n';
import { OnboardingEngineStep } from './OnboardingEngineStep';
import { PermissionsSection } from './PermissionsSection';

interface OnboardingPageProps {
  onComplete: () => void;
}

// [M12.7.3] macOS-юзеры проходят 4 шага (Engine setup вставлен между Owner
// и Permissions). Non-macOS — 3 шага (R9: Local не предлагается).
type Step = 1 | 2 | 3 | 4;

export function OnboardingPage({ onComplete }: OnboardingPageProps) {
  const { t } = useI18n();
  // null до probe; true → 4-step, false → 3-step.
  const [isMacos, setIsMacos] = useState<boolean | null>(null);
  const stepTotal: number = isMacos === true ? 4 : 3;

  const STEP_LABEL: Record<Step, string> = {
    1: t('onboarding.step1Label'),
    2: t('onboarding.step2Label'),
    3: isMacos === true ? t('onboarding.engineStepLabel') : t('onboarding.step3Label'),
    4: t('onboarding.step3Label'),
  };
  const HEADLINE: Record<Step, string> = {
    1: t('onboarding.step1Headline'),
    2: t('onboarding.step2Headline'),
    3: isMacos === true ? t('onboarding.engineHeadline') : t('onboarding.step3Headline'),
    4: t('onboarding.step3Headline'),
  };
  const LEDE: Record<Step, string> = {
    1: t('onboarding.step1Lede'),
    2: t('onboarding.step2Lede'),
    3: isMacos === true ? t('onboarding.engineLede') : t('onboarding.step3Lede'),
    4: t('onboarding.step3Lede'),
  };

  const [step, setStep] = useState<Step>(1);
  const [owner, setOwner] = useState<Contact | null>(null);
  const [name, setName] = useState('');
  const [role, setRole] = useState('');
  const [greeting, setGreeting] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  useFocusTrap(rootRef, true);

  // [M12.7.3] Определить macOS — у нас probe возвращает os; на Linux/Windows
  // он отдаёт recommendation=null. На Linux/Windows engine step пропускается.
  useEffect(() => {
    localEngineHwProbe(false)
      .then((r) => setIsMacos(r.os === 'macos'))
      .catch(() => setIsMacos(false));
  }, []);

  // Load owner contact (created by db migration) to pre-fill fields.
  useEffect(() => {
    listContacts()
      .then((cs) => {
        const o = cs.find((c) => c.is_owner) ?? null;
        if (o) {
          setOwner(o);
          setName(o.display_name);
          setRole(o.role ?? '');
          setGreeting(String(o.attributes['greeting'] ?? ''));
        }
      })
      .catch((e: unknown) => console.warn('owner load failed', e));
  }, []);

  const persistOwner = async (): Promise<void> => {
    if (!owner) return;
    const trimmed = name.trim();
    if (!trimmed) {
      throw new Error(t('onboarding.enterName'));
    }
    await updateContact(owner.id, {
      display_name: trimmed,
      role: role.trim() || undefined,
      org: owner.org ?? undefined,
      notes: owner.notes ?? undefined,
      identifiers: owner.identifiers.map((i) => ({ kind: i.kind, value: i.value })),
      attributes: {
        ...Object.fromEntries(
          Object.entries(owner.attributes).map(([k, v]) => [k, String(v)]),
        ),
        ...(greeting.trim() ? { greeting: greeting.trim() } : {}),
      },
    });
  };

  // [M12.7.3] На macOS: step 3 = Engine setup (advance handled internally),
  // step 4 = Permissions+Consent finalize. На остальных: step 3 = finalize.
  const isFinalStep = (s: Step): boolean =>
    isMacos === true ? s === 4 : s === 3;

  const next = async (e?: FormEvent) => {
    e?.preventDefault();
    setError(null);
    if (step === 1) {
      setStep(2);
      return;
    }
    if (step === 2) {
      setSaving(true);
      try {
        await persistOwner();
        setStep(3);
      } catch (err) {
        setError(humanError(err));
      } finally {
        setSaving(false);
      }
      return;
    }
    if (step === 3 && isMacos === true) {
      // Engine step управляет advance изнутри (onAdvance кнопкой). Если юзер
      // дошёл сюда через btn--primary footer'а — пропускаем engine = cloud.
      setStep(4);
      return;
    }
    // final step → finalize
    setSaving(true);
    try {
      await setSetting(SETTINGS_KEYS.RECORDING_CONSENT_AT, new Date().toISOString());
      await setSetting(SETTINGS_KEYS.ONBOARDING_DONE, '1');
      onComplete();
    } catch (err) {
      setError(humanError(err));
      setSaving(false);
    }
  };

  // [Review M-5] useCallback гарантирует стабильную ссылку — без неё каждый
  // ре-рендер OnboardingPage создавал новый onAdvance, что пере-вызывало
  // download drain useEffect в OnboardingEngineStep. Сейчас этот эффект
  // безопасный (early-return на !downloading), но fragile.
  const advanceToFinalStep = useCallback(() => setStep(4), []);

  const skip = async () => {
    setError(null);
    if (isFinalStep(step)) {
      void next();
      return;
    }
    const candidate = step + 1;
    const capped = candidate > stepTotal ? stepTotal : candidate;
    setStep(capped as Step);
  };

  return (
    <div
      ref={rootRef}
      className="modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="onboarding-title"
    >
      <div style={{ width: 540, maxWidth: '90vw', padding: '40px 4px' }}>
        <div className="eyebrow" style={{ marginBottom: 14 }}>
          {t('onboarding.stepLabel', { step, total: stepTotal, label: STEP_LABEL[step] })}
        </div>

        <h1
          id="onboarding-title"
          className="display"
          style={{ marginBottom: 14, whiteSpace: 'pre-line' }}
        >
          {HEADLINE[step]}
        </h1>
        <p
          className="subtitle"
          style={{ marginBottom: 36, maxWidth: 480 }}
        >
          {LEDE[step]}
        </p>

        {step === 1 && (
          <ul
            style={{
              listStyle: 'none',
              padding: 0,
              margin: '0 0 32px',
              display: 'flex',
              flexDirection: 'column',
              gap: 10,
              fontFamily: 'var(--font)',
              fontSize: 17,
              color: 'var(--text-2)',
            }}
          >
            <li>{t('onboarding.feature1')}</li>
            <li>{t('onboarding.feature2')}</li>
            <li>{t('onboarding.feature3')}</li>
            <li>{t('onboarding.feature4')}</li>
            <li>{t('onboarding.feature5')}</li>
          </ul>
        )}

        {step === 2 && (
          <form
            onSubmit={(e) => void next(e)}
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: '24px 32px',
              marginBottom: 40,
            }}
          >
            <div className="field">
              <label className="field-label" htmlFor="onboarding-name">
                {t('onboarding.fieldName')}
              </label>
              <input
                id="onboarding-name"
                className="input"
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoFocus
                required
                placeholder={t('onboarding.namePlaceholder')}
              />
            </div>
            <div className="field">
              <label className="field-label" htmlFor="onboarding-role">
                {t('onboarding.fieldRole')}
              </label>
              <input
                id="onboarding-role"
                className="input"
                value={role}
                onChange={(e) => setRole(e.target.value)}
                placeholder={t('onboarding.rolePlaceholder')}
              />
            </div>
            <div className="field" style={{ gridColumn: '1 / -1' }}>
              <label
                className="field-label"
                htmlFor="onboarding-greeting"
              >
                {t('onboarding.fieldGreeting')}
              </label>
              <input
                id="onboarding-greeting"
                className="input"
                value={greeting}
                onChange={(e) => setGreeting(e.target.value)}
                placeholder={t('onboarding.greetingPlaceholder')}
              />
              <span
                className="muted"
                style={{ fontSize: 12, marginTop: 6, fontStyle: 'italic' }}
              >
                {t('onboarding.greetingHint')}
              </span>
            </div>
          </form>
        )}

        {step === 3 && isMacos === true && (
          <OnboardingEngineStep onAdvance={advanceToFinalStep} />
        )}

        {((step === 3 && isMacos === false) || step === 4) && (
          <div style={{ marginBottom: 32 }}>
            <PermissionsSection />
            <p
              className="muted"
              style={{
                marginTop: 18,
                fontFamily: 'var(--font)',
                fontStyle: 'italic',
                fontSize: 14,
                lineHeight: 1.55,
                maxWidth: 480,
              }}
            >
              {t('onboarding.consentBody')}
            </p>
          </div>
        )}

        {error && (
          <p
            role="alert"
            style={{
              color: 'var(--danger)',
              fontFamily: 'var(--font)',
              marginBottom: 16,
            }}
          >
            {error}
          </p>
        )}

        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 16,
            borderTop: '1px solid var(--border-2)',
            paddingTop: 24,
          }}
        >
          {/*
            [M12.7.3] На engine-шаге кнопки advance — внутри
            OnboardingEngineStep (download/choose/use-cloud). Footer кнопку
            «Дальше» скрываем; «Пропустить» оставляем как escape hatch.
          */}
          {!(step === 3 && isMacos === true) && (
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => void next()}
              disabled={saving || (step === 2 && !name.trim())}
            >
              {isFinalStep(step)
                ? saving
                  ? t('onboarding.saving')
                  : t('onboarding.finishBtn')
                : saving
                  ? t('common.loadingShort')
                  : t('common.next')}
            </button>
          )}
          {!isFinalStep(step) && (
            <button
              type="button"
              className="btn btn--quiet"
              onClick={() => void skip()}
              disabled={saving}
            >
              {t('common.skip')}
            </button>
          )}
          <div
            style={{
              marginLeft: 'auto',
              display: 'flex',
              gap: 6,
            }}
            aria-label={t('onboarding.stepAria', { step, total: stepTotal })}
          >
            {Array.from({ length: stepTotal }, (_, i) => i + 1).map((i) => (
              <span
                key={i}
                className="dot"
                style={{
                  background: i <= step ? 'var(--accent)' : 'var(--border)',
                  opacity: 1,
                }}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
