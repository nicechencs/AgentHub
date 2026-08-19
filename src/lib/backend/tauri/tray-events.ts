/**
 * Tauri-only tray navigation events.
 * Only Tauri ports (and App.tsx) may import this module.
 */
import { isTauriApp } from '@/lib/platform';

export const TRAY_NAVIGATE_EVENT = 'tray-navigate';

export function trayNavigatePath(
  payload: { path?: unknown } | undefined,
): string | null {
  const path = payload?.path;
  if (typeof path === 'string' && path.startsWith('/')) {
    return path;
  }
  return null;
}

/**
 * Subscribe to tray "open routes" navigation from the Rust menu.
 * Returns an unsubscribe function.
 */
export async function onTrayNavigate(
  handler: (path: string) => void,
): Promise<() => void> {
  if (!isTauriApp()) {
    return () => {};
  }
  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<{ path?: unknown }>(TRAY_NAVIGATE_EVENT, (event) => {
      const path = trayNavigatePath(event.payload);
      if (path !== null) {
        handler(path);
      }
    });
    return unlisten;
  } catch {
    return () => {};
  }
}
