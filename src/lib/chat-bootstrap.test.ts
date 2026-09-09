import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  chatBootstrapGeneration,
  isChatBootstrapHandoff,
  restoreChatBootstrapIfUnchanged,
  setChatBootstrap,
  setChatBootstrapFitting,
  takeChatBootstrap,
} from '@/lib/chat-bootstrap';
import { StorageKey } from '@/lib/storage-key';

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

  it('shrinks the prompt until the write succeeds', () => {
    let writes = 0;
    Object.defineProperty(globalThis, 'sessionStorage', {
      value: {
        setItem(_key: string, value: string) {
          writes += 1;
          const parsed = JSON.parse(value) as { prompt?: string };
          if ((parsed.prompt?.length ?? 0) > 10) throw new Error('quota');
        },
      },
      configurable: true,
      writable: true,
    });
    expect(
      setChatBootstrapFitting(
        { agentIds: ['claude'], prompt: 'x'.repeat(80) },
        (limit) => 'p'.repeat(Math.min(limit, 8)),
      ),
    ).toBe(true);
    expect(writes).toBeGreaterThan(1);
  });

  it('rejects empty agentIds unless a working directory is set', () => {
    sessionStorage.setItem(
      StorageKey.chatBootstrap,
      JSON.stringify({ agentIds: [], prompt: 'x' }),
    );
    expect(takeChatBootstrap()).toBeNull();
    expect(
      setChatBootstrap({
        agentIds: [],
        cwd: 'D:\\work\\app',
        title: 'app',
      }),
    ).toBe(true);
    expect(takeChatBootstrap()).toEqual({
      agentIds: [],
      cwd: 'D:\\work\\app',
      title: 'app',
    });
  });

  it('clears corrupt payload on the canonical key', () => {
    sessionStorage.setItem(StorageKey.chatBootstrap, '{not-json');
    expect(takeChatBootstrap()).toBeNull();
    expect(sessionStorage.getItem(StorageKey.chatBootstrap)).toBeNull();
  });

  it('accepts a session id without a working directory', () => {
    expect(
      setChatBootstrap({
        agentIds: ['claude'],
        sessionId: 'sess-1',
        history: [{ role: 'user', content: 'hi' }],
      }),
    ).toBe(true);
    expect(takeChatBootstrap()).toEqual({
      agentIds: ['claude'],
      sessionId: 'sess-1',
      history: [{ role: 'user', content: 'hi' }],
    });
  });

  it('recognizes shell and projects handoff query values', () => {
    expect(isChatBootstrapHandoff('shell')).toBe(true);
    expect(isChatBootstrapHandoff('projects')).toBe(true);
    expect(isChatBootstrapHandoff('settings')).toBe(false);
    expect(isChatBootstrapHandoff(null)).toBe(false);
  });

  it('restores a taken payload when nothing newer was written', () => {
    const payload = { agentIds: [] as string[], cwd: 'D:\\one', title: 'one' };
    expect(setChatBootstrap(payload)).toBe(true);
    const generation = chatBootstrapGeneration();
    expect(takeChatBootstrap()).toEqual(payload);
    expect(restoreChatBootstrapIfUnchanged(payload, generation)).toBe(true);
    expect(takeChatBootstrap()).toEqual(payload);
  });

  it('does not restore over a newer handoff payload or generation', () => {
    const first = { agentIds: [] as string[], cwd: 'D:\\one', title: 'one' };
    const second = { agentIds: [] as string[], cwd: 'D:\\two', title: 'two' };
    expect(setChatBootstrap(first)).toBe(true);
    const firstGeneration = chatBootstrapGeneration();
    expect(takeChatBootstrap()).toEqual(first);
    expect(setChatBootstrap(second)).toBe(true);
    expect(restoreChatBootstrapIfUnchanged(first, firstGeneration)).toBe(false);
    expect(takeChatBootstrap()).toEqual(second);
  });

  it('does not restore over a different payload written at the same generation', () => {
    const first = { agentIds: [] as string[], cwd: 'D:\\one', title: 'one' };
    expect(setChatBootstrap(first)).toBe(true);
    const generation = chatBootstrapGeneration();
    expect(takeChatBootstrap()).toEqual(first);
    sessionStorage.setItem(
      StorageKey.chatBootstrap,
      JSON.stringify({ agentIds: [], cwd: 'D:\\two', title: 'two' }),
    );
    expect(restoreChatBootstrapIfUnchanged(first, generation)).toBe(false);
    expect(takeChatBootstrap()).toEqual({
      agentIds: [],
      cwd: 'D:\\two',
      title: 'two',
    });
  });
});
