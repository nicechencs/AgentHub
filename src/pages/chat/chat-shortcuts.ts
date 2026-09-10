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

type ShortcutKeyRoot = {
  addEventListener(
    type: 'keydown',
    listener: (event: KeyboardEvent) => void,
    options?: boolean | AddEventListenerOptions,
  ): void;
  removeEventListener(
    type: 'keydown',
    listener: (event: KeyboardEvent) => void,
    options?: boolean | AddEventListenerOptions,
  ): void;
};

function defaultShortcutKeyRoots(): ShortcutKeyRoot[] {
  if (typeof document === 'undefined') return [];
  return typeof window === 'undefined' ? [document] : [document, window];
}

/**
 * Capture-phase on document and window. Window bubble never sees many real
 * keydowns in the Tauri webview; window capture still gets `window.dispatchEvent`
 * used by existing tests.
 */
export function subscribeChatShortcutKeydown(
  onKey: (event: KeyboardEvent) => void,
  roots: ShortcutKeyRoot | ShortcutKeyRoot[] = defaultShortcutKeyRoots(),
): () => void {
  const list = Array.isArray(roots) ? roots : [roots];
  const seen = new WeakSet<KeyboardEvent>();
  const wrapped = (event: KeyboardEvent) => {
    if (seen.has(event)) return;
    seen.add(event);
    onKey(event);
  };
  for (const root of list) {
    root.addEventListener('keydown', wrapped, true);
  }
  return () => {
    for (const root of list) {
      root.removeEventListener('keydown', wrapped, true);
    }
  };
}
