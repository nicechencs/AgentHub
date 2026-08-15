import type { AppSettings, LogLevel } from '@/lib/types';

export interface SettingsPort {
  getSettings(): Promise<AppSettings>;
  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings>;
  openLogsDir(): Promise<string>;
  /** Open http(s) URL in the system browser (Tauri cannot rely on window.open). */
  openExternalUrl(url: string): Promise<void>;
  logLevelOptions: { value: LogLevel; label: string }[];
}
