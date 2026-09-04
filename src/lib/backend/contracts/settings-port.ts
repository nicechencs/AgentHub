import type { AppSettings, LogLevel } from '@/lib/types';

export interface SettingsPort {
  getSettings(): Promise<AppSettings>;
  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings>;
  openLogsDir(): Promise<string>;
  /** Open http(s) URL in the system browser (Tauri cannot rely on window.open). */
  openExternalUrl(url: string): Promise<void>;
  /**
   * Open Sub2API `{base}/login` in a child webview and return session tokens
   * when localStorage is readable. Rejects with `cancelled` when the user closes
   * the window. Mock / browser builds should reject so the GUI uses paste fallback.
   */
  openSub2ApiLoginWindow(loginUrl: string): Promise<{
    accessToken: string;
    refreshToken?: string;
    expiresAt?: number;
  }>;
  /** Native folder picker. `null` = cancelled. Value is a filesystem path, not a URI. */
  pickDirectory(options?: {
    title?: string;
    defaultPath?: string | null;
  }): Promise<string | null>;
  /**
   * Best-effort GUI log line. `last4` only — never a raw key.
   * Optional `profileId` / `route` / `code` help correlate Routes actions.
   * Mock is a no-op.
   */
  logGuiEvent?(
    op: string,
    detail?: {
      agent?: string;
      last4?: string;
      profileId?: string;
      route?: string;
      code?: string;
    },
  ): Promise<void>;
  logLevelOptions: { value: LogLevel; label: string }[];
}
