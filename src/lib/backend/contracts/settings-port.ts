import type { AppSettings, LogLevel } from '@/lib/types';

export interface SettingsPort {
  getSettings(): Promise<AppSettings>;
  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings>;
  openLogsDir(): Promise<string>;
  /** Open http(s) URL in the system browser (Tauri cannot rely on window.open). */
  openExternalUrl(url: string): Promise<void>;
  /** Native folder picker. `null` = cancelled. Value is a filesystem path, not a URI. */
  pickDirectory(options?: {
    title?: string;
    defaultPath?: string | null;
  }): Promise<string | null>;
  /**
   * Best-effort GUI log line. `last4` only — never a raw key.
   * Mock is a no-op.
   */
  logGuiEvent?(
    op: string,
    detail?: { agent?: string; last4?: string },
  ): Promise<void>;
  logLevelOptions: { value: LogLevel; label: string }[];
}
