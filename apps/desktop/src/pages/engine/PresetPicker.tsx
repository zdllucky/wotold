// Настройки → Обработка: выбор размера движка.
//
// [design-gate] Surface: pages/engine/PresetPicker
// Reference: docs/design/wotold-v2/_reference/wk-settings.jsx (SecEngine presets)
// Tokens: наследуются .optioncard (--accent, --border, --r-md, --t-*)
// Classes: .optioncard (через OptionCard), .mono, GroupLabel
// Logic preserved: roving tabindex радиогруппы, бейдж рекомендации, quality-точки,
//   подтверждение Quality на слабом железе — в родителе.
// A11y: role="radiogroup" + aria-label, табостановка на выбранном (или первом,
//   если размер ещё не выбран — иначе в группу не войти с клавиатуры).
//
// Размеры и раскладка моделей приходят с бэкенда (`local_engine_preset_specs`):
// копия раскладки здесь молча расходилась с Rust, а захардкоженные
// «1.2 / 2.4 / 5.5 ГБ» занижали все три размера — база в них не входила.

import type { LocalEnginePreset, ModelStatus, PresetSizeSpec, PresetSpec } from '@wotold/contracts';

import { GroupLabel, OptionCard } from '../../ui';
import { useI18n } from '../../i18n';
import { formatBytes } from './formatBytes';

const PRESETS: LocalEnginePreset[] = ['light', 'balanced', 'quality'];

interface PresetPickerProps {
  preset: PresetSpec | null;
  specs: PresetSizeSpec[];
  statuses: Record<string, ModelStatus>;
  /** id моделей, которые качаются прямо сейчас. */
  downloadingIds: Set<string>;
  /** Идёт докачка — размер менять нельзя. */
  busy: boolean;
  recommendation: LocalEnginePreset | null;
  onPick: (preset: LocalEnginePreset) => void;
}

export function PresetPicker({
  preset,
  specs,
  statuses,
  downloadingIds,
  busy,
  recommendation,
  onPick,
}: PresetPickerProps) {
  const { t } = useI18n();

  return (
    <div>
      <GroupLabel>{t('localEngine.presetLabel')}</GroupLabel>
      <div
        role="radiogroup"
        aria-label={t('localEngine.presetLabel')}
        style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
      >
        {PRESETS.map((p, qi) => {
          const spec = specs.find((s) => s.preset === p);
          const ids = spec ? [spec.whisper_model_id, spec.llm_model_id] : [];
          const allPresent =
            ids.length > 0 && ids.every((id) => statuses[id]?.state === 'present');
          const anyDownloading = ids.some((id) => downloadingIds.has(id));
          return (
            <OptionCard
              key={p}
              radio
              active={preset?.preset === p}
              tabStop={preset ? preset.preset === p : qi === 0}
              title={t(`localEngine.preset.${p}`)}
              badge={recommendation === p ? t('localEngine.presetRecommend') : undefined}
              quality={qi + 1}
              meta={
                <span className="mono">
                  {t(`localEngine.presetMeta.${p}`)}
                  {' · '}
                  {spec ? formatBytes(spec.total_bytes) : '—'}
                  {' · '}
                  {allPresent
                    ? t('localEngine.statusInstalled')
                    : anyDownloading
                      ? t('localEngine.statusDownloading')
                      : t('localEngine.statusAbsent')}
                </span>
              }
              // Пока идёт докачка, сменить размер нельзя: очередь на бэкенде
              // одна, и второй запрос дожидался бы первого — со стороны это
              // выглядело бы как «нажал, ничего не произошло».
              disabled={busy}
              onClick={() => onPick(p)}
            />
          );
        })}
      </div>
      {busy && <p className="set-hint">{t('localEngine.presetLockedWhileDownloading')}</p>}
    </div>
  );
}
