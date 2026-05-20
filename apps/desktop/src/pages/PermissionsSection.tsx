import { useEffect, useState } from 'react';

import {
  getAudioPermissions,
  openSystemPrivacyPane,
  requestAudioPermissions,
  type PermissionsStatus,
  type PermissionStatus,
  type SystemPane,
} from '../api/permissions';

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
      .catch((e: unknown) => setError(String(e)));
  }, []);

  const refreshAfter = async (fn: () => Promise<PermissionsStatus>, t: Target) => {
    setBusy(t);
    setError(null);
    try {
      const next = await fn();
      setStatus(next);
    } catch (e) {
      setError(String(e));
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
      setError(String(e));
    }
  };

  return (
    <fieldset className="perms">
      <legend>Системные разрешения</legend>
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
              <button
                type="button"
                onClick={() => onRequest(row.target)}
                disabled={isBusy}
                title="Показать macOS-диалог запроса"
              >
                {isBusy ? '…' : current === 'granted' ? 'Перезапросить' : 'Запросить'}
              </button>
              {current !== 'granted' && (
                <button
                  type="button"
                  onClick={() => onOpen(row.pane)}
                  disabled={isBusy}
                  title="Открыть System Settings → Privacy & Security"
                >
                  Открыть Настройки
                </button>
              )}
              <button
                type="button"
                onClick={() => onRefresh(row.target)}
                disabled={isBusy}
                title="Перечитать текущий статус"
              >
                ↻
              </button>
            </div>
          </div>
        );
      })}
    </fieldset>
  );
}

function PermBadge({ status }: { status: PermissionStatus }) {
  const meta = badgeMeta(status);
  return (
    <span className={`perm-badge perm-${status}`} title={meta.title}>
      {meta.label}
    </span>
  );
}

function badgeMeta(status: PermissionStatus): { label: string; title: string } {
  switch (status) {
    case 'granted':
      return { label: 'выдано', title: 'Доступ разрешён' };
    case 'denied':
      return {
        label: 'отказано',
        title: 'Пользователь отказал или ещё не давал доступ. Запроси заново или открой Настройки.',
      };
    case 'not_determined':
      return { label: 'не запрошено', title: 'Ещё не запрашивали. Жми «Запросить».' };
    case 'restricted':
      return {
        label: 'заблок. системой',
        title: 'Системная политика (MDM / родительский контроль) запретила.',
      };
    default:
      return { label: '?', title: 'Статус неизвестен' };
  }
}
