// [Bug-fix #6] RecapRegenSuggestionStrip — после bind speaker → contact
// предлагает пересоздать саммари с новыми именами участников. Mirror
// LegacyRecapBanner .activity-strip pattern. Memory-only dismissal —
// банер появится снова после следующего bind action в том же звонке.

import { useI18n } from '../../i18n';

interface RecapRegenSuggestionStripProps {
  busy: boolean;
  onRegenerate: () => void;
  onDismiss: () => void;
}

export function RecapRegenSuggestionStrip({
  busy,
  onRegenerate,
  onDismiss,
}: RecapRegenSuggestionStripProps) {
  const { t } = useI18n();
  return (
    <div
      className="activity-strip recap-regen-suggestion"
      data-comment-anchor="call-recap-regen-suggestion"
      style={{ marginBottom: 14 }}
    >
      <span className="stat-tag-dot" aria-hidden="true" />
      <span>
        <strong>{t('callDetail.recapRegenSuggestionTitle')}</strong>
        <span className="muted" style={{ marginLeft: 8 }}>
          — {t('callDetail.recapRegenSuggestionHint')}
        </span>
      </span>
      <button
        type="button"
        className="btn btn--primary btn--sm"
        onClick={onRegenerate}
        disabled={busy}
        style={{ marginLeft: 'auto' }}
        aria-busy={busy}
      >
        {busy
          ? t('callDetail.recapRegenSuggestionBusy')
          : t('callDetail.recapRegenSuggestionButton')}
      </button>
      <button
        type="button"
        className="btn btn--ghost btn--sm"
        onClick={onDismiss}
        disabled={busy}
        aria-label={t('callDetail.recapRegenSuggestionDismiss')}
        style={{ marginLeft: 6, padding: '4px 8px' }}
      >
        ✕
      </button>
    </div>
  );
}
