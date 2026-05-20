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
import { PermissionsSection } from './PermissionsSection';

interface OnboardingPageProps {
  onComplete: () => void;
}

type Step = 1 | 2 | 3;

const STEP_TOTAL = 3;
const STEP_LABEL: Record<Step, string> = {
  1: 'Знакомство',
  2: 'Владелец',
  3: 'Разрешения и согласие',
};

const HEADLINE: Record<Step, string> = {
  1: 'Диктофон  со смыслом.',
  2: 'Ваш голос —\nпервый.',
  3: 'Готовы? \nДва разрешения и старт.',
};

const LEDE: Record<Step, string> = {
  1: 'Записывает звонки и встречи на твоём Mac, расшифровывает речь и кратко пересказывает что обсуждалось. Всё хранится локально — в облако ничего не утекает.',
  2: 'Wotold отделяет вашу речь от речи собеседника. Расскажите, кто вы — мы запомним ваш голос и больше не будем спрашивать.',
  3: 'Wotold нужны два разрешения macOS, чтобы записывать звонки. Дай доступ — без них запись не пойдёт. Записи остаются локально на твоём диске.',
};

export function OnboardingPage({ onComplete }: OnboardingPageProps) {
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
      throw new Error('Введи имя.');
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
          Шаг 0{step} из 0{STEP_TOTAL} · {STEP_LABEL[step]}
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
            <li>— Запись микрофона и системного звука раздельно</li>
            <li>— Расшифровка с распознаванием участников</li>
            <li>— Авто-саммари и список задач</li>
            <li>— Поиск по разговорам прямо в Claude через MCP</li>
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
                Имя
              </label>
              <input
                id="onboarding-name"
                className="input"
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoFocus
                required
                placeholder="Айдар Жунусов"
              />
            </div>
            <div className="field">
              <label className="field-label" htmlFor="onboarding-role">
                Роль
              </label>
              <input
                id="onboarding-role"
                className="input"
                value={role}
                onChange={(e) => setRole(e.target.value)}
                placeholder="Co-founder, Wotold"
              />
            </div>
            <div className="field" style={{ gridColumn: '1 / -1' }}>
              <label
                className="field-label"
                htmlFor="onboarding-greeting"
              >
                Краткое представление
              </label>
              <input
                id="onboarding-greeting"
                className="input"
                value={greeting}
                onChange={(e) => setGreeting(e.target.value)}
                placeholder="как вы здороваетесь"
              />
              <span
                className="muted"
                style={{ fontSize: 12, marginTop: 6, fontStyle: 'italic' }}
              >
                Поможет распознать вас на старте звонка.
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
              Wotold будет записывать твой микрофон и звук собеседника во время
              звонков. Перед началом убедись, что собеседник предупреждён о
              записи. По закону РФ/РК запись переговоров без уведомления может
              быть нарушением.
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
                ? 'Сохраняем…'
                : 'Готово'
              : saving
                ? '…'
                : 'Дальше →'}
          </button>
          {step < STEP_TOTAL && (
            <button
              type="button"
              className="btn btn--quiet"
              onClick={() => void skip()}
              disabled={saving}
            >
              Пропустить
            </button>
          )}
          <div
            style={{
              marginLeft: 'auto',
              display: 'flex',
              gap: 6,
            }}
            aria-label={`Шаг ${step} из ${STEP_TOTAL}`}
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
