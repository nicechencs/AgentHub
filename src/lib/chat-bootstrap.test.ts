import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { setChatBootstrap, takeChatBootstrap } from '@/lib/chat-bootstrap';

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

  it('set then take returns payload once', () => {
    setChatBootstrap({
      agentIds: ['claude'],
      cwd: 'D:\\demo',
      title: 'from projects',
      prompt: 'continue please',
    });
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
      'agenthub.chat.bootstrap',
      JSON.stringify({ agentIds: [], prompt: 'x' }),
    );
    expect(takeChatBootstrap()).toBeNull();
  });

  it('clears corrupt payload', () => {
    sessionStorage.setItem('agenthub.chat.bootstrap', '{not-json');
    expect(takeChatBootstrap()).toBeNull();
    expect(sessionStorage.getItem('agenthub.chat.bootstrap')).toBeNull();
  });
});
