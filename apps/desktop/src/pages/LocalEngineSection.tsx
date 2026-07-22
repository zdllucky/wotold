// [M12.5] Settings → «Движок распознавания».
//
// Wotold v2 (uikit) alignment block см. PR описание / chat (B18.5c). Использует
// только uikit-токены + классы (.field, .optioncard, .set-table, .panel, .wave,
// .chip, .dot, .btn, .iconbtn). Иконки — <Icon/>.
//
// Логика секции:
//   - Engine picker (Local / Cloud / BYO) — radiogroup из .optioncard-кнопок
//     (role="radio"). Atomic swap через localEngineSetActiveEngine. Меняет
//     следующую запись, не трогает существующие (PRD §M12.6.6).
//   - Hardware probe banner — accent-soft .panel на первом mount'е если
//     recommendation != null и не совпадает с текущим preset'ом.
//   - При выбранном Local — preset picker (Light / Balanced / Quality) на
//     .optioncard со статус-.dot (ok/accent-pulse/faint) для моделей.
//   - «Освободить место» — .set-table со статусом и кнопками удаления.
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
import { getSetting, setSetting, SETTINGS_KEYS } from '../api/settings';
import {
  localEngineGetActiveEngine,
  localEngineGetActivePreset,
  localEngineHwProbe,
  localEngineListCatalog,
  localEngineModelDelete,
  localEngineModelDownload,
  localEngineModelStatus,
  localEngineGetKeepResident,
  localEngineSetActiveEngine,
  localEngineSetActivePreset,
  localEngineSetKeepResident,
  localEngineStorageList,
  type LocalEngineCatalogEntry,
  type LocalEngineStorageRow,
} from '../api/local-engine';
import { getAssistantSemanticSearch, setAssistantSemanticSearch } from '../api/assistant';
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

/** [B25] Пара файлов текст-эмбеддера ассистента (семантический поиск). */
const E5_MODEL_ID = 'e5-small-qint8';
const E5_TOKENIZER_ID = 'e5-small-tokenizer';
const E5_IDS = [E5_MODEL_ID, E5_TOKENIZER_ID] as const;

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
  const [engine, setEngine] = useState<EngineKind | null>(null);
  const [preset, setPreset] = useState<PresetSpec | null>(null);
  // [B2] Тумблер «держать модель активной» (resident llama-server).
  const [keepResident, setKeepResident] = useState(false);
  // [B25] Тумблер «Семантический поиск ассистента» (default on).
  const [semanticSearch, setSemanticSearch] = useState(true);
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
      const [e, p, c, h, rows, resident, semantic] = await Promise.all([
        localEngineGetActiveEngine(),
        localEngineGetActivePreset(),
        localEngineListCatalog(),
        localEngineHwProbe(false),
        localEngineStorageList().catch(() => [] as LocalEngineStorageRow[]),
        localEngineGetKeepResident().catch(() => false),
        // [B25] Тумблер семантического поиска; ошибка → default on.
        getAssistantSemanticSearch().catch(() => true),
      ]);
      setEngine(e);
      setPreset(p);
      setCatalog(c);
      setHw(h);
      setStorageRows(rows);
      setKeepResident(resident);
      setSemanticSearch(semantic);
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
        // pyannote (shared, optional но рекомендованная) + [B25] эмбеддер
        // семантического поиска (если тумблер включён).
        const needed = PRESET_TO_MODELS[next];
        const shared = [PYANNOTE_MODEL_ID, ...(semanticSearch ? E5_IDS : [])];
        for (const id of [needed.whisper, needed.llm, ...shared]) {
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

      {!hwLoading && (
        <div>
          <GroupLabel top={2}>{t('localEngine.engineLabel')}</GroupLabel>
          <div
            role="radiogroup"
            aria-label={t('localEngine.engineLabel')}
            style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
          >
            {/* [B21] Канон: Local первым. */}
            {(['local', 'cloud_managed'] as EngineKind[]).map((k) => (
              <OptionCard
                key={k}
                radio
                active={engine === k}
                icon={k === 'local' ? 'cpu' : 'cloud'}
                title={t(`localEngine.engine.${k}.title`)}
                sub={t(`localEngine.engine.${k}.body`)}
                quality={k === 'local' ? 2 : 3}
                meta={<span className="mono">{t(`localEngine.engine.${k}.quality`)}</span>}
                onClick={() => void onEngineChange(k)}
              />
            ))}
          </div>
        </div>
      )}

      {!hwLoading && engine === 'cloud_managed' && (
        <div>
          <GroupLabel>{t('usage.quotaTitle')}</GroupLabel>
          <UsageSection />
        </div>
      )}

      {/* [B21] Канон :183-187 — sunken-плашка: Icon cpu + mono-спеки + ghost. */}
      {!hwLoading && engine === 'local' && hw && (
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
                  setError(humanError(e));
                }
              })();
            }}
          >
            {t('localEngine.reprobe')}
          </Button>
        </div>
      )}

      {!hwLoading && engine === 'local' && (
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
                  setError(humanError(err));
                }
              }}
            />
          }
        />
      )}

      {/* [B25] Семантический поиск ассистента: тумблер + живой статус
          модели (скачивается %/активен) из общих statuses/progresses. */}
      {!hwLoading && engine === 'local' && (
        <SettingRow
          label={t('localEngine.semanticLabel')}
          hint={t('localEngine.semanticHint')}
          align="top"
          last
          control={
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 10 }}>
              {semanticSearch && (
                <span className="u-muted" style={{ fontSize: 12, whiteSpace: 'nowrap' }}>
                  {(() => {
                    const prog = progresses[E5_MODEL_ID] ?? progresses[E5_TOKENIZER_ID];
                    if (prog) return t('localEngine.semanticDownloading', { pct: prog.pct });
                    const ready = E5_IDS.every((id) => statuses[id]?.state === 'present');
                    return ready
                      ? t('localEngine.semanticActive')
                      : t('localEngine.semanticWaiting');
                  })()}
                </span>
              )}
              <Switch
                checked={semanticSearch}
                label={t('localEngine.semanticLabel')}
                onChange={async (next) => {
                  setSemanticSearch(next); // optimistic
                  try {
                    await setAssistantSemanticSearch(next);
                  } catch (err) {
                    setSemanticSearch(!next); // revert on failure
                    setError(humanError(err));
                  }
                }}
              />
            </span>
          }
        />
      )}

      {!hwLoading && engine === 'local' && (
        <div>
          <GroupLabel>{t('localEngine.presetLabel')}</GroupLabel>
          <div
            role="radiogroup"
            aria-label={t('localEngine.presetLabel')}
            style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
          >
            {PRESETS.map((p, qi) => {
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
              return (
                <OptionCard
                  key={p}
                  radio
                  active={preset?.preset === p}
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

      {!hwLoading && engine === 'local' && storageRows.length > 0 && (
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
