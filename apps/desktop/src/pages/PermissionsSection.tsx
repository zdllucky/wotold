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
import { Button, Chip, IconBtn, SettingRow } from '../ui';

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

  // [B21] Канон SecPermissions: SettingRow + Chip-статус у лейбла; «Запросить»
  // — primary (иерархия!), «Открыть настройки»/«Обновить» — IconBtn.
  return (
    <div>
      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: '0 0 12px' }}>
          {error}
        </p>
      )}
      {ROWS.map((row, i) => {
        const current: PermissionStatus = status?.[row.target] ?? 'unknown';
        const isBusy = busy === row.target;
        return (
          <SettingRow
            key={row.target}
            label={t(row.labelKey)}
            labelAdornment={<PermChip status={current} />}
            hint={t(row.descKey)}
            align="top"
            last={i === ROWS.length - 1}
          >
            {current !== 'granted' && (
              <Button
                size="sm"
                variant="primary"
                onClick={() => onRequest(row.target)}
                disabled={isBusy}
                busy={isBusy}
                title={t('permissions.requestTitle')}
              >
                {isBusy ? t('common.loadingShort') : t('permissions.request')}
              </Button>
            )}
            {current === 'granted' && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onRequest(row.target)}
                disabled={isBusy}
                busy={isBusy}
                title={t('permissions.requestTitle')}
              >
                {isBusy ? t('common.loadingShort') : t('permissions.requestAgain')}
              </Button>
            )}
            {current !== 'granted' && (
              <IconBtn
                icon="external"
                size="sm"
                label={t('permissions.openSettings')}
                title={t('permissions.openSettingsTitle')}
                onClick={() => void onOpen(row.pane)}
                disabled={isBusy}
              />
            )}
            <IconBtn
              icon="refresh"
              size="sm"
              label={t('permissions.refreshStatusAria')}
              title={t('permissions.refreshStatusTitle')}
              onClick={() => void onRefresh(row.target)}
              disabled={isBusy}
            />
          </SettingRow>
        );
      })}
    </div>
  );
}

type ChipVariant = 'ok' | 'danger' | 'warn' | 'line';

type TFn = ReturnType<typeof useI18n>['t'];

// [B18.5b, B21] Permission state → Chip wrapper. Semantics preserved from the
// prior <Badge> mapping (granted=ok, denied/restricted=danger, pending=warn).
function PermChip({ status }: { status: PermissionStatus }) {
  const { t } = useI18n();
  const meta = chipMeta(status, t);
  return (
    <Chip tone={meta.variant} size="sm" title={meta.title}>
      {meta.label}
    </Chip>
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
