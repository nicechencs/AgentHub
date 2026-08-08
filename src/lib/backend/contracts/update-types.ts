/** Result of a successful update check (newer version available). */
export interface UpdateInfo {
  /** SemVer of the available release (may include leading `v` from server). */
  version: string;
  /** Currently running app version. */
  currentVersion: string;
  /** Release notes / changelog body when provided by the update feed. */
  notes?: string | null;
  /** Release publication time (RFC 3339) when provided. */
  date?: string | null;
}

/** Download progress for one-click install. */
export interface UpdateDownloadProgress {
  /** Bytes received so far. */
  downloaded: number;
  /** Total bytes when known (`Content-Length`); null when chunked / unknown. */
  total: number | null;
  /** 0–100 when total is known; otherwise null. */
  percent: number | null;
}

export interface UpdatePort {
  /** Whether auto-update is supported in this runtime (Tauri desktop). */
  isAvailable(): Promise<boolean>;
  /** Package version from the running shell (`tauri.conf` / Cargo). */
  getAppVersion(): Promise<string>;
  /**
   * Check the configured update endpoint.
   * Returns update info when a newer version is available; `null` when up to date.
   */
  checkForUpdate(): Promise<UpdateInfo | null>;
  /**
   * Download and install the last checked update (or re-check if none cached),
   * then relaunch the app. Rejects when no update is available.
   */
  downloadAndInstall(onProgress?: (progress: UpdateDownloadProgress) => void): Promise<void>;
}
