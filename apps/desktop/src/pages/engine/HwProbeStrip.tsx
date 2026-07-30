// Настройки → Обработка: железо. Рекомендация, сводка, переоценка.
//
// [design-gate] Surface: pages/engine/HwProbeStrip
// Reference: docs/design/wotold-v2/_reference/wk-settings.jsx (SecEngine)
// Tokens: --accent, --accent-soft, --sunken, --text-2, --text-3, --r-md
// Classes: .panel, .set-eyebrow, .mono, .btn (via Button), Wave, Icon
// Logic preserved: probe(false) на mount из родителя, probe(true) по кнопке,
//   баннер только когда рекомендация не совпадает с выбранным размером.
//
// R13: рекомендация — подсказка, не гейт. Слабое железо не блокирует движок.

import type { HwReport, LocalEnginePreset, PresetSpec } from '@wotold/contracts';

import { Button, Icon, Skeleton, Wave } from '../../ui';
import { useI18n } from '../../i18n';

interface HwProbeStripProps {
  hw: HwReport | null;
  loading: boolean;
  preset: PresetSpec | null;
  bannerDismissed: boolean;
  onDismissBanner: () => void;
  onApplyRecommendation: (preset: LocalEnginePreset) => void;
  onReprobe: () => void;
}

export function HwProbeStrip({
  hw,
  loading,
  preset,
  bannerDismissed,
  onDismissBanner,
  onApplyRecommendation,
  onReprobe,
}: HwProbeStripProps) {
  const { t } = useI18n();

  if (loading) {
    return (
      <div
        className="panel"
        role="status"
        aria-label={t('localEngine.probeSkeleton.measuring')}
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 10,
          padding: '12px 16px',
          maxWidth: 560,
        }}
      >
        <p style={{ fontSize: 12, margin: 0, color: 'var(--text-2)' }}>
          {t('localEngine.probeSkeleton.measuring')}
        </p>
        <Skeleton width="75%" height="12px" />
        <Skeleton width="55%" height="12px" />
        <Skeleton width="40%" height="12px" />
      </div>
    );
  }

  if (!hw) return null;

  const showBanner =
    !bannerDismissed && hw.recommendation !== null && preset?.preset !== hw.recommendation;

  return (
    <>
      {showBanner && hw.recommendation && (
        <div
          className="panel"
          role="status"
          style={{
            background: 'var(--accent-soft)',
            borderColor: 'transparent',
            display: 'flex',
            alignItems: 'center',
            gap: 14,
            padding: '12px 16px',
            maxWidth: 560,
          }}
        >
          <span style={{ color: 'var(--accent)' }} aria-hidden>
            <Wave />
          </span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="set-eyebrow" style={{ marginBottom: 4 }}>
              {t('localEngine.hwBannerTitle')}
            </div>
            <div style={{ fontSize: 13, color: 'var(--text-2)', lineHeight: 1.5 }}>
              {t('localEngine.hwBannerBody', {
                preset: t(`localEngine.preset.${hw.recommendation}`),
                cpu: hw.cpu_model,
                ram: hw.ram_gb,
              })}
            </div>
          </div>
          <Button
            variant="primary"
            size="sm"
            onClick={() => {
              if (hw.recommendation) onApplyRecommendation(hw.recommendation);
              onDismissBanner();
            }}
          >
            {t('localEngine.hwBannerApply')}
          </Button>
          <Button variant="ghost" size="sm" onClick={onDismissBanner}>
            {t('localEngine.hwBannerDismiss')}
          </Button>
        </div>
      )}

      {/* [B21] Канон :183-187 — sunken-плашка: Icon cpu + mono-спеки + ghost. */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          padding: '10px 13px',
          marginTop: 12,
          background: 'var(--sunken)',
          borderRadius: 'var(--r-md)',
          fontSize: 12.5,
        }}
      >
        <Icon name="cpu" size={15} style={{ color: 'var(--text-3)' }} />
        <span className="mono" style={{ color: 'var(--text-2)', minWidth: 0 }}>
          {t('localEngine.probeSummary', {
            cpu: hw.cpu_model,
            ram: hw.ram_gb,
            metal: hw.metal_supported
              ? t('localEngine.probeMetalYes')
              : t('localEngine.probeMetalNo'),
            preset: hw.recommendation ? t(`localEngine.preset.${hw.recommendation}`) : '—',
          })}
        </span>
        <Button
          variant="ghost"
          size="sm"
          style={{ marginLeft: 'auto' }}
          leading={<Icon name="refresh" size={13} />}
          onClick={onReprobe}
        >
          {t('localEngine.reprobe')}
        </Button>
      </div>
    </>
  );
}
