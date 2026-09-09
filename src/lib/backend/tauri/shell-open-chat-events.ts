/**
 * Tauri-only: OS file-manager "open chat" events and the cold-start pending path.
 * Only Tauri ports (and App.tsx) may import this module.
 */
import { isTauriApp } from '@/lib/platform';
import { unavailableError } from '@/lib/backend/contracts/errors';
import { invoke } from './invoke';

export const OPEN_CHAT_CWD_EVENT = 'open-chat-cwd';

export function openChatCwdFromPayload(
  payload: { cwd?: unknown } | undefined,
): string | null {
  const cwd = payload?.cwd;
  if (typeof cwd !== 'string') return null;
  const trimmed = cwd.trim();
  return trimmed ? trimmed : null;
}

export async function takePendingOpenChatCwd(): Promise<string | null> {
  if (!isTauriApp()) {
    throw unavailableError(
      '系统右键打开对话',
      '当前不是 Tauri 桌面运行时；请使用桌面应用，或开发时注入 mock backend',
    );
  }
  const cwd = await invoke<string | null>('take_pending_open_chat_cwd');
  return openChatCwdFromPayload({ cwd: cwd ?? undefined });
}

/** Event is a wake-up only; the folder always comes from takePending. */
export async function onOpenChatCwd(handler: () => void): Promise<() => void> {
  if (!isTauriApp()) {
    throw unavailableError(
      '系统右键打开对话',
      '当前不是 Tauri 桌面运行时；请使用桌面应用，或开发时注入 mock backend',
    );
  }
  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen(OPEN_CHAT_CWD_EVENT, () => {
      handler();
    });
    return unlisten;
  } catch (error) {
    throw unavailableError(
      '系统右键打开对话',
      error instanceof Error ? error.message : String(error),
    );
  }
}

/**
 * Wake the GUI when a folder handoff may be waiting: event, window focus, or
 * the page becoming visible again after hide-to-tray.
 */
export async function subscribeOpenChatCwdWakeups(
  handler: () => void,
): Promise<() => void> {
  const unsubs: Array<() => void> = [await onOpenChatCwd(handler)];

  if (typeof document !== 'undefined') {
    const onVisible = () => {
      if (document.visibilityState === 'visible') handler();
    };
    document.addEventListener('visibilitychange', onVisible);
    unsubs.push(() => document.removeEventListener('visibilitychange', onVisible));
  }

  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const unlistenFocus = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) handler();
    });
    unsubs.push(unlistenFocus);
  } catch {
    // Focus events are extra wake-ups; the folder event still works.
  }

  return () => {
    for (const unsub of unsubs) unsub();
  };
}
