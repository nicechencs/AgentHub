import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentStatus } from '@/lib/types';
import {
  __resetDismissedAlertsForTests,
  buildAlertsFromAgents,
  dismissAlertLocal,
  filterDismissedAlerts,
} from './dashboard-alerts';

function installMemoryLocalStorage() {
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    getItem(key: string) {
      return store.has(key) ? store.get(key)! : null;
    },
    setItem(key: string, value: string) {
      store.set(key, String(value));
    },
    removeItem(key: string) {
      store.delete(key);
    },
    key(index: number) {
      return Array.from(store.keys())[index] ?? null;
    },
  });
}

function base(partial: Partial<AgentStatus> & Pick<AgentStatus, 'agentId'>): AgentStatus {
  return {
    installed: true,
    authStatus: 'valid',
    authLabel: '已登录',
    running: false,
    envReady: true,
    ...partial,
  };
}

describe('buildAlertsFromAgents', () => {
  beforeEach(() => {
    installMemoryLocalStorage();
    __resetDismissedAlertsForTests();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('emits auth and env alerts', () => {
    const alerts = buildAlertsFromAgents([
      base({ agentId: 'claude', authStatus: 'expired' }),
      base({ agentId: 'codex', authStatus: 'expiring' }),
      base({ agentId: 'grok', authStatus: 'none' }),
      base({ agentId: 'kimi', envReady: false }),
      base({
        agentId: 'pi',
        update: {
          agentId: 'pi',
          state: 'update_available',
          latestVersion: '9.9.9',
        },
      }),
    ]);

    expect(alerts.map((a) => a.id)).toEqual([
      'auth-expired:claude',
      'auth-expiring:codex',
      'env-not-ready:kimi',
      'auth-none:grok',
      'upgrade:pi',
    ]);
    expect(new Set(alerts.map((a) => a.actionKind))).toEqual(
      new Set(['refresh-token', 'upgrade']),
    );
  });

  it('skips hidden agents', () => {
    const alerts = buildAlertsFromAgents([
      base({ agentId: 'claude', hidden: true, authStatus: 'expired' }),
      base({ agentId: 'codex', authStatus: 'none' }),
    ]);
    expect(alerts.map((a) => a.id)).toEqual(['auth-none:codex']);
  });

  it('skips uninstalled agents', () => {
    const alerts = buildAlertsFromAgents([
      base({ agentId: 'claude', installed: false, authStatus: 'expired' }),
    ]);
    expect(alerts).toEqual([]);
  });

  it('dismiss filters by fingerprint and resurfaces on change', () => {
    const first = buildAlertsFromAgents([
      base({ agentId: 'claude', authStatus: 'expiring' }),
    ]);
    dismissAlertLocal(first[0].id, first);
    expect(filterDismissedAlerts(first)).toEqual([]);

    const changed = buildAlertsFromAgents([
      base({ agentId: 'claude', authStatus: 'expired' }),
    ]);
    // different id + message → not dismissed
    expect(filterDismissedAlerts(changed).map((a) => a.id)).toEqual([
      'auth-expired:claude',
    ]);
  });
});
