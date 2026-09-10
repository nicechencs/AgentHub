/**
 * Tauri-only: OS/menu accelerator for Chat new-chat (Ctrl/Cmd+N).
 * Pages must use the chat API façade, not this module.
 */
import { isTauriApp } from '@/lib/platform';
import { unavailableError } from '@/lib/backend/contracts/errors';

export const CHAT_SHORTCUT_EVENT = 'chat-shortcut';

export type ChatNativeShortcutAction = 'newChat';

export function chatNativeShortcutFromPayload(
  payload: { action?: unknown } | undefined,
): ChatNativeShortcutAction | null {
  return payload?.action === 'newChat' ? 'newChat' : null;
}

export async function onChatNativeShortcut(
  handler: (action: ChatNativeShortcutAction) => void,
): Promise<() => void> {
  if (!isTauriApp()) {
    throw unavailableError(
      '对话快捷键',
      '当前不是 Tauri 桌面运行时；请使用桌面应用，或开发时注入 mock backend',
    );
  }
  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen(CHAT_SHORTCUT_EVENT, (event) => {
      const action = chatNativeShortcutFromPayload(event.payload as { action?: unknown });
      if (action) handler(action);
    });
    return unlisten;
  } catch (error) {
    throw unavailableError(
      '对话快捷键',
      error instanceof Error ? error.message : String(error),
    );
  }
}
