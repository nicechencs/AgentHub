import type { UpdateDownloadProgress, UpdateInfo, UpdatePort } from '@/lib/backend/contracts/update-types';
import { delay } from '@/dev/mocks/delay';

const MOCK_VERSION = '0.1.0';

/**
 * Browser mock update port.
 * Default: already up to date. Tests may call `__setMockAvailableUpdate`.
 */
let forcedUpdate: UpdateInfo | null = null;

export function __setMockAvailableUpdate(info: UpdateInfo | null): void {
  forcedUpdate = info;
}

export function createMockUpdatePort(): UpdatePort {
  return {
    async isAvailable() {
      return true;
    },

    async getAppVersion() {
      await delay(20);
      return MOCK_VERSION;
    },

    async checkForUpdate() {
      await delay(120);
      if (!forcedUpdate) return null;
      return { ...forcedUpdate, currentVersion: MOCK_VERSION };
    },

    async downloadAndInstall(onProgress) {
      await delay(80);
      if (!forcedUpdate) {
        throw new Error('当前已是最新版本');
      }
      const total = 1_000_000;
      let downloaded = 0;
      const steps = 5;
      for (let i = 1; i <= steps; i++) {
        await delay(40);
        downloaded = Math.round((total * i) / steps);
        const progress: UpdateDownloadProgress = {
          downloaded,
          total,
          percent: Math.round((downloaded / total) * 100),
        };
        onProgress?.(progress);
      }
      // Mock does not relaunch the browser; clear forced update as "installed".
      forcedUpdate = null;
    },
  };
}
