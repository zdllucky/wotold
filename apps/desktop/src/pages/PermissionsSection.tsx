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
import { Button } from '../ui';

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
    <div className="set-group">
      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: 0 }}>
          {error}
        </p>
      )}
      {ROWS.map((row) => {
        const current: PermissionStatus = status?.[row.target] ?? 'unknown';
        const isBusy = busy === row.target;
        return (
          <div key={row.target} className="setting-row">
            <div className="setting-row-text">
              <div
                style={{
                  display: 'flex',
                  gap: 8,
                  alignItems: 'center',
                  flexWrap: 'wrap',
                  marginBottom: 4,
                }}
              >
                <span className="setting-row-label">{t(row.labelKey)}</span>
                <PermChip status={current} />
              </div>
              <p className="set-hint" style={{ marginTop: 0 }}>
                {t(row.descKey)}
              </p>
            </div>
            <div
              style={{
                display: 'flex',
                gap: 6,
                alignItems: 'center',
                flexWrap: 'wrap',
                flex: '0 0 auto',
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

type ChipVariant = 'ok' | 'danger' | 'warn' | 'line';

type TFn = ReturnType<typeof useI18n>['t'];

// [B18.5b] Permission state → v2 `.chip` variant. Semantics preserved from the
// prior <Badge> mapping (granted=ok, denied/restricted=danger, pending=warn).
function PermChip({ status }: { status: PermissionStatus }) {
  const { t } = useI18n();
  const meta = chipMeta(status, t);
  return (
    <span className={`chip chip--${meta.variant}`} data-size="sm" title={meta.title}>
      {meta.label}
    </span>
  );
}

function chipMeta(
  status: PermissionStatus,
  t: TFn,
): { label: string; title: string; variant: ChipVariant } {
  switch (status) {
    case 'granted':
      return {
        label: t('permissions.granted'),
        title: t('permissions.grantedTitle'),
        variant: 'ok',
      };
    case 'denied':
      return {
        label: t('permissions.denied'),
        title: t('permissions.deniedTitle'),
        variant: 'danger',
      };
    case 'not_determined':
      return {
        label: t('permissions.notDetermined'),
        title: t('permissions.notDeterminedTitle'),
        variant: 'warn',
      };
    case 'restricted':
      return {
        label: t('permissions.restricted'),
        title: t('permissions.restrictedTitle'),
        variant: 'danger',
      };
    default:
      return {
        label: t('permissions.unknown'),
        title: t('permissions.unknownTitle'),
        variant: 'line',
      };
  }
}
