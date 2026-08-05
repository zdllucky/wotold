import { useCallback, useEffect, useRef, useState } from 'react';
import { humanError } from '../api/errors';

import {
  getAudioPermissions,
  openSystemPrivacyPane,
  requestAudioPermissions,
  resetPermission,
  type PermissionsStatus,
  type PermissionStatus,
  type SystemPane,
} from '../api/permissions';
import { useI18n } from '../i18n';
import { Button, Chip, IconBtn, Modal, SettingRow } from '../ui';

type Target = 'microphone' | 'screen_recording';

interface Row {
  target: Target;
  pane: SystemPane;
  /** [B34.4] Якорь из `SETTINGS_ENTRIES` — по нему палитра подсвечивает строку. */
  settingId: string;
  labelKey: 'permissions.rowMic' | 'permissions.rowScreen';
  descKey: 'permissions.rowMicDesc' | 'permissions.rowScreenDesc';
}

// [perm-usage] Строка «Универсальный доступ» убрана. AX мерился внутри
// сайдкара (`app.wotold.macos-audio`), а в системный список пользователь
// добавляет `Wotold.app` — статус всегда был `denied`. И мерить нечего:
// глобального хоткея нет, ⌘⇧R это `keydown` окна.
const ROWS: Row[] = [
  {
    target: 'microphone',
    pane: 'microphone',
    settingId: 'perm-mic',
    labelKey: 'permissions.rowMic',
    descKey: 'permissions.rowMicDesc',
  },
  {
    target: 'screen_recording',
    pane: 'screen_recording',
    settingId: 'perm-screen',
    labelKey: 'permissions.rowScreen',
    descKey: 'permissions.rowScreenDesc',
  },
];

export function PermissionsSection() {
  const { t } = useI18n();
  const [status, setStatus] = useState<PermissionsStatus | null>(null);
  const [busy, setBusy] = useState<Target | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [resetting, setResetting] = useState<Row | null>(null);

  /** Счётчик завершённых явных действий — арбитр гонки с фоновым опросом. */
  const explicitDone = useRef(0);
  const busyRef = useRef<Target | null>(null);

  // [TD-25] Параметр назывался `t` и затенял i18n-`t` — переименован, иначе
  // humanError получал Target вместо функции перевода.
  const refreshAfter = useCallback(
    async (fn: () => Promise<PermissionsStatus>, target: Target) => {
      setBusy(target);
      busyRef.current = target;
      setError(null);
      try {
        const next = await fn();
        setStatus(next);
      } catch (e) {
        setError(humanError(e, t));
      } finally {
        explicitDone.current += 1;
        busyRef.current = null;
        setBusy(null);
      }
    },
    [t],
  );

  useEffect(() => {
    getAudioPermissions()
      .then(setStatus)
      .catch((e: unknown) => setError(humanError(e, t)));
  }, []);

  // [perm-usage] Разрешение выдают в System Settings, а не у нас, и о том, что
  // оно выдано, нам никто не сообщает: событий TCC для приложения нет. Пока
  // этого не было, пользователь возвращался в окно с прежним «отказано» и
  // считал, что не сработало. Каждый опрос поднимает новый процесс сайдкара,
  // так что процессный кэш TCC свежему статусу не мешает.
  //
  // Гонка тут неизбежна и обязана разрешаться в пользу явного действия:
  // системный диалог сам забирает и возвращает фокус, так что «Запросить» и
  // фоновый опрос идут внахлёст. Опрос, стартовавший до того, как явное
  // действие завершилось, читает состояние ДО ответа пользователя — применить
  // его значит откатить свежевыданное разрешение обратно в «отказано».
  useEffect(() => {
    const onFocus = () => {
      if (busyRef.current) return;
      const startedAfter = explicitDone.current;
      void getAudioPermissions()
        .then((next) => {
          if (busyRef.current || explicitDone.current !== startedAfter) return;
          setStatus(next);
          // Ошибка пережила свою причину: разрешение уже выдали снаружи, а
          // красный алерт продолжал висеть поверх зелёных чипов.
          setError(null);
        })
        .catch(() => {
          /* фоновый пере-опрос: молча, ошибку покажет явное действие */
        });
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  const onRequest = (target: Target) =>
    refreshAfter(() => requestAudioPermissions(target), target);
  const onRefresh = (target: Target) => refreshAfter(getAudioPermissions, target);
  const onOpen = async (pane: SystemPane) => {
    try {
      await openSystemPrivacyPane(pane);
    } catch (e) {
      setError(humanError(e, t));
    }
  };

  const onResetConfirm = async () => {
    const row = resetting;
    if (!row) return;
    setResetting(null);
    await refreshAfter(async () => {
      await resetPermission(row.pane);
      return requestAudioPermissions(row.target);
    }, row.target);
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
            settingId={row.settingId}
            label={t(row.labelKey)}
            labelAdornment={<PermChip status={current} />}
            hint={
              <>
                {t(row.descKey)}
                {current === 'denied' && (
                  <div style={{ marginTop: 'var(--s2)' }}>
                    {t('permissions.staleHint')}{' '}
                    {/* Имя разрешения — в доступном имени кнопки: при двух
                        отказанных строках иначе получаются две кнопки с
                        одинаковым «Сбросить доступ», и скринридер их не
                        различает. Видимый текст короткий намеренно. */}
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={`${t('permissions.reset')}: ${t(row.labelKey)}`}
                      onClick={() => setResetting(row)}
                      disabled={isBusy}
                    >
                      {t('permissions.reset')}
                    </Button>
                  </div>
                )}
              </>
            }
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

      {/* Заголовок называет разрешение: диалог сбрасывает ровно одно, а по
          безымянному «Сбросить доступ и запросить заново» не понять какое. */}
      <Modal
        open={resetting !== null}
        onClose={() => setResetting(null)}
        title={
          resetting
            ? `${t('permissions.resetTitle')}: ${t(resetting.labelKey)}`
            : t('permissions.resetTitle')
        }
        footer={
          <>
            <Button size="sm" variant="ghost" onClick={() => setResetting(null)}>
              {t('common.cancel')}
            </Button>
            <Button size="sm" variant="primary" onClick={() => void onResetConfirm()}>
              {t('permissions.resetConfirm')}
            </Button>
          </>
        }
      >
        {t('permissions.resetBody')}
      </Modal>
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
