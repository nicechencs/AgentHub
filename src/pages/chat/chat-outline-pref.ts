import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';

const listeners = new Set<(enabled: boolean) => void>();

export function loadChatOutlineEnabled(): boolean {
  return loadBool(StorageKey.chatOutlineEnabled, true);
}

export function saveChatOutlineEnabled(value: boolean): void {
  saveBool(StorageKey.chatOutlineEnabled, value);
  for (const listener of listeners) listener(value);
}

export function subscribeChatOutlineEnabled(listener: (enabled: boolean) => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
