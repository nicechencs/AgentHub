/**
 * App self-update API façade.
 */
import { getBackend } from '@/app/runtime';
import type { UpdateDownloadProgress, UpdateInfo } from '@/lib/backend/contracts/update-types';

export type { UpdateDownloadProgress, UpdateInfo };

export async function isUpdateAvailable(): Promise<boolean> {
  return getBackend().update.isAvailable();
}

export async function getAppVersion(): Promise<string> {
  return getBackend().update.getAppVersion();
}

/** Returns update info when a newer version exists; otherwise `null`. */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  return getBackend().update.checkForUpdate();
}

/** Download, install, and relaunch (desktop). */
export async function downloadAndInstallUpdate(
  onProgress?: (progress: UpdateDownloadProgress) => void,
): Promise<void> {
  return getBackend().update.downloadAndInstall(onProgress);
}
