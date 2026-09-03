import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { setChatBootstrap, takeChatBootstrap } from '@/lib/chat-bootstrap';
import { LegacyStorageKey, StorageKey } from '@/lib/storage-key';

function installMemoryStorage() {
  const store = new Map<string, string>();
  const storage = {
    getItem(key: string) {
      return store.has(key) ? store.get(key)! : null;
    },
    setItem(key: string, value: string) {
      store.set(key, String(value));
    },
    removeItem(key: string) {
      store.delete(key);
    },
    clear() {
      store.clear();
    },
    key(i: number) {
      return [...store.keys()][i] ?? null;
    },
    get length() {
      return store.size;
    },
  };
  Object.defineProperty(globalThis, 'sessionStorage', {
    value: storage,
    configurable: true,
    writable: true,
  });
  return storage;
}

describe('chat-bootstrap', () => {
  beforeEach(() => {
    installMemoryStorage();
  });

  afterEach(() => {
    // @ts-expect-error test teardown
    delete globalThis.sessionStorage;
  });

  it('returns false when sessionStorage cannot write', () => {
    Object.defineProperty(globalThis, 'sessionStorage', {
      value: {
        setItem() {
          throw new Error('quota');
        },
      },
      configurable: true,
      writable: true,
    });
    expect(
      setChatBootstrap({
        agentIds: ['claude'],
        cwd: null,
        title: 'x',
        prompt: 'y',
      }),
    ).toBe(false);
  });

  it('writes the canonical session key', () => {
    expect(
      setChatBootstrap({
        agentIds: ['claude'],
        cwd: 'D:\\demo',
        title: 'from projects',
        prompt: 'continue please',
      }),
    ).toBe(true);
    expect(sessionStorage.getItem(StorageKey.chatBootstrap)).toContain('continue please');
    expect(sessionStorage.getItem(LegacyStorageKey.chatBootstrap)).toBeNull();
  });

  it('set then take returns payload once', () => {
    expect(setChatBootstrap({
      agentIds: ['claude'],
      cwd: 'D:\\demo',
      title: 'from projects',
      prompt: 'continue please',
    })).toBe(true);
    const once = takeChatBootstrap();
    expect(once).toEqual({
      agentIds: ['claude'],
      cwd: 'D:\\demo',
      title: 'from projects',
      prompt: 'continue please',
    });
    expect(takeChatBootstrap()).toBeNull();
  });

  it('rejects empty agentIds', () => {
    sessionStorage.setItem(
      StorageKey.chatBootstrap,
      JSON.stringify({ agentIds: [], prompt: 'x' }),
    );
    expect(takeChatBootstrap()).toBeNull();
  });

  it('clears corrupt payload on the canonical key', () => {
    sessionStorage.setItem(StorageKey.chatBootstrap, '{not-json');
    expect(takeChatBootstrap()).toBeNull();
    expect(sessionStorage.getItem(StorageKey.chatBootstrap)).toBeNull();
  });

  it('consumes a leftover dotted session key once', () => {
    sessionStorage.setItem(
      LegacyStorageKey.chatBootstrap,
      JSON.stringify({
        agentIds: ['codex'],
        cwd: null,
        title: 'legacy',
        prompt: 'old',
      }),
    );
    expect(takeChatBootstrap()).toEqual({
      agentIds: ['codex'],
      cwd: null,
      title: 'legacy',
      prompt: 'old',
    });
    expect(sessionStorage.getItem(LegacyStorageKey.chatBootstrap)).toBeNull();
    expect(sessionStorage.getItem(StorageKey.chatBootstrap)).toBeNull();
    expect(takeChatBootstrap()).toBeNull();
  });

  it('clears a leftover dotted corrupt payload', () => {
    sessionStorage.setItem(LegacyStorageKey.chatBootstrap, '{not-json');
    expect(takeChatBootstrap()).toBeNull();
    expect(sessionStorage.getItem(LegacyStorageKey.chatBootstrap)).toBeNull();
    expect(sessionStorage.getItem(StorageKey.chatBootstrap)).toBeNull();
  });
});
