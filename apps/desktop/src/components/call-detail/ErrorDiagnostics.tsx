// [V6.5] Сборная диагностика для error-state — PIPELINE_FAIL code,
// провайдер, last_at, quota. Раскрывается по клику для пользователя
// и в багрепортах.

import type { Call } from '../../api/recording';
import { useI18n } from '../../i18n';

export function ErrorDiagnostics({ call }: { call: Call }) {
  const { t } = useI18n();
  return (
    <details style={{ marginTop: 18 }}>
      <summary
        className="small-caps"
        style={{ cursor: 'pointer', color: 'var(--text-muted)' }}
      >
        {t('callDetail.errorDiagnosticsTitle')}
      </summary>
      <dl
        style={{
          display: 'grid',
          gridTemplateColumns: '160px 1fr',
          gap: '6px 16px',
          marginTop: 10,
          fontFamily: 'var(--font-mono)',
          fontSize: 12,
        }}
      >
        <dt className="muted">{t('callDetail.errorDiagnosticsCode')}</dt>
        <dd style={{ margin: 0 }}>PIPELINE_FAIL</dd>
        {call.provider && (
          <>
            <dt className="muted">
              {t('callDetail.errorDiagnosticsProvider')}
            </dt>
            <dd style={{ margin: 0 }}>{call.provider}</dd>
          </>
        )}
        <dt className="muted">
          {t('callDetail.errorDiagnosticsLastAt')}
        </dt>
        <dd style={{ margin: 0 }}>{call.updated_at}</dd>
        <dt className="muted">
          {t('callDetail.errorDiagnosticsQuota')}
        </dt>
        <dd style={{ margin: 0 }}>—</dd>
      </dl>
    </details>
  );
}
