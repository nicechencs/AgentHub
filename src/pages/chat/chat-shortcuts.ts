import type { MessageKey } from '@/lib/i18n';
import { detectHostPlatform, type HostPlatform } from '@/lib/platform-detect';

export type ChatShortcutRow = {
  id: string;
  keys: string;
  actionKey: MessageKey;
};

/** Chat shortcut overview. Chords use Ctrl; `chatShortcutChord` shows Cmd on macOS. */
export const CHAT_SHORTCUT_ROWS: readonly ChatShortcutRow[] = [
  { id: 'send', keys: 'Enter', actionKey: 'chat.shortcuts.send' },
  { id: 'newline', keys: 'Shift+Enter', actionKey: 'chat.shortcuts.newline' },
  { id: 'stop', keys: 'Esc', actionKey: 'chat.shortcuts.stop' },
  { id: 'actions', keys: '/', actionKey: 'chat.shortcuts.actions' },
  { id: 'history', keys: 'Ctrl+K', actionKey: 'chat.shortcuts.history' },
  { id: 'model', keys: 'Ctrl+Shift+I', actionKey: 'chat.shortcuts.model' },
  { id: 'newChat', keys: 'Ctrl+N', actionKey: 'chat.shortcuts.newChat' },
  { id: 'overview', keys: '?', actionKey: 'chat.shortcuts.overview' },
];

export function chatShortcutChord(
  keys: string,
  platform: HostPlatform = detectHostPlatform(),
): string {
  if (platform !== 'macos') return keys;
  return keys.replace(/Ctrl/g, 'Cmd');
}
