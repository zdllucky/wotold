import { useI18n } from '../i18n';
import { Button } from '../ui';

interface RediscoveryChipProps {
  onInstall: () => void;
  onTerminalDismiss: () => void;
}

export function RediscoveryChip({ onInstall, onTerminalDismiss }: RediscoveryChipProps) {
  const { t } = useI18n();

  return (
    <div className="rediscovery-chip" role="region" aria-label={t('localEngine.rediscovery.title')}>
      <p
        className="muted"
        style={{
          fontFamily: 'var(--font-mono)',
          fontSize: 9.5,
          textTransform: 'uppercase',
          letterSpacing: '0.1em',
          marginBottom: 4,
        }}
      >
        {t('localEngine.rediscovery.eyebrow')}
      </p>
      <p style={{ fontFamily: 'var(--font-serif)', fontSize: 15, marginBottom: 4 }}>
        {t('localEngine.rediscovery.title')}
      </p>
      <p className="muted" style={{ fontSize: 13, lineHeight: 1.45, marginBottom: 0 }}>
        {t('localEngine.rediscovery.body')}
      </p>
      <div className="rediscovery-chip-actions">
        <Button variant="secondary" size="sm" onClick={onInstall}>
          {t('localEngine.rediscovery.install')}
        </Button>
        <Button variant="ghost" size="sm" onClick={onTerminalDismiss}>
          {t('localEngine.rediscovery.dismiss')}
        </Button>
      </div>
    </div>
  );
}
