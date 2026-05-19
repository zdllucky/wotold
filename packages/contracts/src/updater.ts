// Tauri Updater manifest (latest.json). См. M11.2 паспорта.
// Tauri требует именно snake_case в JSON-полях, поэтому интерфейс
// зеркалит формат.

export type UpdaterPlatform =
  | 'darwin-aarch64'
  | 'darwin-x86_64'
  | 'linux-x86_64'
  | 'windows-x86_64';

export interface UpdaterPlatformEntry {
  /** Tauri minisign signature of the artifact. M11.1. */
  signature: string;
  /** Direct download URL for the artifact. */
  url: string;
}

export interface TauriUpdaterManifest {
  version: string;
  /** Release notes, plain text or markdown. */
  notes: string;
  /** ISO 8601 publication timestamp. */
  pub_date: string;
  platforms: Partial<Record<UpdaterPlatform, UpdaterPlatformEntry>>;
  /** SCAFFOLD M11.7 — reserved for future channel routing. */
  channel?: 'stable' | 'beta';
}
