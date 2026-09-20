import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import { loadChatOutlineEnabled, saveChatOutlineEnabled } from './chat-outline-pref';

const store = new Map<string, string>();

beforeEach(() => {
  store.clear();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('chat outline preference', () => {
  it('defaults to on when nothing is stored', () => {
    expect(loadChatOutlineEnabled()).toBe(true);
  });

  it('persists off and on through the canonical key', () => {
    saveChatOutlineEnabled(false);
    expect(store.get(StorageKey.chatOutlineEnabled)).toBe('0');
    expect(loadChatOutlineEnabled()).toBe(false);

    saveChatOutlineEnabled(true);
    expect(store.get(StorageKey.chatOutlineEnabled)).toBe('1');
    expect(loadChatOutlineEnabled()).toBe(true);
  });
});
