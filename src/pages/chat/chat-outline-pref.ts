import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';

export function loadChatOutlineEnabled(): boolean {
  return loadBool(StorageKey.chatOutlineEnabled, true);
}

export function saveChatOutlineEnabled(value: boolean): void {
  saveBool(StorageKey.chatOutlineEnabled, value);
}
