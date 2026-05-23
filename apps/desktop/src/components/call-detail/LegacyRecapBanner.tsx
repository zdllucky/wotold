// [M14 T-15] LegacyRecapBanner — opt-in upgrade пути для legacy v1 саммари.
//
// Рендерится в CallDetailPage когда `summary_schema_version` ∈ {1, null}
// и есть recap.md. Клик «Обновить до v2» переиспользует уже существующий
// `regenerateRecap(call_id)` Tauri-команд — backend через T-02 path пишет
// CallSummaryV2 в DB → pipeline:finished → useCallDetail.refetchAll →
// `summary_schema_version=2` → banner condition false → banner исчезает,
// CallTypeBadge / DecisionsBlock / OpenQuestionsBlock появляются.
//
// Mirror AutoBoundBanner.tsx — то же `.activity-strip` оформление.

import { useI18n } from '../../i18n';

interface LegacyRecapBannerProps {
  busy: boolean;
  onUpgrade: () => void;
}

export function LegacyRecapBanner({ busy, onUpgrade }: LegacyRecapBannerProps) {
  const { t } = useI18n();
  return (
    <div
      className="activity-strip legacy-recap-banner"
      data-comment-anchor="call-legacy-recap-banner"
      style={{ marginBottom: 14 }}
    >
      <span className="stat-tag-dot" aria-hidden="true" />
      <span>
        <strong>{t('callDetail.legacyRecapTitle')}</strong>
        <span className="muted" style={{ marginLeft: 8 }}>
          — {t('callDetail.legacyRecapHint')}
        </span>
      </span>
      <button
        type="button"
        className="btn btn--primary btn--sm"
        onClick={onUpgrade}
        disabled={busy}
        style={{ marginLeft: 'auto' }}
        aria-busy={busy}
      >
        {busy ? t('callDetail.legacyRecapUpgrading') : t('callDetail.legacyRecapButton')}
      </button>
    </div>
  );
}
