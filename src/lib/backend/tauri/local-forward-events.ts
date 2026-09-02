/**
 * Tauri-only local-forwarding lifecycle events.
 * Only Tauri ports (and app-shell chrome) may import this module.
 */
import { unavailableError } from '@/lib/backend/contracts/errors';
import { isTauriApp } from '@/lib/platform';

export const LOCAL_FORWARD_LIFECYCLE_EVENT = 'local-forward-lifecycle';

export type LocalForwardLifecyclePhase = 'restarting' | 'ready';

export type LocalForwardLifecyclePayload = {
  phase: LocalForwardLifecyclePhase;
};

export function localForwardLifecyclePhase(
  payload: { phase?: unknown } | undefined,
): LocalForwardLifecyclePhase | null {
  if (payload?.phase === 'restarting' || payload?.phase === 'ready') {
    return payload.phase;
  }
  return null;
}

/**
 * Subscribe to restore/start lifecycle from the desktop host.
 * Returns an unsubscribe function.
 */
export async function onLocalForwardLifecycle(
  handler: (payload: LocalForwardLifecyclePayload) => void,
): Promise<() => void> {
  if (!isTauriApp()) {
    throw unavailableError(
      '本机转发状态订阅',
      '当前不是 Tauri 桌面运行时；请使用桌面应用，或开发时注入 mock backend',
    );
  }
  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<{ phase?: unknown }>(LOCAL_FORWARD_LIFECYCLE_EVENT, (event) => {
      const phase = localForwardLifecyclePhase(event.payload);
      if (phase !== null) {
        handler({ phase });
      }
    });
    return unlisten;
  } catch (error) {
    throw unavailableError(
      '本机转发状态订阅',
      error instanceof Error ? error.message : String(error),
    );
  }
}
