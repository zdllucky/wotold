// [M12.5] Settings → «Движок распознавания».
//
// Atelier v2 alignment block см. PR описание / chat. Использует только
// existing tokens + классы (.field, .field-label, .dot, .btn, .activity-strip).
//
// Логика секции:
//   - Engine picker (Local / Cloud / BYO) — radiogroup. Atomic swap через
//     localEngineSetActiveEngine. Меняет следующую запись, не трогает
//     существующие (PRD §M12.6.6).
//   - Hardware probe banner — `.activity-strip`-style на первом mount'е
//     если recommendation != null и не совпадает с текущим preset'ом.
//   - При выбранном Local — preset picker (Light / Balanced / Quality)
//     с .dot--{success|accent|muted} статусом моделей.
//   - «Освободить место» — list-modal со статусом и кнопками удаления.
//
// События `model:progress`/`model:done` слушаются глобально для всех id;
// при completion → refresh status.

import { useCallback, useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import type {
  EngineKind,
  HwReport,
  LocalEnginePreset,
  ModelStatus,
  PresetSpec,
} from '@wotold/contracts';

import { DeleteModelConfirm } from '../components/DeleteModelConfirm';
import { RediscoveryChip } from '../components/RediscoveryChip';
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';
import {
  localEngineGetActiveEngine,
  localEngineGetActivePreset,
  localEngineHwProbe,
  localEngineListCatalog,
  localEngineModelDelete,
  localEngineModelDownload,
  localEngineModelStatus,
  localEngineSetActiveEngine,
  localEngineSetActivePreset,
  localEngineStorageList,
  type LocalEngineCatalogEntry,
  type LocalEngineStorageRow,
} from '../api/local-engine';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { modelLabel } from '../utils/modelLabel';
import { UsageSection } from './UsageSection';

const PRESETS: LocalEnginePreset[] = ['light', 'balanced', 'quality'];

// [PRD §11 O1 deviation] Gemma → Qwen 1.5B (Google TOS gating). Должен 1:1
// совпадать с Rust `LocalEnginePreset::llm_model_id` в [local_engine/preset.rs](../../../src-tauri/src/local_engine/preset.rs)
// и `PRESET_MODELS` в [OnboardingEngineStep.tsx](OnboardingEngineStep.tsx) —
// расхождение приведёт к UI «модель не установлена» сразу после download'а
// в onboarding'е. Регрессия покрыта Rust-тестом `light_preset_uses_qwen_not_gemma`.
const PRESET_TO_MODELS: Record<LocalEnginePreset, { whisper: string; llm: string }> = {
  light: { whisper: 'whisper-small', llm: 'qwen25-1_5b' },
  balanced: { whisper: 'whisper-medium', llm: 'qwen25-3b' },
  quality: { whisper: 'whisper-large-v3', llm: 'qwen25-7b' },
};

/** [M12-D5] Optional модель для multi-speaker diarization (degraded без неё). */
const PYANNOTE_MODEL_ID = 'pyannote-segmentation';

interface ModelProgress {
  pct: number;
  bytesDone: number;
  bytesTotal: number;
}

function formatGB(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

/** Compact ISO → «5 мин», «вчера», «12 мая» формат. См. _atelier-2.jsx паттерн. */
function formatLastUsed(iso: string): string {
  const d = new Date(iso);
  const now = Date.now();
  const diffMs = now - d.getTime();
  const min = Math.floor(diffMs / 60_000);
  if (min < 60) return `${Math.max(1, min)}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.floor(hr / 24);
  if (day < 30) return `${day}d`;
  return d.toLocaleDateString();
}

function dotClassForStatus(status: ModelStatus | null, progress: ModelProgress | null): string {
  if (progress) return 'dot--accent dot--pulse';
  if (!status) return 'dot--muted';
  if (status.state === 'present') return 'dot--success';
  if (status.state === 'corrupted') return 'dot--warning';
  return 'dot--muted';
}

export function LocalEngineSection() {
  const { t } = useI18n();
  const [engine, setEngine] = useState<EngineKind | null>(null);
  const [preset, setPreset] = useState<PresetSpec | null>(null);
  const [catalog, setCatalog] = useState<LocalEngineCatalogEntry[]>([]);
  const [statuses, setStatuses] = useState<Record<string, ModelStatus>>({});
  const [progresses, setProgresses] = useState<Record<string, ModelProgress>>({});
  const [hw, setHw] = useState<HwReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [storageRows, setStorageRows] = useState<LocalEngineStorageRow[]>([]);
  // [M12-v1.1] hwLoading — пока true рендерим skeleton вместо engine rows.
  const [hwLoading, setHwLoading] = useState(true);
  // [M12-v1.1] deleteConfirmModel — pending confirm для активной модели.
  const [deleteConfirmModel, setDeleteConfirmModel] = useState<{
    id: string;
    fallbackPreset: string;
    modelRole: string;
    currentPreset: string;
  } | null>(null);
  // [M12-v1.1] rediscovery chip — показываем когда engine !== 'local'
  // И invite не dismissed permanently.
  const [showRediscovery, setShowRediscovery] = useState(false);

  const refreshStatuses = useCallback(async (ids: string[]) => {
    const entries = await Promise.all(
      ids.map(async (id) => [id, await localEngineModelStatus(id)] as const),
    );
    setStatuses((prev) => {
      const next = { ...prev };
      for (const [id, status] of entries) {
        next[id] = status;
      }
      return next;
    });
  }, []);

  const refreshAll = useCallback(async () => {
    setHwLoading(true);
    try {
      const [e, p, c, h, rows] = await Promise.all([
        localEngineGetActiveEngine(),
        localEngineGetActivePreset(),
        localEngineListCatalog(),
        localEngineHwProbe(false),
        localEngineStorageList().catch(() => [] as LocalEngineStorageRow[]),
      ]);
      setEngine(e);
      setPreset(p);
      setCatalog(c);
      setHw(h);
      setStorageRows(rows);
      await refreshStatuses(c.map((m) => m.id));
      // [M12-v1.1] Rediscovery: show when not local + invite not dismissed.
      if (e !== 'local') {
        const dismissed = await getSetting(SETTINGS_KEYS.LOCAL_ENGINE_INVITE_DISMISSED).catch(() => null);
        if (!dismissed) setShowRediscovery(true);
      }
      setError(null);
    } catch (e) {
      setError(humanError(e));
    } finally {
      setHwLoading(false);
    }
  }, [refreshStatuses]);

  useEffect(() => {
    void refreshAll();
  }, [refreshAll]);

  const refreshStorage = useCallback(async () => {
    try {
      const rows = await localEngineStorageList();
      setStorageRows(rows);
    } catch (e) {
      setError(humanError(e));
    }
  }, []);

  useEffect(() => {
    // [Review HIGH-2] React 18 StrictMode + fast unmount race: `listen()` —
    // async; если cleanup сработает до резолва promise, UnlistenFn остаётся
    // не вызванной → listener leak до конца сессии. Решение — `cancelled`
    // флаг: дождались резолва, проверили cancelled, тогда сразу очистили.
    let cancelled = false;
    let unProgress: UnlistenFn | undefined;
    let unDone: UnlistenFn | undefined;
    (async () => {
      unProgress = await listen<{ id: string; pct: number; bytes_done: number; bytes_total: number }>(
        'model:progress',
        (e) => {
          setProgresses((prev) => ({
            ...prev,
            [e.payload.id]: {
              pct: e.payload.pct,
              bytesDone: e.payload.bytes_done,
              bytesTotal: e.payload.bytes_total,
            },
          }));
        },
      );
      unDone = await listen<{ id: string; status: string; expected?: string; got?: string; message?: string }>(
        'model:done',
        (e) => {
          const id = e.payload.id;
          setProgresses((prev) => {
            const next = { ...prev };
            delete next[id];
            return next;
          });
          if (e.payload.status === 'verify_failed') {
            setError(t('localEngine.verifyFailed', { id }));
          } else if (e.payload.status === 'io_error' && e.payload.message) {
            setError(e.payload.message);
          }
          void refreshStatuses([id]);
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
  }, [refreshStatuses, t]);

  const onEngineChange = useCallback(
    async (next: EngineKind) => {
      try {
        const saved = await localEngineSetActiveEngine(next);
        setEngine(saved);
      } catch (e) {
        setError(humanError(e));
      }
    },
    [],
  );

  const onPresetChange = useCallback(
    async (next: LocalEnginePreset) => {
      // PRD §M12.5.4: Quality на железе <16 GB RAM → confirm modal.
      if (next === 'quality' && hw && hw.ram_gb < 16) {
        const ok = await ask(t('localEngine.qualityConfirmMsg'), {
          title: t('localEngine.qualityConfirmTitle'),
          kind: 'warning',
        });
        if (!ok) return;
      }
      try {
        const saved = await localEngineSetActivePreset(next);
        setPreset(saved);
        // Авто-старт download'ов на отсутствующие модели preset'а +
        // pyannote (shared, optional но рекомендованная).
        const needed = PRESET_TO_MODELS[next];
        for (const id of [needed.whisper, needed.llm, PYANNOTE_MODEL_ID]) {
          const status = statuses[id];
          if (!status || status.state === 'absent' || status.state === 'corrupted') {
            void localEngineModelDownload(id).catch((err) => setError(humanError(err)));
          }
        }
      } catch (e) {
        setError(humanError(e));
      }
    },
    [hw, statuses, t],
  );

  const onDownload = useCallback(
    async (id: string) => {
      try {
        await localEngineModelDownload(id);
        // refresh после done event.
      } catch (e) {
        setError(humanError(e));
      }
    },
    [],
  );

  /**
   * [M12.5.4] Удаление модели из Storage UI. Если модель активна сейчас
   * (входит в текущий preset) — confirm-modal жёстче: «Эта модель используется,
   * удаление переключит preset на ...». Если других установленных preset'ов
   * нет — UI должен сначала предложить switch на Cloud (deferred — пока
   * показываем generic warning).
   */
  const onDeleteFromStorage = useCallback(
    async (id: string, isActive: boolean, after: () => void) => {
      if (isActive && preset) {
        // [M12-v1.1] Use inline modal instead of native ask() for active models.
        const fallbackIdx = PRESETS.indexOf(preset.preset as LocalEnginePreset);
        const fallbackKey: LocalEnginePreset =
          fallbackIdx > 0 ? PRESETS[fallbackIdx - 1] ?? 'light' : 'light';
        const fallbackPreset = t(`localEngine.preset.${fallbackKey}`);
        setDeleteConfirmModel({
          id,
          fallbackPreset,
          modelRole: modelLabel(id, t),
          currentPreset: t(`localEngine.preset.${preset.preset as LocalEnginePreset}`),
        });
        // after() will be called by the confirm modal's onConfirm handler.
        return;
      }
      const ok = await ask(t('localEngine.deleteConfirmMsg', { id }), {
        title: t('localEngine.deleteConfirmTitle'),
        kind: 'warning',
      });
      if (!ok) return;
      try {
        await localEngineModelDelete(id);
        after();
      } catch (e) {
        setError(humanError(e));
      }
    },
    [t, preset],
  );

  const totalInstalledBytes = catalog.reduce((sum, m) => {
    const s = statuses[m.id];
    if (s && s.state === 'present') return sum + s.bytes_total;
    return sum;
  }, 0);

  const showHwBanner =
    !bannerDismissed &&
    hw &&
    hw.recommendation !== null &&
    preset?.preset !== hw.recommendation;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 28, maxWidth: 640 }}>
      {deleteConfirmModel && (
        <DeleteModelConfirm
          modelRole={deleteConfirmModel.modelRole}
          currentPreset={deleteConfirmModel.currentPreset}
          fallbackPreset={deleteConfirmModel.fallbackPreset}
          onConfirm={async () => {
            try {
              await localEngineModelDelete(deleteConfirmModel.id);
              setDeleteConfirmModel(null);
              await refreshStorage();
            } catch (e) {
              setError(humanError(e));
              setDeleteConfirmModel(null);
            }
          }}
          onCancel={() => setDeleteConfirmModel(null)}
        />
      )}

      {showRediscovery && (
        <RediscoveryChip
          onInstall={() => {
            setShowRediscovery(false);
            void onEngineChange('local');
          }}
          onTerminalDismiss={async () => {
            setShowRediscovery(false);
            await setSetting(SETTINGS_KEYS.LOCAL_ENGINE_INVITE_DISMISSED, '1').catch(() => {});
          }}
        />
      )}

      {error && (
        <p
          role="alert"
          style={{
            color: 'var(--signal)',
            fontFamily: 'var(--font-sans)',
            margin: 0,
          }}
        >
          {error}
        </p>
      )}

      {hwLoading && (
        <div
          className="activity-strip"
          role="status"
          aria-label={t('localEngine.probeSkeleton.measuring')}
          style={{ flexDirection: 'column', gap: 10, alignItems: 'stretch' }}
        >
          <p className="muted" style={{ fontSize: 12, margin: 0 }}>
            {t('localEngine.probeSkeleton.measuring')}
          </p>
          <div className="probe-skeleton-row" style={{ width: '75%' }} />
          <div className="probe-skeleton-row" style={{ width: '55%' }} />
          <div className="probe-skeleton-row" style={{ width: '40%' }} />
        </div>
      )}

      {showHwBanner && hw && hw.recommendation && (
        <div className="activity-strip" role="status">
          <div>
            <div className="small-caps">{t('localEngine.hwBannerTitle')}</div>
            <div style={{ fontFamily: 'var(--font-serif)', fontSize: 14, color: 'var(--ink-2)' }}>
              {t('localEngine.hwBannerBody', {
                preset: t(`localEngine.preset.${hw.recommendation}`),
                cpu: hw.cpu_model,
                ram: hw.ram_gb,
              })}
            </div>
          </div>
          <button
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => {
              if (hw.recommendation) void onPresetChange(hw.recommendation);
              setBannerDismissed(true);
            }}
          >
            {t('localEngine.hwBannerApply')}
          </button>
          <button
            type="button"
            className="btn btn--quiet btn--sm"
            onClick={() => setBannerDismissed(true)}
          >
            {t('localEngine.hwBannerDismiss')}
          </button>
        </div>
      )}

      {!hwLoading && (
        <div className="field">
          <label className="field-label" id="local-engine-kind-label">
            {t('localEngine.engineLabel')}
          </label>
          <div
            role="radiogroup"
            aria-labelledby="local-engine-kind-label"
            style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
          >
            {(['cloud_managed', 'local'] as EngineKind[]).map((k) => {
              const active = engine === k;
              return (
                <label
                  key={k}
                  style={{
                    display: 'flex',
                    alignItems: 'flex-start',
                    gap: 12,
                    padding: 12,
                    borderRadius: 'var(--radius-card, 8px)',
                    border: active ? '1.5px solid var(--accent)' : '1px solid var(--line-soft)',
                    background: active ? 'var(--accent-soft)' : 'transparent',
                    cursor: 'pointer',
                    fontFamily: 'var(--font-sans)',
                  }}
                >
                  <input
                    type="radio"
                    name="local-engine-kind"
                    checked={active}
                    onChange={() => void onEngineChange(k)}
                    style={{ marginTop: 4 }}
                  />
                  <div style={{ flex: 1 }}>
                    <div style={{ fontWeight: 500, color: 'var(--ink)', marginBottom: 4 }}>
                      {t(`localEngine.engine.${k}.title`)}
                    </div>
                    <div style={{ fontSize: 13, color: 'var(--subtle)', lineHeight: 1.5 }}>
                      {t(`localEngine.engine.${k}.body`)}
                    </div>
                    <div
                      className="mono"
                      style={{ fontSize: 11, color: 'var(--subtle)', marginTop: 6 }}
                    >
                      {t(`localEngine.engine.${k}.quality`)}
                    </div>
                  </div>
                  {active && (
                    <span className="badge badge--active" style={{ alignSelf: 'center', flexShrink: 0 }}>
                      <span className="dot" style={{ background: 'var(--success)', width: 5, height: 5 }} aria-hidden />
                      {t('localEngine.engine.active')}
                    </span>
                  )}
                </label>
              );
            })}
          </div>
        </div>
      )}

      {!hwLoading && engine === 'cloud_managed' && (
        <div style={{ marginTop: 4 }}>
          <UsageSection />
        </div>
      )}

      {!hwLoading && engine === 'local' && hw && (
        <div
          className="subtle"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            fontSize: 12,
            fontFamily: 'var(--font-sans)',
          }}
        >
          <span>
            {t('localEngine.probeSummary', {
              cpu: hw.cpu_model,
              ram: hw.ram_gb,
              metal: hw.metal_supported
                ? t('localEngine.probeMetalYes')
                : t('localEngine.probeMetalNo'),
              preset: hw.recommendation
                ? t(`localEngine.preset.${hw.recommendation}`)
                : '—',
            })}
          </span>
          <button
            type="button"
            className="btn btn--quiet btn--sm"
            onClick={async () => {
              try {
                const next = await localEngineHwProbe(true);
                setHw(next);
              } catch (e) {
                setError(humanError(e));
              }
            }}
          >
            {t('localEngine.reprobe')}
          </button>
        </div>
      )}

      {!hwLoading && engine === 'local' && (
        <div className="field">
          <label className="field-label" id="local-engine-preset-label">
            {t('localEngine.presetLabel')}
          </label>
          <div
            role="radiogroup"
            aria-labelledby="local-engine-preset-label"
            style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
          >
            {PRESETS.map((p) => {
              const active = preset?.preset === p;
              const models = PRESET_TO_MODELS[p];
              const whisperStatus = statuses[models.whisper];
              const llmStatus = statuses[models.llm];
              const whisperProgress = progresses[models.whisper];
              const llmProgress = progresses[models.llm];
              const allPresent =
                whisperStatus?.state === 'present' && llmStatus?.state === 'present';
              const anyDownloading = !!whisperProgress || !!llmProgress;
              const totalSize =
                (whisperStatus?.bytes_total ?? 0) + (llmStatus?.bytes_total ?? 0);
              const isRecommended = hw?.recommendation === p;
              return (
                <label
                  key={p}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 12,
                    padding: 12,
                    borderRadius: 'var(--radius-card, 8px)',
                    border: active ? '1.5px solid var(--accent)' : '1px solid var(--line-soft)',
                    background: active ? 'var(--accent-soft)' : 'transparent',
                    cursor: 'pointer',
                    fontFamily: 'var(--font-sans)',
                  }}
                >
                  <input
                    type="radio"
                    name="local-engine-preset"
                    checked={active}
                    onChange={() => void onPresetChange(p)}
                  />
                  <span
                    className={`dot ${allPresent ? 'dot--success' : anyDownloading ? 'dot--accent dot--pulse' : 'dot--muted'}`}
                    aria-hidden
                  />
                  <div style={{ flex: 1 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                      <span style={{ fontWeight: 500, color: 'var(--ink)' }}>
                        {t(`localEngine.preset.${p}`)}
                      </span>
                      {isRecommended && (
                        <span className="badge badge--recommend">
                          {t('localEngine.presetRecommend')}
                        </span>
                      )}
                    </div>
                    <div className="mono" style={{ fontSize: 11, color: 'var(--subtle)', marginTop: 2 }}>
                      {t(`localEngine.presetMeta.${p}`)}{' · '}
                      {totalSize > 0 ? formatGB(totalSize) : '—'}{' · '}
                      {allPresent
                        ? t('localEngine.statusInstalled')
                        : anyDownloading
                          ? t('localEngine.statusDownloading')
                          : t('localEngine.statusAbsent')}
                    </div>
                  </div>
                </label>
              );
            })}
          </div>

          <div style={{ marginTop: 6 }}>
            <span className="subtle" style={{ fontSize: 12 }}>
              {t('localEngine.installedFootprint', { size: formatGB(totalInstalledBytes) })}
            </span>
          </div>
        </div>
      )}

      {!hwLoading && engine === 'local' && storageRows.length > 0 && (
        <div className="field">
          <div
            className="field-label small-caps"
            id="local-engine-storage-title"
            style={{ marginBottom: 10 }}
          >
            {t('localEngine.storageTitle')}
          </div>
          <p
            className="subtle"
            style={{ margin: 0, marginBottom: 12, fontSize: 12, fontFamily: 'var(--font-sans)' }}
          >
            {t('localEngine.storageLede')}
          </p>
          {/* [M12.4.4-bis] Таблица inline: name · size · last_used · active badge · × */}
          <div role="table" style={{ display: 'flex', flexDirection: 'column' }}>
            <div
              role="row"
              style={{
                display: 'grid',
                gridTemplateColumns: '1fr 70px 90px 100px 28px',
                gap: 10,
                padding: '6px 0',
                borderBottom: '1px solid var(--line-soft)',
              }}
            >
              <span className="small-caps">{t('localEngine.colName')}</span>
              <span className="small-caps">{t('localEngine.colSize')}</span>
              <span className="small-caps">{t('localEngine.colLastUsed')}</span>
              <span className="small-caps">{t('localEngine.colState')}</span>
              <span />
            </div>
            {storageRows.map((row) => {
              const progress = progresses[row.id];
              const status = row.status;
              return (
                <div
                  key={row.id}
                  role="row"
                  style={{
                    display: 'grid',
                    gridTemplateColumns: '1fr 70px 90px 100px 28px',
                    gap: 10,
                    padding: '10px 0',
                    borderBottom: '1px solid var(--line-soft)',
                    alignItems: 'center',
                    fontFamily: 'var(--font-sans)',
                    fontSize: 13,
                  }}
                >
                  <div style={{ color: 'var(--ink)' }}>{modelLabel(row.id, t)}</div>
                  <span className="mono" style={{ fontSize: 11, color: 'var(--subtle)' }}>
                    {formatGB(row.size_bytes)}
                  </span>
                  <span className="mono" style={{ fontSize: 11, color: 'var(--subtle)' }}>
                    {row.last_used_at ? formatLastUsed(row.last_used_at) : '—'}
                  </span>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    {row.is_active && status.state === 'present' ? (
                      <span className="badge badge--active">
                        <span className="dot" style={{ background: 'var(--success)', width: 5, height: 5 }} aria-hidden />
                        {t('localEngine.statusActive')}
                      </span>
                    ) : (
                      <>
                        <span
                          className={`dot ${dotClassForStatus(status, progress ?? null)}`}
                          aria-hidden
                        />
                        <span style={{ fontSize: 11, color: 'var(--subtle)' }}>
                          {status.state === 'present'
                            ? t('localEngine.statusInstalled')
                            : progress
                              ? `${progress.pct.toFixed(0)}%`
                              : status.state === 'corrupted'
                                ? t('localEngine.statusCorrupted')
                                : t('localEngine.statusAbsent')}
                        </span>
                      </>
                    )}
                  </div>
                  {status.state === 'present' ? (
                    <button
                      type="button"
                      className="btn btn--quiet btn--sm"
                      aria-label={t('localEngine.deleteAria', { name: modelLabel(row.id, t) })}
                      title={t('localEngine.delete')}
                      onClick={() =>
                        void onDeleteFromStorage(row.id, row.is_active, () => {
                          void refreshStorage();
                          void refreshStatuses([row.id]);
                        })
                      }
                    >
                      ×
                    </button>
                  ) : !progress ? (
                    <button
                      type="button"
                      className="btn btn--ghost btn--sm"
                      aria-label={t('localEngine.downloadAria', { name: modelLabel(row.id, t) })}
                      title={t('localEngine.download')}
                      onClick={() => void onDownload(row.id)}
                    >
                      ↓
                    </button>
                  ) : (
                    <span />
                  )}
                </div>
              );
            })}
          </div>
          <p className="subtle" style={{ fontSize: 11, marginTop: 12 }}>
            {t('localEngine.storageFootnote')}
          </p>
        </div>
      )}
    </div>
  );
}
