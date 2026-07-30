// [M12.5] Settings → «Движок распознавания».
//
// Wotold v2 (uikit) alignment block см. PR описание / chat (B18.5c). Использует
// только uikit-токены + классы (.field, .optioncard, .set-table, .panel, .wave,
// .chip, .dot, .btn, .iconbtn). Иконки — <Icon/>.
//
// Логика секции (local-only движок — единственный):
//   - Hardware probe banner — accent-soft .panel на первом mount'е если
//     recommendation != null и не совпадает с текущим preset'ом.
//   - Preset picker (Light / Balanced / Quality) на .optioncard со
//     статус-.dot (ok/accent-pulse/faint) для моделей.
//   - «Освободить место» — .set-table со статусом и кнопками удаления.
//
// События `model:progress`/`model:done` слушаются глобально для всех id;
// при completion → refresh status.

import { useCallback, useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import type {
  HwReport,
  LocalEnginePreset,
  ModelStatus,
  PresetSizeSpec,
  PresetSpec,
} from '@wotold/contracts';

import { DeleteModelConfirm } from '../components/DeleteModelConfirm';
import {
  Button,
  Dot,
  GroupLabel,
  Icon,
  IconBtn,
  OptionCard,
  Skeleton,
  SettingRow,
  Switch,
  Wave,
} from '../ui';
import {
  localEngineGetActivePreset,
  localEngineHwProbe,
  localEngineListCatalog,
  localEngineModelDelete,
  localEngineEnsureRequired,
  localEngineModelDownload,
  localEnginePresetSpecs,
  localEngineModelStatus,
  localEngineGetKeepResident,
  localEngineSetActivePreset,
  localEngineSetKeepResident,
  localEngineStorageList,
  type LocalEngineCatalogEntry,
  type LocalEngineStorageRow,
} from '../api/local-engine';
import { humanError } from '../api/errors';
import { useI18n } from '../i18n';
import { modelLabel } from '../utils/modelLabel';

const PRESETS: LocalEnginePreset[] = ['light', 'balanced', 'quality'];

// Какие модели нужны для размера движка, знает бэкенд
// (`local_engine::readiness::required_ids`). Здесь этой раскладки больше нет:
// её копии в настройках и онбординге молча расходились с Rust, и расхождение
// читалось как «скачал, а оно пишет что модели нет».

interface ModelProgress {
  pct: number;
  bytesDone: number;
  bytesTotal: number;
}

/**
 * Размер файла модели. Гигабайты только когда их правда больше одного:
 * мелкие модули (VAD 0.9 MB, pyannote 6 MB, эмбеддер 26 MB) в таблице все
 * показывались как «0.0 GB» и выглядели пустыми строками.
 */
function formatGB(bytes: number): string {
  if (bytes < 1024 ** 3) return `${Math.max(1, Math.round(bytes / 1024 ** 2))} MB`;
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

function dotStyleForStatus(
  status: ModelStatus | null,
  progress: ModelProgress | null,
): { background: string; pulse: boolean } {
  if (progress) return { background: 'var(--accent)', pulse: true };
  if (!status) return { background: 'var(--text-faint)', pulse: false };
  if (status.state === 'present') return { background: 'var(--ok)', pulse: false };
  if (status.state === 'corrupted') return { background: 'var(--warn)', pulse: false };
  return { background: 'var(--text-faint)', pulse: false };
}

export function LocalEngineSection() {
  const { t } = useI18n();
  const [preset, setPreset] = useState<PresetSpec | null>(null);
  // [B2] Тумблер «держать модель активной» (resident llama-server).
  const [keepResident, setKeepResident] = useState(false);
  // [B25] Тумблер «Семантический поиск ассистента» (default on).
  const [catalog, setCatalog] = useState<LocalEngineCatalogEntry[]>([]);
  const [specs, setSpecs] = useState<PresetSizeSpec[]>([]);
  const [statuses, setStatuses] = useState<Record<string, ModelStatus>>({});
  const [progresses, setProgresses] = useState<Record<string, ModelProgress>>({});
  const [hw, setHw] = useState<HwReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [storageRows, setStorageRows] = useState<LocalEngineStorageRow[]>([]);
  // [M12-v1.1] hwLoading — пока true рендерим skeleton вместо конфигурации движка.
  const [hwLoading, setHwLoading] = useState(true);
  // [M12-v1.1] deleteConfirmModel — pending confirm для активной модели.
  const [deleteConfirmModel, setDeleteConfirmModel] = useState<{
    id: string;
    fallbackPreset: string;
    modelRole: string;
    currentPreset: string;
  } | null>(null);

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
      const [p, c, h, rows, resident, sizeSpecs] = await Promise.all([
        localEngineGetActivePreset(),
        localEngineListCatalog(),
        localEngineHwProbe(false),
        localEngineStorageList().catch(() => [] as LocalEngineStorageRow[]),
        localEngineGetKeepResident().catch(() => false),
        localEnginePresetSpecs(),
      ]);
      setPreset(p);
      setCatalog(c);
      setSpecs(sizeSpecs);
      setHw(h);
      setStorageRows(rows);
      setKeepResident(resident);
      await refreshStatuses(c.map((m) => m.id));
      setError(null);
    } catch (e) {
      setError(humanError(e, t));
    } finally {
      setHwLoading(false);
    }
  }, [refreshStatuses]);

  useEffect(() => {
    void refreshAll();
  }, [refreshAll]);

  const refreshStorage = useCallback(async () => {
    try {
      setStorageRows(await localEngineStorageList());
    } catch (e) {
      setError(humanError(e, t));
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
        // Докачку недостающего запускает бэкенд по единому обязательному
        // списку — фронт больше не перебирает модели сам.
        await localEngineEnsureRequired();
      } catch (e) {
        setError(humanError(e, t));
      }
    },
    [hw, t],
  );

  const onDownload = useCallback(
    async (id: string) => {
      try {
        await localEngineModelDownload(id);
        // refresh после done event.
      } catch (e) {
        setError(humanError(e, t));
      }
    },
    [t],
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
        setError(humanError(e, t));
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
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
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
              setError(humanError(e, t));
              setDeleteConfirmModel(null);
            }
          }}
          onCancel={() => setDeleteConfirmModel(null)}
        />
      )}

      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: 0 }}>
          {error}
        </p>
      )}

      {hwLoading && (
        <div
          className="panel"
          role="status"
          aria-label={t('localEngine.probeSkeleton.measuring')}
          style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: '12px 16px', maxWidth: 560 }}
        >
          <p style={{ fontSize: 12, margin: 0, color: 'var(--text-2)' }}>
            {t('localEngine.probeSkeleton.measuring')}
          </p>
          <Skeleton width="75%" height="12px" />
          <Skeleton width="55%" height="12px" />
          <Skeleton width="40%" height="12px" />
        </div>
      )}

      {showHwBanner && hw && hw.recommendation && (
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
              if (hw.recommendation) void onPresetChange(hw.recommendation);
              setBannerDismissed(true);
            }}
          >
            {t('localEngine.hwBannerApply')}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setBannerDismissed(true)}>
            {t('localEngine.hwBannerDismiss')}
          </Button>
        </div>
      )}

      {/* [B21] Канон :183-187 — sunken-плашка: Icon cpu + mono-спеки + ghost. */}
      {!hwLoading && hw && (
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
              preset: hw.recommendation
                ? t(`localEngine.preset.${hw.recommendation}`)
                : '—',
            })}
          </span>
          <Button
            variant="ghost"
            size="sm"
            style={{ marginLeft: 'auto' }}
            leading={<Icon name="refresh" size={13} />}
            onClick={() => {
              void (async () => {
                try {
                  const next = await localEngineHwProbe(true);
                  setHw(next);
                } catch (e) {
                  setError(humanError(e, t));
                }
              })();
            }}
          >
            {t('localEngine.reprobe')}
          </Button>
        </div>
      )}

      {!hwLoading && (
        <SettingRow
          label={t('localEngine.keepResidentLabel')}
          hint={t('localEngine.keepResidentHint')}
          align="top"
          control={
            <Switch
              checked={keepResident}
              label={t('localEngine.keepResidentLabel')}
              onChange={async (next) => {
                setKeepResident(next); // optimistic
                try {
                  await localEngineSetKeepResident(next);
                } catch (err) {
                  setKeepResident(!next); // revert on failure
                  setError(humanError(err, t));
                }
              }}
            />
          }
        />
      )}

      {!hwLoading && (
        <div>
          <GroupLabel>{t('localEngine.presetLabel')}</GroupLabel>
          <div
            role="radiogroup"
            aria-label={t('localEngine.presetLabel')}
            style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
          >
            {PRESETS.map((p, qi) => {
              // Раскладку моделей и полный размер (включая обязательные
              // базовые модули) считает бэкенд по каталогу.
              const spec = specs.find((s) => s.preset === p);
              const whisperStatus = spec ? statuses[spec.whisper_model_id] : undefined;
              const llmStatus = spec ? statuses[spec.llm_model_id] : undefined;
              const anyDownloading = spec
                ? !!progresses[spec.whisper_model_id] || !!progresses[spec.llm_model_id]
                : false;
              const allPresent =
                whisperStatus?.state === 'present' && llmStatus?.state === 'present';
              const totalSize = spec?.total_bytes ?? 0;
              return (
                <OptionCard
                  key={p}
                  radio
                  active={preset?.preset === p}
                  // Пресет ещё не выбран (свежая установка) — табостановку
                  // держит первый вариант, иначе в группу не войти с клавиатуры.
                  tabStop={preset ? preset.preset === p : qi === 0}
                  title={t(`localEngine.preset.${p}`)}
                  badge={
                    hw?.recommendation === p ? t('localEngine.presetRecommend') : undefined
                  }
                  quality={qi + 1}
                  meta={
                    <span className="mono">
                      {t(`localEngine.presetMeta.${p}`)}
                      {' · '}
                      {totalSize > 0 ? formatGB(totalSize) : '—'}
                      {' · '}
                      {allPresent
                        ? t('localEngine.statusInstalled')
                        : anyDownloading
                          ? t('localEngine.statusDownloading')
                          : t('localEngine.statusAbsent')}
                    </span>
                  }
                  onClick={() => void onPresetChange(p)}
                />
              );
            })}
          </div>

          <p className="set-hint">
            {t('localEngine.installedFootprint', { size: formatGB(totalInstalledBytes) })}
          </p>
        </div>
      )}

      {!hwLoading && storageRows.length > 0 && (
        <div>
          <GroupLabel>{t('localEngine.storageTitle')}</GroupLabel>
          <p className="set-hint" style={{ marginTop: 0, marginBottom: 12 }}>
            {t('localEngine.storageLede')}
          </p>
          {/* [M12.4.4-bis] Таблица inline: name · size · last_used · active badge · × */}
          <div className="set-table" role="table">
            <div className="set-trow set-thead" role="row">
              <span role="columnheader" style={{ flex: '1 1 auto', minWidth: 0 }}>
                {t('localEngine.colName')}
              </span>
              <span role="columnheader" style={{ flex: '0 0 64px' }}>
                {t('localEngine.colSize')}
              </span>
              <span role="columnheader" style={{ flex: '0 0 70px' }}>
                {t('localEngine.colLastUsed')}
              </span>
              <span role="columnheader" style={{ flex: '0 0 96px' }}>
                {t('localEngine.colState')}
              </span>
              <span role="columnheader" style={{ flex: '0 0 28px' }} />
            </div>
            {storageRows.map((row) => {
              const progress = progresses[row.id];
              const status = row.status;
              const dot = dotStyleForStatus(status, progress ?? null);
              return (
                <div key={row.id} className="set-trow" role="row">
                  {/* [B22] u-trunc: имя не переносится и не наезжает на колонки. */}
                  <div
                    className="u-trunc"
                    title={modelLabel(row.id, t)}
                    style={{ flex: '1 1 auto', minWidth: 0, color: 'var(--text)' }}
                  >
                    {modelLabel(row.id, t)}
                  </div>
                  <span className="mono" style={{ flex: '0 0 64px', color: 'var(--text-3)' }}>
                    {formatGB(row.size_bytes)}
                  </span>
                  <span className="mono" style={{ flex: '0 0 70px', color: 'var(--text-3)' }}>
                    {row.last_used_at ? formatLastUsed(row.last_used_at) : '—'}
                  </span>
                  <div style={{ flex: '0 0 96px', display: 'flex', alignItems: 'center', gap: 6 }}>
                    {row.is_active && status.state === 'present' ? (
                      <span className="chip chip--accent" data-size="sm">
                        {t('localEngine.statusActive')}
                      </span>
                    ) : progress ? (
                      // [B21] Канон загрузки: Dot ring pulse + accent-text %.
                      <span
                        style={{
                          display: 'inline-flex',
                          alignItems: 'center',
                          gap: 6,
                          color: 'var(--accent-text)',
                          fontSize: 11.5,
                        }}
                      >
                        <Dot ring pulse color="var(--accent)" />
                        {progress.pct.toFixed(0)}%
                      </span>
                    ) : (
                      <>
                        <span
                          className={'dot' + (dot.pulse ? ' dot--pulse' : '')}
                          style={{ background: dot.background }}
                          aria-hidden
                        />
                        <span style={{ fontSize: 11, color: 'var(--text-3)' }}>
                          {status.state === 'present'
                            ? t('localEngine.statusInstalled')
                            : status.state === 'corrupted'
                              ? t('localEngine.statusCorrupted')
                              : t('localEngine.statusAbsent')}
                        </span>
                      </>
                    )}
                  </div>
                  <div style={{ flex: '0 0 28px' }}>
                    {status.state === 'present' ? (
                      <IconBtn
                        icon="trash"
                        size="sm"
                        iconSize={14}
                        label={t('localEngine.deleteAria', { name: modelLabel(row.id, t) })}
                        title={t('localEngine.delete')}
                        onClick={() =>
                          void onDeleteFromStorage(row.id, row.is_active, () => {
                            void refreshStorage();
                            void refreshStatuses([row.id]);
                          })
                        }
                      />
                    ) : !progress ? (
                      <IconBtn
                        icon="download"
                        size="sm"
                        iconSize={14}
                        label={t('localEngine.downloadAria', { name: modelLabel(row.id, t) })}
                        title={t('localEngine.download')}
                        onClick={() => void onDownload(row.id)}
                      />
                    ) : null}
                  </div>
                </div>
              );
            })}
          </div>
          <p className="set-hint">{t('localEngine.storageFootnote')}</p>
        </div>
      )}
    </div>
  );
}
