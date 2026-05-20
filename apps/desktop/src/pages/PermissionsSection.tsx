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
import { Badge, Button } from '../ui';

type Target = 'microphone' | 'screen_recording';

interface Row {
  target: Target;
  label: string;
  description: string;
  pane: SystemPane;
}

const ROWS: Row[] = [
  {
    target: 'microphone',
    label: 'Микрофон',
    description: 'Запись твоей дорожки звонка (mic.wav).',
    pane: 'microphone',
  },
  {
    target: 'screen_recording',
    label: 'Запись экрана',
    description:
      'Захват системного выхода через ScreenCaptureKit (system.wav). После grant в System Settings перезапусти приложение.',
    pane: 'screen_recording',
  },
];

export function PermissionsSection() {
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
    <div className="perms">
      {error && <p className="error">{error}</p>}
      {ROWS.map((row) => {
        const current: PermissionStatus = status?.[row.target] ?? 'unknown';
        const isBusy = busy === row.target;
        return (
          <div key={row.target} className="perm-row">
            <div className="perm-info">
              <div className="perm-head">
                <span className="perm-label">{row.label}</span>
                <PermBadge status={current} />
              </div>
              <p className="perm-desc">{row.description}</p>
            </div>
            <div className="perm-actions">
              <Button
                size="sm"
                variant="secondary"
                onClick={() => onRequest(row.target)}
                disabled={isBusy}
                busy={isBusy}
                title="Показать macOS-диалог запроса"
              >
                {isBusy ? '…' : current === 'granted' ? 'Перезапросить' : 'Запросить'}
              </Button>
              {current !== 'granted' && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => onOpen(row.pane)}
                  disabled={isBusy}
                  title="Открыть System Settings → Privacy & Security"
                >
                  Открыть Настройки
                </Button>
              )}
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onRefresh(row.target)}
                disabled={isBusy}
                title="Перечитать текущий статус"
                aria-label="Обновить статус"
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
  const meta = badgeMeta(status);
  return (
    <Badge tone={meta.tone} title={meta.title}>
      {meta.label}
    </Badge>
  );
}

type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';

function badgeMeta(status: PermissionStatus): { label: string; title: string; tone: Tone } {
  switch (status) {
    case 'granted':
      return { label: 'выдано', title: 'Доступ разрешён', tone: 'success' };
    case 'denied':
      return {
        label: 'отказано',
        title:
          'Пользователь отказал или ещё не давал доступ. Запроси заново или открой Настройки.',
        tone: 'danger',
      };
    case 'not_determined':
      return {
        label: 'не запрошено',
        title: 'Ещё не запрашивали. Жми «Запросить».',
        tone: 'warning',
      };
    case 'restricted':
      return {
        label: 'заблок. системой',
        title: 'Системная политика (MDM / родительский контроль) запретила.',
        tone: 'danger',
      };
    default:
      return { label: '?', title: 'Статус неизвестен', tone: 'neutral' };
  }
}
