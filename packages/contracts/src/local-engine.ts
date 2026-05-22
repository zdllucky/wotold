// Local Engine contracts — M12 PRD §7.
//
// Третий путь обработки (см. [M12_LOCAL_ENGINE_PRD.md](../../../docs/M12_LOCAL_ENGINE_PRD.md)):
// `local` параллельно `cloud_managed` и `cloud_byo`, всё на устройстве.
//
// `DiarizedTranscript` / `RecapJson` / `CallProgress` остаются без изменений —
// local engine produces совместимые структуры (PRD §7).

/** Активный preset локального движка. См. PRD §2.5. */
export type LocalEnginePreset = 'light' | 'balanced' | 'quality';

/** Какой движок обрабатывает звонок. См. PRD §2.1. */
export type EngineKind = 'local' | 'cloud_managed' | 'cloud_byo';

/** Тип модели в каталоге. */
export type LocalModelKind = 'stt' | 'llm';

/**
 * Запись в hardcoded MODEL_CATALOG (PRD §M12.4.1).
 *
 * # Naming convention
 *
 * M12 контракты используют snake_case в JSON чтобы 1:1 match'ить Rust
 * `#[derive(Serialize)]` output (см. `apps/desktop/src-tauri/src/local_engine/`).
 * Other contracts (transcript, recap) исторически camelCase с явным rename
 * на Rust стороне — M12 отказывается от двойного renaming чтобы упростить.
 */
export interface ModelEntry {
  /** Стабильный id: `whisper-small` | `whisper-medium` | `whisper-large-v3` | `gemma3-2b` | `qwen25-3b` | `qwen25-7b`. */
  id: string;
  kind: LocalModelKind;
  display_name: string;
  /** Прямой HTTPS-URL (HuggingFace / собственный R2). */
  url: string;
  /** Lowercase hex SHA256 — единственная защита от подмены (M12.4.6). */
  sha256: string;
  size_bytes: number;
  /** Ссылка на лицензию модели (Apache 2.0 / Gemma TOS / MIT etc). */
  license_url: string;
}

/**
 * Tagged-union по `state` (Rust `#[serde(tag = "state")]`).
 * Absent — файл отсутствует. Present — установлен + SHA verified.
 * Corrupted — есть файл но SHA mismatch (поломка / placeholder pre-flight).
 *
 * # `downloading` НЕ в этом union — он не существует как persistent state.
 * Прогресс закачки модели приходит через отдельный канал `model:progress`
 * (см. {@link ModelProgressEvent}). По завершению — `model:done` event,
 * затем `state` снова из `ModelStatus`. UI должен трекать прогресс отдельно
 * от status snapshot'а.
 */
export type ModelStatus =
  | { state: 'absent'; id: string; bytes_total: number }
  | { state: 'present'; id: string; bytes_total: number }
  | {
      state: 'corrupted';
      id: string;
      bytes_done: number;
      bytes_total: number;
      expected: string;
      got: string;
    };

/** Привязка preset → конкретные id в каталоге. См. PRD §2.5. */
export interface PresetSpec {
  preset: LocalEnginePreset;
  whisper_model_id: string;
  llm_model_id: string;
}

/** Результат hardware probe. См. PRD §M12.7. */
export interface HwReport {
  os: 'macos' | 'windows' | 'linux';
  arch: 'arm64' | 'x86_64';
  /** Например `Apple M2 Pro` / `Intel Core i7-9750H`. */
  cpu_model: string;
  ram_gb: number;
  metal_supported: boolean;
  /**
   * Рекомендованный preset. `null` на платформах где локальный движок
   * не работает (R9 — Linux/Windows).
   */
  recommendation: LocalEnginePreset | null;
}

/**
 * Payload события `model:progress` — отдельный канал от `call:progress`.
 * См. PRD §M12.4.2.
 */
export interface ModelProgressEvent {
  id: string;
  pct: number;
  bytes_done: number;
  bytes_total: number;
}

/** Pipeline-step расширение для local-engine. См. PRD §M12.6.3. */
export type LocalPipelineStep = 'upload' | 'stt' | 'speakers' | 'merge' | 'recap';
