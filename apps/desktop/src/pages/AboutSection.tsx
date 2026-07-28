// Раздел настроек «О приложении».
//
// До него версия приложения не показывалась в интерфейсе нигде: единственный
// путь узнать её — дождаться, пока апдейтер сам предложит новую. Для продукта,
// который просят присылать баг-репорты, это неудобно ровно в тот момент, когда
// нужнее всего.
//
// Ручная проверка здесь же. Фоновая идёт раз в шесть часов, и «я не хочу ждать
// шесть часов» — законное желание.
import { getVersion } from '@tauri-apps/api/app';
import { useEffect, useState } from 'react';

import { humanError } from '../api/errors';
import { type AvailableUpdate, applyUpdate, checkForUpdate } from '../api/updater';
import { useI18n } from '../i18n';
import { Button } from '../ui/Button';
import { GroupLabel } from '../ui/GroupLabel';
import { Icon } from '../ui/Icon';
import { SettingRow } from '../ui/SettingRow';

const RELEASES_URL = 'https://github.com/zdllucky/wotold/releases';

type CheckState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'upToDate' }
  | { kind: 'available'; update: AvailableUpdate }
  | { kind: 'failed'; message: string };

export function AboutSection() {
  const { t } = useI18n();
  const [version, setVersion] = useState<string | null>(null);
  const [state, setState] = useState<CheckState>({ kind: 'idle' });
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    let alive = true;
    void getVersion()
      .then((v) => {
        if (alive) setVersion(v);
      })
      // Версия — украшение строки, а не функциональность: молчаливый прочерк
      // лучше, чем сломанный раздел настроек.
      .catch(() => {
        if (alive) setVersion(null);
      });
    return () => {
      alive = false;
    };
  }, []);

  async function check() {
    setState({ kind: 'checking' });
    try {
      const update = await checkForUpdate();
      setState(update ? { kind: 'available', update } : { kind: 'upToDate' });
    } catch (e: unknown) {
      setState({ kind: 'failed', message: humanError(e, t) });
    }
  }

  async function install() {
    setInstalling(true);
    try {
      // При успехе не возвращается — процесс перезапускается.
      await applyUpdate();
    } catch (e: unknown) {
      setState({ kind: 'failed', message: humanError(e, t) });
      setInstalling(false);
    }
  }

  // Итог проверки — подсказкой той же строки, а не отдельной строкой с
  // пустым label: SettingRow без подписи ломает и вид, и скринридер.
  const statusHint =
    state.kind === 'upToDate'
      ? t('update.upToDate')
      : state.kind === 'available'
        ? t('update.availableChip', { version: state.update.version })
        : undefined;

  return (
    <>
      <GroupLabel>{t('update.sectionAbout')}</GroupLabel>

      <SettingRow label={t('update.version')} hint={t('update.versionHint')}>
        <span className="mono u-faint">{version ?? '—'}</span>
      </SettingRow>

      <SettingRow label={t('update.check')} hint={statusHint} align="top">
        {state.kind === 'available' ? (
          <Button
            variant="primary"
            size="sm"
            onClick={() => void install()}
            disabled={installing}
            leading={<Icon name="download" size={14} />}
          >
            {installing ? t('home.updateInstalling') : t('home.updateInstall')}
          </Button>
        ) : (
          <Button
            variant="default"
            size="sm"
            onClick={() => void check()}
            disabled={state.kind === 'checking'}
            leading={<Icon name="refresh" size={14} />}
          >
            {state.kind === 'checking' ? t('update.checking') : t('update.check')}
          </Button>
        )}
      </SettingRow>

      {state.kind === 'failed' && (
        <p role="alert" style={{ margin: 'var(--s2) 0 0', fontSize: 'var(--t-12)' }}>
          {state.message}
        </p>
      )}

      <SettingRow label={t('update.changelog')} hint={t('update.changelogHint')} last>
        <a className="btn btn--ghost" href={RELEASES_URL} target="_blank" rel="noreferrer">
          <Icon name="external" size={14} />
        </a>
      </SettingRow>
    </>
  );
}
