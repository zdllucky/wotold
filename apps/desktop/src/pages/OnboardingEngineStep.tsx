// [M12.7.3] Onboarding step «Engine setup» — обязательный шаг для новых
// macOS-юзеров между Owner (step 2) и Permissions+Consent (step 4).
//
// Design Gate alignment block — см. PR описание. Wotold v2 (uikit) классы:
// .panel (probe result card — content внутри родительского onboarding dialog,
// не сам dialog), .dot--{success,accent,muted}, .btn--{primary,ghost,quiet},
// .activity-strip pattern для progress.
//
// Flow (PRD §M12.7.3):
//   1. Probe выдаёт recommendation
//   2. User видит карточку «recommended preset · size · что входит»
//   3. Три кнопки:
//      - «Скачать и продолжить» → запуск download → progress → onAdvance
//      - «Выбрать другой» → раскрывает radio group Light/Balanced/Quality
//      - «Использовать облако вместо» → engine=cloud_managed, advance
//   4. Cancel во время download → cleanup partial + engine=cloud_managed + advance

import { useCallback, useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { HwReport, LocalEnginePreset } from '@wotold/contracts';

import {
  localEngineHwProbe,
  localEngineModelDelete,
  localEngineModelDownload,
  localEngineModelStatus,
  localEngineSetActiveEngine,
  localEngineSetActivePreset,
} from '../api/local-engine';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { EngineChip } from '../components/EngineChip';
import { Button, OptionCard, Progress } from '../ui';

const PRESETS: LocalEnginePreset[] = ['light', 'balanced', 'quality'];

const PRESET_MODELS: Record<LocalEnginePreset, { whisper: string; llm: string; sizeGb: number }> = {
  light: { whisper: 'whisper-small', llm: 'qwen25-1_5b', sizeGb: 1.2 },
  balanced: { whisper: 'whisper-medium', llm: 'qwen25-3b', sizeGb: 2.4 },
  quality: { whisper: 'whisper-large-v3', llm: 'qwen25-7b', sizeGb: 5.5 },
};

/** [M12-D5] Pyannote ~6 MB — добавляем в download queue для multi-speaker. */
const PYANNOTE_MODEL_ID = 'pyannote-segmentation';

interface ProgressState {
  modelId: string;
  pct: number;
  bytesDone: number;
  bytesTotal: number;
}

interface Props {
  /** Вызвать когда юзер завершил step — продвинуть onboarding дальше. */
  onAdvance: () => void;
}

export function OnboardingEngineStep({ onAdvance }: Props) {
  const { t } = useI18n();
  const [hw, setHw] = useState<HwReport | null>(null);
  const [chosenPreset, setChosenPreset] = useState<LocalEnginePreset | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [previewMode, setPreviewMode] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<ProgressState | null>(null);
  const [downloadQueue, setDownloadQueue] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const report = await localEngineHwProbe(false);
        setHw(report);
        setChosenPreset(report.recommendation ?? 'light');
      } catch (e) {
        setError(humanError(e));
      }
    })();
  }, []);

  useEffect(() => {
    // См. LocalEngineSection.tsx — тот же cancelled-flag pattern против
    // listen() unlisten leak при fast unmount.
    let cancelled = false;
    let unProgress: UnlistenFn | undefined;
    let unDone: UnlistenFn | undefined;
    (async () => {
      unProgress = await listen<{ id: string; pct: number; bytes_done: number; bytes_total: number }>(
        'model:progress',
        (e) => {
          setProgress({
            modelId: e.payload.id,
            pct: e.payload.pct,
            bytesDone: e.payload.bytes_done,
            bytesTotal: e.payload.bytes_total,
          });
        },
      );
      unDone = await listen<{ id: string; status: string; expected?: string; got?: string; message?: string }>(
        'model:done',
        (e) => {
          if (e.payload.status === 'verify_failed') {
            setError(t('onboarding.engine.verifyFailed', { id: e.payload.id }));
            setDownloading(false);
            return;
          }
          if (e.payload.status === 'io_error') {
            setError(e.payload.message ?? 'io error');
            setDownloading(false);
            return;
          }
          // success or already_present — drain queue
          setDownloadQueue((q) => q.slice(1));
        },
      );
      if (cancelled) {
        unProgress?.();
        unDone?.();
      }
    })();
    return () => {
      cancelled = true;
      unProgress?.();
      unDone?.();
    };
  }, [t]);

  // Drain queue: if downloading and queue non-empty, kick next download.
  useEffect(() => {
    if (!downloading) return;
    if (downloadQueue.length === 0) {
      // queue empty → done
      setDownloading(false);
      setProgress(null);
      onAdvance();
      return;
    }
    const next = downloadQueue[0];
    if (!next) return;
    void localEngineModelDownload(next).catch((e) => {
      setError(humanError(e));
      setDownloading(false);
    });
  }, [downloadQueue, downloading, onAdvance]);

  const startDownload = useCallback(async () => {
    if (!chosenPreset) return;
    setError(null);
    try {
      // Engine + preset фиксируем в settings ДО download — если cancel,
      // user остаётся на local engine с preset (модели докачаются позже).
      await localEngineSetActiveEngine('local');
      await localEngineSetActivePreset(chosenPreset);

      // Соберём список моделей которые нужно скачать (skip present).
      // Включаем pyannote для multi-speaker diarization (PRD §M12-D5).
      const models = PRESET_MODELS[chosenPreset];
      const ids = [models.whisper, models.llm, PYANNOTE_MODEL_ID];
      const queue: string[] = [];
      for (const id of ids) {
        const status = await localEngineModelStatus(id);
        if (status.state !== 'present') queue.push(id);
      }
      if (queue.length === 0) {
        // Всё уже скачано — сразу advance.
        onAdvance();
        return;
      }
      setDownloadQueue(queue);
      setDownloading(true);
    } catch (e) {
      setError(humanError(e));
    }
  }, [chosenPreset, onAdvance]);

  const useCloudInstead = useCallback(async () => {
    setError(null);
    try {
      await localEngineSetActiveEngine('cloud_managed');
      onAdvance();
    } catch (e) {
      setError(humanError(e));
    }
  }, [onAdvance]);

  const cancelDownload = useCallback(async () => {
    setError(null);
    setDownloading(false);
    setDownloadQueue([]);
    setProgress(null);
    // Cleanup partial files: delete the model that was mid-download.
    if (progress?.modelId) {
      try {
        await localEngineModelDelete(progress.modelId);
      } catch {
        // Best-effort; UI continues.
      }
    }
    try {
      await localEngineSetActiveEngine('cloud_managed');
    } catch (e) {
      setError(humanError(e));
      return;
    }
    onAdvance();
  }, [onAdvance, progress]);

  // [Review HIGH-3] Non-macOS auto-skip нельзя дёргать в render — это вызовет
  // `setStep` в родителе во время рендера ребёнка (React warning + риск
  // infinite re-render). Переносим в useEffect.
  // [B21] Хук обязан стоять ДО любого early-return (Rules of Hooks): раньше
  // он объявлялся после `if (downloading) return …` — первый же старт
  // загрузки менял число вызванных хуков → React crash.
  useEffect(() => {
    if (hw && hw.os !== 'macos') {
      onAdvance();
    }
  }, [hw, onAdvance]);

  if (downloading) {
    const pct = progress?.pct ?? 0;
    const mb = progress ? (progress.bytesDone / 1024 / 1024).toFixed(0) : '0';
    const totalMb = progress ? (progress.bytesTotal / 1024 / 1024).toFixed(0) : '?';
    return (
      <div
        className="activity-strip"
        role="status"
        style={{ marginBottom: 28, fontFamily: 'var(--font)' }}
      >
        <div style={{ flex: 1 }}>
          <div className="small-caps" style={{ marginBottom: 4 }}>
            {t('onboarding.engine.downloadingLabel', {
              id: progress?.modelId ?? '…',
            })}
          </div>
          <div className="mono" style={{ fontSize: 13, color: 'var(--text-2)' }}>
            {mb} / {totalMb} MB · {pct.toFixed(0)}%
          </div>
          {/* [B21] Канонный .progress вместо самописного бара. */}
          <Progress value={pct} style={{ marginTop: 8 }} />
        </div>
        <Button variant="ghost" size="sm" onClick={() => void cancelDownload()}>
          {t('onboarding.engine.cancelDownloadCta')}
        </Button>
      </div>
    );
  }

  if (!hw || hw.os !== 'macos') {
    return (
      <p className="subtle" style={{ marginBottom: 32 }}>
        {t('common.loadingShort')}
      </p>
    );
  }

  const preset = chosenPreset ?? 'light';
  const models = PRESET_MODELS[preset];
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
              {t(`onboarding.engine.previewTranscript${n}` as 'onboarding.engine.previewTranscript1')}
            </div>
          ))}
        </div>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            marginBottom: 24,
          }}
        >
          <EngineChip kind="local" variant="inline" />
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
            {t('onboarding.engine.previewInstall', { size: models.sizeGb })}
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
          style={{
            color: 'var(--danger)',
            fontFamily: 'var(--font)',
            marginBottom: 14,
          }}
        >
          {error}
        </p>
      )}

      {/* Probe result card */}
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
          <strong>{presetLabel}</strong> · ~{models.sizeGb} GB
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

      {/* Picker (collapsible) — [B21] OptionCard radio, как в Настройках. */}
      {pickerOpen && (
        <div style={{ marginBottom: 20 }}>
          <div
            role="radiogroup"
            aria-label={t('localEngine.presetLabel')}
            style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
          >
            {PRESETS.map((p, qi) => (
              <OptionCard
                key={p}
                radio
                active={p === preset}
                title={t(`localEngine.preset.${p}`)}
                badge={
                  hw.recommendation === p ? t('onboarding.engine.recommendedTag') : undefined
                }
                quality={qi + 1}
                meta={<span className="mono">~{PRESET_MODELS[p].sizeGb} GB</span>}
                onClick={() => setChosenPreset(p)}
              />
            ))}
          </div>
        </div>
      )}

      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 10,
          marginBottom: 32,
        }}
      >
        <Button variant="primary" onClick={() => void startDownload()}>
          {t('onboarding.engine.downloadCta', { size: models.sizeGb })}
        </Button>
        <Button variant="ghost" onClick={() => setPreviewMode(true)}>
          {t('onboarding.engine.previewCta')}
        </Button>
        <Button variant="ghost" onClick={() => setPickerOpen((v) => !v)}>
          {pickerOpen
            ? t('onboarding.engine.collapsePickerCta')
            : t('onboarding.engine.chooseAnotherCta')}
        </Button>
        <Button variant="ghost" onClick={() => void useCloudInstead()}>
          {t('onboarding.engine.useCloudCta')}
        </Button>
      </div>
    </>
  );
}
