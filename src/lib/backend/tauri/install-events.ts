/**
 * Tauri-only live install/upgrade log stream.
 * Only Tauri ports may import this module.
 */
import type { InstallProgressPayload } from '@/lib/backend/contracts/install-types';
import { isTauriApp } from '@/lib/platform';

export const INSTALL_PROGRESS_EVENT = 'install-progress';

export type { InstallProgressPayload };

/**
 * Subscribe to install progress lines emitted while install/upgrade/uninstall runs.
 * Returns an unsubscribe function.
 */
export async function onInstallProgress(
  handler: (payload: InstallProgressPayload) => void,
): Promise<() => void> {
  if (!isTauriApp()) {
    return () => {};
  }
  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<InstallProgressPayload>(INSTALL_PROGRESS_EVENT, (event) => {
      if (event.payload?.line) {
        handler(event.payload);
      }
    });
    return unlisten;
  } catch {
    return () => {};
  }
}
