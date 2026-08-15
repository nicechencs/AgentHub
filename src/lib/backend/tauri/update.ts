/**
 * App self-update via tauri-plugin-updater + process relaunch.
 * Only this module may talk to the updater / process plugins.
 */
import { getVersion } from '@tauri-apps/api/app';
import { check, type Update } from '@tauri-apps/plugin-updater';
import type { UpdateDownloadProgress, UpdateInfo, UpdatePort } from '@/lib/backend/contracts/update-types';
import { assertTauriRuntime, invoke } from './invoke';
import { logger } from '@/lib/logger';

const log = logger.scope('backend:tauri:update');

/** Keep the last successful check so install can reuse the signed resource. */
let pendingUpdate: Update | null = null;

function mapUpdate(update: Update): UpdateInfo {
  return {
    version: update.version,
    currentVersion: update.currentVersion,
    notes: update.body ?? null,
    date: update.date ?? null,
  };
}

function mapProgress(
  downloaded: number,
  total: number | null,
): UpdateDownloadProgress {
  const percent =
    total != null && total > 0
      ? Math.min(100, Math.round((downloaded / total) * 100))
      : null;
  return { downloaded, total, percent };
}

async function ensurePendingUpdate(): Promise<Update> {
  if (pendingUpdate) return pendingUpdate;
  const update = await check({ timeout: 30_000 });
  if (!update) {
    throw new Error('当前已是最新版本');
  }
  pendingUpdate = update;
  return update;
}

export function createTauriUpdatePort(): UpdatePort {
  return {
    async isAvailable() {
      try {
        assertTauriRuntime('update');
        return true;
      } catch {
        return false;
      }
    },

    async getAppVersion() {
      assertTauriRuntime('getAppVersion');
      try {
        return await getVersion();
      } catch (e) {
        log.error('getAppVersion failed', e);
        throw e;
      }
    },

    async checkForUpdate() {
      assertTauriRuntime('checkForUpdate');
      try {
        // Drop previous handle so we do not leak updater resources across checks.
        if (pendingUpdate) {
          try {
            await pendingUpdate.close();
          } catch {
            // ignore close errors
          }
          pendingUpdate = null;
        }
        const update = await check({ timeout: 30_000 });
        if (!update) {
          return null;
        }
        pendingUpdate = update;
        return mapUpdate(update);
      } catch (e) {
        log.error('checkForUpdate failed', e);
        throw e;
      }
    },

    async downloadAndInstall(onProgress) {
      assertTauriRuntime('downloadAndInstall');
      try {
        const update = await ensurePendingUpdate();
        let downloaded = 0;
        let total: number | null = null;

        await update.downloadAndInstall((event) => {
          if (event.event === 'Started') {
            total =
              typeof event.data.contentLength === 'number'
                ? event.data.contentLength
                : null;
            downloaded = 0;
            onProgress?.(mapProgress(downloaded, total));
          } else if (event.event === 'Progress') {
            downloaded += event.data.chunkLength;
            onProgress?.(mapProgress(downloaded, total));
          } else if (event.event === 'Finished') {
            if (total != null) downloaded = total;
            onProgress?.(mapProgress(downloaded, total ?? downloaded));
          }
        });

        pendingUpdate = null;
        // Raw process-plugin relaunch would bypass active bridge impact
        // confirmation. The Rust lifecycle command drains/asks first.
        await invoke('request_controlled_restart');
      } catch (e) {
        log.error('downloadAndInstall failed', e);
        throw e;
      }
    },
  };
}
