// Настройки → Обработка. Железо, размер движка, резидентный режим, место.
//
// [design-gate] Surface: pages/engine/EngineSection (композиция)
// Reference: docs/design/wotold-v2/_reference/wk-settings.jsx (SecEngine)
// Tokens: --danger (только текст ошибки), остальное — внутри дочерних
// Classes: .setting-row (через SettingRow), .switch (через Switch)
// New tokens: нет
// Logic preserved: probe на mount, подтверждение Quality на <16 ГБ RAM,
//   оптимистичный тумблер резидентного режима с откатом при ошибке.
//
// Что ушло из секции: таблица моделей (стала одной строкой + кнопкой),
// тумблер семантического поиска и per-model кнопки скачивания. Список
// обязательного знает бэкенд, а качает его баннер готовности — здесь незачем
// вторая точка входа в те же гигабайты.

import { useCallback, useEffect, useState } from 'react';
import { ask } from '@tauri-apps/plugin-dialog';
import type { HwReport, LocalEnginePreset, ModelStatus, PresetSizeSpec, PresetSpec } from '@wotold/contracts';

import {
  localEngineFreeSpace,
  localEngineGetActivePreset,
  localEngineGetKeepResident,
  localEngineHwProbe,
  localEngineModelStatus,
  localEnginePresetSpecs,
  localEngineReclaimableBytes,
  localEngineSetActivePreset,
  localEngineSetKeepResident,
  localEngineStorageList,
} from '../../api/local-engine';
import { humanError } from '../../api/errors';
import { useI18n } from '../../i18n';
import { SettingRow, Switch } from '../../ui';
import { useReadiness } from '../../components/readiness/ReadinessProvider';
import { HwProbeStrip } from './HwProbeStrip';
import { PresetPicker } from './PresetPicker';
import { StorageLine } from './StorageLine';

/** RAM, ниже которой Quality требует подтверждения (PRD §M12.5.4). */
const QUALITY_MIN_RAM_GB = 16;

export function EngineSection() {
  const { t } = useI18n();
  const { readiness, downloading, downloadingIds, ensure } = useReadiness();
  const [preset, setPreset] = useState<PresetSpec | null>(null);
  const [specs, setSpecs] = useState<PresetSizeSpec[]>([]);
  const [statuses, setStatuses] = useState<Record<string, ModelStatus>>({});
  const [hw, setHw] = useState<HwReport | null>(null);
  const [keepResident, setKeepResident] = useState(false);
  const [usedBytes, setUsedBytes] = useState(0);
  const [reclaimable, setReclaimable] = useState(0);
  const [loading, setLoading] = useState(true);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshStorage = useCallback(async () => {
    try {
      const [rows, free] = await Promise.all([
        localEngineStorageList(),
        localEngineReclaimableBytes(),
      ]);
      setUsedBytes(
        rows.reduce((sum, r) => (r.status.state === 'present' ? sum + r.size_bytes : sum), 0),
      );
      setReclaimable(free);
    } catch (e) {
      setError(humanError(e, t));
    }
  }, [t]);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    try {
      const [p, h, resident, sizeSpecs] = await Promise.all([
        localEngineGetActivePreset(),
        localEngineHwProbe(false),
        localEngineGetKeepResident().catch(() => false),
        localEnginePresetSpecs(),
      ]);
      setPreset(p);
      setHw(h);
      setKeepResident(resident);
      setSpecs(sizeSpecs);
      // Статусы нужны по моделям всех размеров, а не только выбранного: снимок
      // готовности знает лишь про обязательные, поэтому «Balanced уже скачан»
      // из него не выведешь.
      const ids = sizeSpecs.flatMap((s) => [s.whisper_model_id, s.llm_model_id]);
      const entries = await Promise.all(
        ids.map(async (id) => [id, await localEngineModelStatus(id)] as const),
      );
      setStatuses(Object.fromEntries(entries));
      setError(null);
    } catch (e) {
      setError(humanError(e, t));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void refreshAll();
    void refreshStorage();
  }, [refreshAll, refreshStorage]);

  // Снимок готовности меняется на каждую докачку — перечитываем статусы и место
  // тем же поводом, вместо собственной подписки на события моделей.
  useEffect(() => {
    if (!readiness) return;
    void refreshStorage();
  }, [readiness, refreshStorage]);

  const onPick = useCallback(
    async (next: LocalEnginePreset) => {
      // PRD §M12.5.4: Quality на железе <16 ГБ RAM → подтверждение.
      if (next === 'quality' && hw && hw.ram_gb < QUALITY_MIN_RAM_GB) {
        const ok = await ask(t('localEngine.qualityConfirmMsg'), {
          title: t('localEngine.qualityConfirmTitle'),
          kind: 'warning',
        });
        if (!ok) return;
      }
      try {
        setPreset(await localEngineSetActivePreset(next));
        // Докачку недостающего ведёт баннер готовности: одна очередь, один
        // прогресс. Здесь только просим её начать.
        ensure();
        await refreshAll();
      } catch (e) {
        setError(humanError(e, t));
      }
    },
    [ensure, hw, refreshAll, t],
  );

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      {error && (
        <p role="alert" style={{ color: 'var(--danger)', margin: 0 }}>
          {error}
        </p>
      )}

      <HwProbeStrip
        hw={hw}
        loading={loading}
        preset={preset}
        bannerDismissed={bannerDismissed}
        onDismissBanner={() => setBannerDismissed(true)}
        onApplyRecommendation={(p) => void onPick(p)}
        onReprobe={() => {
          void (async () => {
            try {
              setHw(await localEngineHwProbe(true));
            } catch (e) {
              setError(humanError(e, t));
            }
          })();
        }}
      />

      {!loading && (
        <SettingRow
          settingId="keep-resident"
          label={t('localEngine.keepResidentLabel')}
          hint={t('localEngine.keepResidentHint')}
          align="top"
          last
          control={
            <Switch
              checked={keepResident}
              label={t('localEngine.keepResidentLabel')}
              onChange={async (next) => {
                setKeepResident(next); // оптимистично
                try {
                  await localEngineSetKeepResident(next);
                } catch (err) {
                  setKeepResident(!next); // откат
                  setError(humanError(err, t));
                }
              }}
            />
          }
        />
      )}

      {!loading && (
        <PresetPicker
          preset={preset}
          specs={specs}
          statuses={statuses}
          downloadingIds={downloadingIds}
          busy={downloading}
          recommendation={hw?.recommendation ?? null}
          onPick={(p) => void onPick(p)}
        />
      )}

      {!loading && (
        <StorageLine
          usedBytes={usedBytes}
          reclaimableBytes={reclaimable}
          onFreeSpace={async () => {
            try {
              const freed = await localEngineFreeSpace();
              await refreshAll();
              await refreshStorage();
              return freed;
            } catch (e) {
              setError(humanError(e, t));
              return 0;
            }
          }}
        />
      )}
    </div>
  );
}
