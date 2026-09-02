import type { InstallProgressPayload } from '@/lib/backend/contracts/install-types';
import { installProgressChunk } from '@/lib/backend/contracts/install-types';
import { unavailableError } from '@/lib/backend/contracts/errors';
import { isTauriApp } from '@/lib/platform';

export const INSTALL_PROGRESS_EVENT = 'install-progress';

export type { InstallProgressPayload };

/**
 * Subscribe to install progress chunks emitted while install/upgrade/uninstall runs.
 * Returns an unsubscribe function.
 */
export async function onInstallProgress(
  handler: (payload: InstallProgressPayload) => void,
): Promise<() => void> {
  if (!isTauriApp()) {
    throw unavailableError(
      '安装进度订阅',
      '当前不是 Tauri 桌面运行时；请使用桌面应用，或开发时注入 mock backend',
    );
  }
  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<InstallProgressPayload>(INSTALL_PROGRESS_EVENT, (event) => {
      const payload = event.payload;
      if (!payload || typeof installProgressChunk(payload) !== 'string') return;
      // Normalize so consumers can always read `chunk`.
      handler({
        ...payload,
        chunk: installProgressChunk(payload),
      });
    });
    return unlisten;
  } catch (error) {
    throw unavailableError(
      '安装进度订阅',
      error instanceof Error ? error.message : String(error),
    );
  }
}
