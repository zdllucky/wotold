// [M12] Frontend API client для local-engine Tauri commands.
//
// Контракт: packages/contracts/src/local-engine.ts.

import { invoke } from '@tauri-apps/api/core';
import type {
  HwReport,
  LocalEnginePreset,
  LocalEngineReadiness,
  LocalModelKind,
  ModelStatus,
  PresetSpec,
} from '@wotold/contracts';

/** Запись каталога (без url/sha256 — те приватны для backend). */
export interface LocalEngineCatalogEntry {
  id: string;
  kind: LocalModelKind;
  display_name: string;
  size_bytes: number;
  license_url: string;
}

export function localEngineListCatalog(): Promise<LocalEngineCatalogEntry[]> {
  return invoke<LocalEngineCatalogEntry[]>('local_engine_list_catalog');
}

export function localEngineModelStatus(id: string): Promise<ModelStatus> {
  return invoke<ModelStatus>('local_engine_model_status', { id });
}

/**
 * Готовность движка: выбран ли размер и каких обязательных модулей не хватает.
 * Дальше состояние живёт на событии `readiness:changed` — см.
 * `components/readiness/ReadinessProvider.tsx`.
 */
export function localEngineReadiness(): Promise<LocalEngineReadiness> {
  return invoke<LocalEngineReadiness>('local_engine_readiness');
}

export function localEngineModelDownload(id: string): Promise<void> {
  return invoke<void>('local_engine_model_download', { id });
}

export function localEngineModelDelete(id: string): Promise<void> {
  return invoke<void>('local_engine_model_delete', { id });
}

export function localEngineGetActivePreset(): Promise<PresetSpec | null> {
  return invoke<PresetSpec | null>('local_engine_get_active_preset');
}

export function localEngineSetActivePreset(preset: LocalEnginePreset): Promise<PresetSpec> {
  return invoke<PresetSpec>('local_engine_set_active_preset', { preset });
}

export function localEngineHwProbe(force = false): Promise<HwReport> {
  return invoke<HwReport>('local_engine_hw_probe', { force });
}

/** [M12.4.4-bis] Сводная таблица для Settings → Storage UI. */
export interface LocalEngineStorageRow {
  id: string;
  kind: 'stt' | 'llm' | 'diarization';
  display_name: string;
  size_bytes: number;
  status: ModelStatus;
  last_used_at: string | null;
  is_active: boolean;
}

export function localEngineStorageList(): Promise<LocalEngineStorageRow[]> {
  return invoke<LocalEngineStorageRow[]>('local_engine_storage_list');
}

/** [B2] Держать ли локальную модель резидентно в RAM (persistent llama-server). */
export function localEngineGetKeepResident(): Promise<boolean> {
  return invoke<boolean>('local_engine_get_keep_resident');
}

/** [B2] Переключить резидентный режим. Пишет настройку + сразу поднимает/гасит сервер. */
export function localEngineSetKeepResident(enabled: boolean): Promise<void> {
  return invoke<void>('local_engine_set_keep_resident', { enabled });
}

/** [recap-rich] G-Eval dev-харнесс: оценка recap.md звонка по 4 осям. */
export interface RecapEvalScores {
  coherence: number;
  faithfulness: number;
  relevance: number;
  conciseness: number;
  average: number;
  justification: string;
}
export function localEngineEvalRecap(callId: string): Promise<RecapEvalScores> {
  return invoke<RecapEvalScores>('local_engine_eval_recap', { callId });
}
