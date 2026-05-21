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

import { useEffect, useRef, useState, type FormEvent } from 'react';
import { humanError } from '../api/errors';

import {
  listContacts,
  updateContact,
  type Contact,
} from '../api/contacts';
import { setSetting, SETTINGS_KEYS } from '../api/settings';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { useI18n } from '../i18n';
import { PermissionsSection } from './PermissionsSection';

interface OnboardingPageProps {
  onComplete: () => void;
}

type Step = 1 | 2 | 3;

const STEP_TOTAL = 3;

export function OnboardingPage({ onComplete }: OnboardingPageProps) {
  const { t } = useI18n();
  const STEP_LABEL: Record<Step, string> = {
    1: t('onboarding.step1Label'),
    2: t('onboarding.step2Label'),
    3: t('onboarding.step3Label'),
  };
  const HEADLINE: Record<Step, string> = {
    1: t('onboarding.step1Headline'),
    2: t('onboarding.step2Headline'),
    3: t('onboarding.step3Headline'),
  };
  const LEDE: Record<Step, string> = {
    1: t('onboarding.step1Lede'),
    2: t('onboarding.step2Lede'),
    3: t('onboarding.step3Lede'),
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
    // step 3 → finalize
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

  const skip = async () => {
    setError(null);
    if (step === 3) {
      void next();
      return;
    }
    setStep(((step + 1) as Step) > STEP_TOTAL ? (STEP_TOTAL as Step) : (step + 1) as Step);
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
          {t('onboarding.stepLabel', { step, total: STEP_TOTAL, label: STEP_LABEL[step] })}
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
              fontFamily: 'var(--font-serif)',
              fontSize: 17,
              color: 'var(--ink-2)',
            }}
          >
            <li>{t('onboarding.feature1')}</li>
            <li>{t('onboarding.feature2')}</li>
            <li>{t('onboarding.feature3')}</li>
            <li>{t('onboarding.feature4')}</li>
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

        {step === 3 && (
          <div style={{ marginBottom: 32 }}>
            <PermissionsSection />
            <p
              className="muted"
              style={{
                marginTop: 18,
                fontFamily: 'var(--font-serif)',
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
              color: 'var(--signal)',
              fontFamily: 'var(--font-sans)',
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
            borderTop: '1px solid var(--line-soft)',
            paddingTop: 24,
          }}
        >
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => void next()}
            disabled={saving || (step === 2 && !name.trim())}
          >
            {step === 3
              ? saving
                ? t('onboarding.saving')
                : t('onboarding.finishBtn')
              : saving
                ? t('common.loadingShort')
                : t('common.next')}
          </button>
          {step < STEP_TOTAL && (
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
            aria-label={t('onboarding.stepAria', { step, total: STEP_TOTAL })}
          >
            {[1, 2, 3].map((i) => (
              <span
                key={i}
                className="dot"
                style={{
                  background:
                    i <= step ? 'var(--accent)' : 'var(--line)',
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
