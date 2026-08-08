/**
 * Tauri-only live install/upgrade log stream.
 * Browser / mock builds: subscribe is a no-op.
 */
import { isTauriApp } from '@/lib/platform';
import type { AgentId } from '@/lib/types';

export const INSTALL_PROGRESS_EVENT = 'install-progress';

export interface InstallProgressPayload {
  agentId?: string | null;
  action?: string;
  line: string;
}

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

/** Filter helper: only lines for this agent (or runtime-only when agentId is null). */
export function isProgressForAgent(
  payload: InstallProgressPayload,
  agentId: AgentId,
): boolean {
  if (!payload.agentId) return false;
  return payload.agentId === agentId;
}
