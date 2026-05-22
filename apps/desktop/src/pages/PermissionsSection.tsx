import { useEffect, useState } from 'react';
import { humanError } from '../api/errors';

import {
  getAudioPermissions,
  openSystemPrivacyPane,
  requestAudioPermissions,
  type PermissionsStatus,
  type PermissionStatus,
  type SystemPane,
} from '../api/permissions';
import { useI18n } from '../i18n';
import { Badge, Button } from '../ui';

type Target = 'microphone' | 'screen_recording' | 'accessibility';

interface Row {
  target: Target;
  pane: SystemPane;
  labelKey: 'permissions.rowMic' | 'permissions.rowScreen' | 'permissions.rowAccessibility';
  descKey:
    | 'permissions.rowMicDesc'
    | 'permissions.rowScreenDesc'
    | 'permissions.rowAccessibilityDesc';
}

const ROWS: Row[] = [
  {
    target: 'microphone',
    pane: 'microphone',
    labelKey: 'permissions.rowMic',
    descKey: 'permissions.rowMicDesc',
  },
  {
    target: 'screen_recording',
    pane: 'screen_recording',
    labelKey: 'permissions.rowScreen',
    descKey: 'permissions.rowScreenDesc',
  },
  {
    target: 'accessibility',
    pane: 'accessibility',
    labelKey: 'permissions.rowAccessibility',
    descKey: 'permissions.rowAccessibilityDesc',
  },
];

export function PermissionsSection() {
  const { t } = useI18n();
  const [status, setStatus] = useState<PermissionsStatus | null>(null);
  const [busy, setBusy] = useState<Target | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getAudioPermissions()
      .then(setStatus)
      .catch((e: unknown) => setError(humanError(e)));
  }, []);

  const refreshAfter = async (fn: () => Promise<PermissionsStatus>, t: Target) => {
    setBusy(t);
    setError(null);
    try {
      const next = await fn();
      setStatus(next);
    } catch (e) {
      setError(humanError(e));
    } finally {
      setBusy(null);
    }
  };

  const onRequest = (t: Target) => refreshAfter(() => requestAudioPermissions(t), t);
  const onRefresh = (t: Target) => refreshAfter(getAudioPermissions, t);
  const onOpen = async (pane: SystemPane) => {
    try {
      await openSystemPrivacyPane(pane);
    } catch (e) {
      setError(humanError(e));
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
      {error && (
        <p
          role="alert"
          style={{
            color: 'var(--signal)',
            fontFamily: 'var(--font-sans)',
            marginBottom: 0,
          }}
        >
          {error}
        </p>
      )}
      {ROWS.map((row) => {
        const current: PermissionStatus = status?.[row.target] ?? 'unknown';
        const isBusy = busy === row.target;
        return (
          <div
            key={row.target}
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr auto',
              gap: 16,
              padding: '14px 0',
              borderBottom: '1px solid var(--line-soft)',
              alignItems: 'start',
            }}
          >
            <div style={{ minWidth: 0 }}>
              <div
                style={{
                  display: 'flex',
                  gap: 10,
                  alignItems: 'baseline',
                  flexWrap: 'wrap',
                  marginBottom: 4,
                }}
              >
                <span
                  style={{
                    fontFamily: 'var(--font-serif)',
                    fontSize: 16,
                    color: 'var(--ink)',
                  }}
                >
                  {t(row.labelKey)}
                </span>
                <PermBadge status={current} />
              </div>
              <p
                className="muted"
                style={{ fontSize: 13, margin: 0, lineHeight: 1.45 }}
              >
                {t(row.descKey)}
              </p>
            </div>
            <div
              style={{
                display: 'flex',
                gap: 6,
                alignItems: 'center',
                flexWrap: 'wrap',
              }}
            >
              <Button
                size="sm"
                variant="secondary"
                onClick={() => onRequest(row.target)}
                disabled={isBusy}
                busy={isBusy}
                title={t('permissions.requestTitle')}
              >
                {isBusy
                  ? t('common.loadingShort')
                  : current === 'granted'
                    ? t('permissions.requestAgain')
                    : t('permissions.request')}
              </Button>
              {current !== 'granted' && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => onOpen(row.pane)}
                  disabled={isBusy}
                  title={t('permissions.openSettingsTitle')}
                >
                  {t('permissions.openSettings')}
                </Button>
              )}
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onRefresh(row.target)}
                disabled={isBusy}
                title={t('permissions.refreshStatusTitle')}
                aria-label={t('permissions.refreshStatusAria')}
              >
                ↻
              </Button>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function PermBadge({ status }: { status: PermissionStatus }) {
  const { t } = useI18n();
  const meta = badgeMeta(status, t);
  return (
    <Badge tone={meta.tone} title={meta.title}>
      {meta.label}
    </Badge>
  );
}

type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';

type TFn = ReturnType<typeof useI18n>['t'];

function badgeMeta(
  status: PermissionStatus,
  t: TFn,
): { label: string; title: string; tone: Tone } {
  switch (status) {
    case 'granted':
      return {
        label: t('permissions.granted'),
        title: t('permissions.grantedTitle'),
        tone: 'success',
      };
    case 'denied':
      return {
        label: t('permissions.denied'),
        title: t('permissions.deniedTitle'),
        tone: 'danger',
      };
    case 'not_determined':
      return {
        label: t('permissions.notDetermined'),
        title: t('permissions.notDeterminedTitle'),
        tone: 'warning',
      };
    case 'restricted':
      return {
        label: t('permissions.restricted'),
        title: t('permissions.restrictedTitle'),
        tone: 'danger',
      };
    default:
      return {
        label: t('permissions.unknown'),
        title: t('permissions.unknownTitle'),
        tone: 'neutral',
      };
  }
}
