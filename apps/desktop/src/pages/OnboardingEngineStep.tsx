// [M12.7.3] Шаг онбординга «настройка движка» — между владельцем (шаг 2) и
// разрешениями с согласием (шаг 4), только для macOS.
//
// [design-gate] Surface: pages/OnboardingEngineStep
// Reference: docs/design/wotold-v2/_reference/wk-onboarding.jsx
// Tokens: --sunken, --text, --text-2, --text-faint, --danger, --r-md, --t-20
// Classes: .panel, .small-caps, .set-eyebrow, .mono, .activity-strip,
//   .optioncard (через OptionCard), .progress (через Progress), .btn (Button)
// New tokens: нет
// Logic preserved: probe на mount, авто-переход на не-macOS через useEffect
//   (не в рендере — иначе setState родителя во время рендера ребёнка), хук до
//   любого early-return (Rules of Hooks).
// A11y: role="radiogroup" + aria-label, role="status" на полосе прогресса.
//
// Что изменилось: размеры берутся из каталога (`local_engine_preset_specs`), а
// не из захардкоженных «1.2 / 2.4 / 5.5 ГБ» — те занижали все три, потому что
// базовые модули в них не входили. Скачиванием распоряжается общий баннер
// готовности: своя очередь здесь дублировала его логику и слушала те же
// события. «Свернуть» больше не удаляет полускачанный файл (удалялся всё равно
// не тот путь) — докачка продолжается в фоне под баннером.

import { useCallback, useEffect, useState } from 'react';
import type { HwReport, LocalEnginePreset, PresetSizeSpec } from '@wotold/contracts';

import {
  localEngineHwProbe,
  localEnginePresetSpecs,
  localEngineSetActivePreset,
} from '../api/local-engine';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { Button, OptionCard, Progress } from '../ui';
import { useReadiness } from '../components/readiness/ReadinessProvider';

const PRESETS: LocalEnginePreset[] = ['light', 'balanced', 'quality'];

/** Гигабайты с одним знаком — для подписей кнопок и карточек. */
function gb(bytes: number): string {
  return (bytes / 1024 ** 3).toFixed(1);
}

interface Props {
  /** Вызвать когда юзер завершил шаг — продвинуть онбординг дальше. */
  onAdvance: () => void;
}

export function OnboardingEngineStep({ onAdvance }: Props) {
  const { t } = useI18n();
  const { readiness, aggregate, ensure } = useReadiness();
  const [hw, setHw] = useState<HwReport | null>(null);
  const [specs, setSpecs] = useState<PresetSizeSpec[]>([]);
  const [chosenPreset, setChosenPreset] = useState<LocalEnginePreset | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [previewMode, setPreviewMode] = useState(false);
  const [started, setStarted] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const [report, sizeSpecs] = await Promise.all([
          localEngineHwProbe(false),
          localEnginePresetSpecs(),
        ]);
        setHw(report);
        setSpecs(sizeSpecs);
        setChosenPreset(report.recommendation ?? 'light');
      } catch (e) {
        setError(humanError(e, t));
      }
    })();
  }, [t]);

  // [Review HIGH-3] Авто-переход на не-macOS нельзя дёргать в рендере — это
  // setState родителя во время рендера ребёнка. [B21] И хук обязан стоять до
  // любого early-return: раньше он объявлялся после `if (downloading) return`,
  // и первый же старт загрузки менял число вызванных хуков.
  useEffect(() => {
    if (hw && hw.os !== 'macos') onAdvance();
  }, [hw, onAdvance]);

  // Движок готов — шаг сделан. Без `started` в условии намеренно: при повторном
  // прохождении онбординга всё уже скачано, и требовать нажатие «Скачать»
  // ради мгновенного no-op незачем.
  useEffect(() => {
    if (readiness?.ready) onAdvance();
  }, [readiness, onAdvance]);

  const startDownload = useCallback(async () => {
    if (!chosenPreset) return;
    setError(null);
    try {
      // Размер фиксируем до скачивания: если пользователь свернёт шаг, выбор
      // уже сохранён и докачка продолжится в фоне.
      await localEngineSetActivePreset(chosenPreset);
      setStarted(true);
      ensure();
    } catch (e) {
      setError(humanError(e, t));
    }
  }, [chosenPreset, ensure, t]);

  const spec = specs.find((s) => s.preset === (chosenPreset ?? 'light'));

  if (started && !readiness?.ready) {
    return (
      <div
        className="activity-strip"
        role="status"
        style={{ marginBottom: 28, fontFamily: 'var(--font)' }}
      >
        <div style={{ flex: 1 }}>
          <div className="small-caps" style={{ marginBottom: 4 }}>
            {t('onboarding.engine.downloadingLabel')}
          </div>
          <div className="mono" style={{ fontSize: 13, color: 'var(--text-2)' }}>
            {aggregate
              ? `${gb(aggregate.doneBytes)} / ${gb(aggregate.totalBytes)} GB · ${aggregate.pct}%`
              : '…'}
          </div>
          <Progress
            value={aggregate?.pct ?? 0}
            style={{ marginTop: 8 }}
            ariaLabel={t('onboarding.engine.downloadingLabel')}
          />
        </div>
        <Button variant="ghost" size="sm" onClick={onAdvance}>
          {t('onboarding.engine.continueInBackgroundCta')}
        </Button>
      </div>
    );
  }

  if (!hw || hw.os !== 'macos' || !spec) {
    return (
      <p className="subtle" style={{ marginBottom: 32 }}>
        {t('common.loadingShort')}
      </p>
    );
  }

  const preset = chosenPreset ?? 'light';
  const presetLabel = t(`localEngine.preset.${preset}`);

  if (previewMode) {
    return (
      <>
        <p className="set-eyebrow" style={{ marginBottom: 6 }}>
          {t('onboarding.engine.previewEyebrow')}
        </p>
        <p
          style={{
            fontFamily: 'var(--font)',
            fontSize: 'var(--t-20)',
            letterSpacing: '-0.01em',
            marginBottom: 18,
          }}
        >
          {t('onboarding.engine.previewTitle')}
        </p>
        <div
          style={{
            background: 'var(--sunken)',
            borderRadius: 'var(--r-md)',
            padding: '14px 18px',
            marginBottom: 14,
            display: 'flex',
            flexDirection: 'column',
            gap: 6,
          }}
        >
          {[1, 2, 3, 4].map((n) => (
            <div key={n} style={{ fontSize: 13, lineHeight: 1.6, color: 'var(--text-2)' }}>
              {t(
                `onboarding.engine.previewTranscript${n}` as 'onboarding.engine.previewTranscript1',
              )}
            </div>
          ))}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 24 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t('onboarding.engine.previewProcessed', { ms: '420' })}
          </span>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <Button
            variant="primary"
            onClick={() => {
              setPreviewMode(false);
              void startDownload();
            }}
          >
            {t('onboarding.engine.previewInstall', { size: gb(spec.total_bytes) })}
          </Button>
          <Button variant="ghost" onClick={() => setPreviewMode(false)}>
            {t('onboarding.engine.previewBack')}
          </Button>
        </div>
      </>
    );
  }

  return (
    <>
      {error && (
        <p
          role="alert"
          style={{ color: 'var(--danger)', fontFamily: 'var(--font)', marginBottom: 14 }}
        >
          {error}
        </p>
      )}

      {/* Что за машина и что на ней будет работать. */}
      <div
        className="panel"
        style={{
          marginBottom: 24,
          padding: 18,
          display: 'flex',
          flexDirection: 'column',
          gap: 8,
        }}
      >
        <div className="small-caps">{t('onboarding.engine.probeEyebrow')}</div>
        <div
          style={{
            fontFamily: 'var(--font)',
            fontSize: 18,
            color: 'var(--text)',
            display: 'flex',
            alignItems: 'baseline',
            gap: 10,
            flexWrap: 'wrap',
          }}
        >
          <span>
            {hw.cpu_model} · {hw.ram_gb} GB ·{' '}
            {hw.metal_supported
              ? t('localEngine.probeMetalYes')
              : t('localEngine.probeMetalNo')}
          </span>
        </div>
        <div
          style={{
            fontFamily: 'var(--font)',
            fontSize: 14,
            color: 'var(--text-2)',
            marginTop: 8,
          }}
        >
          <strong>{presetLabel}</strong> · ~{gb(spec.total_bytes)} GB
        </div>
        <ul
          style={{
            listStyle: 'none',
            padding: 0,
            margin: '6px 0 0',
            display: 'flex',
            flexDirection: 'column',
            gap: 4,
            fontFamily: 'var(--font)',
            fontSize: 13,
            color: 'var(--text-faint)',
          }}
        >
          <li>— {t(`onboarding.engine.feat.${preset}.stt`)}</li>
          <li>— {t(`onboarding.engine.feat.${preset}.llm`)}</li>
          <li>— {t('onboarding.engine.featSpeakers')}</li>
        </ul>
      </div>

      {/* Выбор размера — раскрывается по кнопке ниже. */}
      {pickerOpen && (
        <div style={{ marginBottom: 20 }}>
          <div
            role="radiogroup"
            aria-label={t('localEngine.presetLabel')}
            style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
          >
            {PRESETS.map((p, qi) => {
              const s = specs.find((x) => x.preset === p);
              return (
                <OptionCard
                  key={p}
                  radio
                  active={p === preset}
                  tabStop={p === preset}
                  title={t(`localEngine.preset.${p}`)}
                  badge={
                    hw.recommendation === p ? t('onboarding.engine.recommendedTag') : undefined
                  }
                  quality={qi + 1}
                  meta={<span className="mono">~{s ? gb(s.total_bytes) : '—'} GB</span>}
                  onClick={() => setChosenPreset(p)}
                />
              );
            })}
          </div>
        </div>
      )}

      <div
        style={{ display: 'flex', flexDirection: 'column', gap: 10, marginBottom: 32 }}
      >
        <Button variant="primary" onClick={() => void startDownload()}>
          {t('onboarding.engine.downloadCta', { size: gb(spec.total_bytes) })}
        </Button>
        <Button variant="ghost" onClick={() => setPreviewMode(true)}>
          {t('onboarding.engine.previewCta')}
        </Button>
        <Button variant="ghost" onClick={() => setPickerOpen((v) => !v)}>
          {pickerOpen
            ? t('onboarding.engine.collapsePickerCta')
            : t('onboarding.engine.chooseAnotherCta')}
        </Button>
      </div>
    </>
  );
}
