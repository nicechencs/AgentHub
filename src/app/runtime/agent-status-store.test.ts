import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Backend } from '@/lib/backend/contracts';
import {
  applyAgentHidden,
  getAgentStatusSnapshot,
  liveAuthProbeForAgent,
  loadAgentStatuses,
  resetAgentStatusStore,
  revertAgentHidden,
} from './agent-status-store';
import { resetConnectionInventoryStore } from './connection-inventory-store';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe('agent-status-store', () => {
  beforeEach(() => {
    vi.useRealTimers();
    resetAgentStatusStore();
    resetConnectionInventoryStore();
  });

  it('deduplicates concurrent detection and exposes ready state', async () => {
    const backend = {
      agent: {
        listAgents: vi.fn(async () => [
          { agentId: 'claude', installed: true },
        ]),
      },
    } as unknown as Backend;

    const [first, second] = await Promise.all([
      loadAgentStatuses(backend),
      loadAgentStatuses(backend),
    ]);

    expect(backend.agent.listAgents).toHaveBeenCalledOnce();
    expect(first.state).toBe('ready');
    expect(second.statuses[0]?.installed).toBe(true);
  });

  it('probes installed agents centrally and projects live auth onto their statuses', async () => {
    const probeLiveAuth = vi.fn(async (agentId: string) => ({
      agentId,
      kind: 'oauth',
      summary: 'verified live credentials',
      hasCredentials: true,
      health: 'verified' as const,
      source: 'auth.json',
      revision: 'rev-1',
    }));
    const backend = {
      agent: {
        listAgents: vi.fn(async () => [
          { agentId: 'claude', installed: true, authStatus: 'none', authLabel: '未配置', running: false },
          { agentId: 'codex', installed: false, authStatus: 'none', authLabel: '未配置', running: false },
        ]),
      },
      account: { probeLiveAuth },
    } as unknown as Backend;

    const loaded = await loadAgentStatuses(backend);

    expect(probeLiveAuth).toHaveBeenCalledTimes(1);
    expect(probeLiveAuth).toHaveBeenCalledWith('claude');
    expect(loaded.statuses[0]).toMatchObject({
      authHealth: 'verified',
      authLabel: '已验证',
      authSource: 'auth.json',
      authRevision: 'rev-1',
    });
    expect(liveAuthProbeForAgent(loaded, 'claude')).toMatchObject({
      agentId: 'claude',
      kind: 'oauth',
      hasCredentials: true,
    });
    expect(loaded.statuses[1]).not.toHaveProperty('authHealth');
  });

  it('keeps deferred live probes scoped to their requested agent during an agent switch', async () => {
    const claude = deferred<{
      agentId: string;
      kind: string;
      summary: string;
      hasCredentials: boolean;
      health: 'verified';
    }>();
    const codex = deferred<{
      agentId: string;
      kind: string;
      summary: string;
      hasCredentials: boolean;
      health: 'configured';
    }>();
    const probeLiveAuth = vi.fn((agentId: string) =>
      agentId === 'claude' ? claude.promise : codex.promise,
    );
    const backend = {
      agent: {
        listAgents: vi.fn(async () => [
          { agentId: 'claude', installed: true, authStatus: 'none', authLabel: '未配置', running: false },
          { agentId: 'codex', installed: true, authStatus: 'none', authLabel: '未配置', running: false },
        ]),
      },
      account: { probeLiveAuth },
    } as unknown as Backend;

    const loading = loadAgentStatuses(backend);

    // listAgents 先完成时：主界面已可渲染（ready + refreshing），live-auth 仍在飞
    await vi.waitFor(() => {
      const mid = getAgentStatusSnapshot();
      expect(mid.state).toBe('ready');
      expect(mid.refreshing).toBe(true);
      expect(mid.statuses).toHaveLength(2);
    });
    expect(liveAuthProbeForAgent(getAgentStatusSnapshot(), 'codex')).toBeUndefined();

    claude.resolve({
      agentId: 'claude',
      kind: 'oauth',
      summary: 'ready',
      hasCredentials: true,
      health: 'verified',
    });
    codex.resolve({
      agentId: 'codex',
      kind: 'api_key',
      summary: 'ready',
      hasCredentials: true,
      health: 'configured',
    });
    const ready = await loading;

    expect(ready.refreshing).toBe(false);
    expect(liveAuthProbeForAgent(ready, 'claude')?.agentId).toBe('claude');
    expect(liveAuthProbeForAgent(ready, 'codex')?.agentId).toBe('codex');
    expect(
      liveAuthProbeForAgent(
        { liveAuthProbes: { codex: { ...liveAuthProbeForAgent(ready, 'claude')!, agentId: 'claude' } } },
        'codex',
      ),
    ).toBeUndefined();
  });

  it('publishes detect results before live-auth enrichment finishes', async () => {
    const auth = deferred<{
      agentId: string;
      kind: string;
      summary: string;
      hasCredentials: boolean;
      health: 'verified';
    }>();
    const backend = {
      agent: {
        listAgents: vi.fn(async () => [
          { agentId: 'claude', installed: true, authStatus: 'none', authLabel: '未配置', running: false },
        ]),
      },
      account: {
        probeLiveAuth: vi.fn(() => auth.promise),
      },
    } as unknown as Backend;

    const loading = loadAgentStatuses(backend);
    await vi.waitFor(() => {
      expect(getAgentStatusSnapshot()).toMatchObject({
        state: 'ready',
        refreshing: true,
      });
    });
    expect(getAgentStatusSnapshot().statuses[0]?.installed).toBe(true);
    expect(liveAuthProbeForAgent(getAgentStatusSnapshot(), 'claude')).toBeUndefined();

    auth.resolve({
      agentId: 'claude',
      kind: 'oauth',
      summary: 'verified',
      hasCredentials: true,
      health: 'verified',
    });
    const ready = await loading;
    expect(ready).toMatchObject({ state: 'ready', refreshing: false });
    expect(liveAuthProbeForAgent(ready, 'claude')?.health).toBe('verified');
  });

  it('merges the shared connection pool after detect and before live-auth', async () => {
    const auth = deferred<{
      agentId: string;
      kind: string;
      summary: string;
      hasCredentials: boolean;
      health: 'verified';
    }>();
    const listAccounts = vi.fn(async () => [
      {
        id: 'acc-1',
        agentId: 'claude',
        kind: 'oauth' as const,
        label: 'me@example.com',
        isCurrent: true,
        tokenValid: true,
      },
    ]);
    const listProviders = vi.fn(async () => []);
    const backend = {
      agent: {
        listAgents: vi.fn(async () => [
          { agentId: 'claude', installed: true, authStatus: 'none', authLabel: '未配置', running: false },
        ]),
      },
      account: {
        listAccounts,
        probeLiveAuth: vi.fn(() => auth.promise),
      },
      provider: { listProviders },
    } as unknown as Backend;

    const loading = loadAgentStatuses(backend);
    await vi.waitFor(() => {
      const snap = getAgentStatusSnapshot();
      expect(snap.state).toBe('ready');
      expect(listAccounts).toHaveBeenCalledOnce();
      expect(snap.statuses[0]?.effectiveLabel).toBe('me@example.com');
    });
    expect(liveAuthProbeForAgent(getAgentStatusSnapshot(), 'claude')).toBeUndefined();

    auth.resolve({
      agentId: 'claude',
      kind: 'oauth',
      summary: 'verified',
      hasCredentials: true,
      health: 'verified',
    });
    await loading;
  });

  it('keeps backend failure distinct from an empty installed result', async () => {
    vi.useFakeTimers();
    const backend = {
      agent: {
        listAgents: vi.fn(async () => {
          throw new Error('agent probe failed');
        }),
      },
    } as unknown as Backend;

    // Attach the assertion before advancing timers so the rejection is handled.
    const pending = expect(loadAgentStatuses(backend)).rejects.toThrow('agent probe failed');
    await vi.runAllTimersAsync();
    await pending;
    expect(backend.agent.listAgents).toHaveBeenCalledTimes(3);
    const snapshot = getAgentStatusSnapshot();
    expect(snapshot.state).toBe('error');
    expect(snapshot.statuses).toEqual([]);
    expect(snapshot.error).toBeInstanceOf(Error);
    vi.useRealTimers();
  });

  it('recovers from a transient cold-start listAgents failure without showing error', async () => {
    vi.useFakeTimers();
    const listAgents = vi
      .fn()
      .mockRejectedValueOnce(new Error('IPC not ready'))
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true }]);
    const backend = { agent: { listAgents } } as unknown as Backend;

    const pending = loadAgentStatuses(backend);
    expect(getAgentStatusSnapshot().state).toBe('loading');
    await vi.runAllTimersAsync();
    const snapshot = await pending;
    expect(listAgents).toHaveBeenCalledTimes(2);
    expect(snapshot.state).toBe('ready');
    expect(snapshot.statuses[0]?.installed).toBe(true);
    vi.useRealTimers();
  });

  it('runs a fresh request when a forced refresh arrives during an in-flight request', async () => {
    let resolveFirst!: (agents: Array<{ agentId: string; installed: boolean }>) => void;
    const firstRequest = new Promise<Array<{ agentId: string; installed: boolean }>>((resolve) => {
      resolveFirst = resolve;
    });
    const listAgents = vi
      .fn()
      .mockImplementationOnce(() => firstRequest)
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true }]);
    const backend = { agent: { listAgents } } as unknown as Backend;

    const initial = loadAgentStatuses(backend);
    await Promise.resolve();
    const forced = loadAgentStatuses(backend, { force: true });

    resolveFirst([{ agentId: 'claude', installed: false }]);
    await initial;
    const refreshed = await forced;

    expect(listAgents).toHaveBeenCalledTimes(2);
    expect(refreshed.statuses[0]?.installed).toBe(true);
  });

  it('keeps the ready snapshot renderable while a forced refresh is pending', async () => {
    const nextAgents = deferred<Array<{ agentId: string; installed: boolean }>>();
    const listAgents = vi
      .fn()
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true }])
      .mockImplementationOnce(() => nextAgents.promise);
    const probeLiveAuth = vi.fn(async (agentId: string) => ({
      agentId,
      kind: 'oauth',
      summary: `live-${agentId}`,
      hasCredentials: true,
      health: 'verified' as const,
    }));
    const backend = {
      agent: { listAgents },
      account: { probeLiveAuth },
    } as unknown as Backend;

    const ready = await loadAgentStatuses(backend);
    expect(ready.state).toBe('ready');
    expect(liveAuthProbeForAgent(ready, 'claude')).toBeDefined();

    const refresh = loadAgentStatuses(backend, { force: true });
    const pending = getAgentStatusSnapshot();
    expect(pending).toMatchObject({ state: 'ready', refreshing: true, error: null });
    expect(pending.statuses).toEqual(ready.statuses);
    expect(pending.liveAuthProbes).toEqual({});

    nextAgents.resolve([{ agentId: 'claude', installed: false }]);
    const refreshed = await refresh;
    expect(refreshed).toMatchObject({ state: 'ready', refreshing: false, error: null });
    expect(refreshed.statuses).toEqual([{ agentId: 'claude', installed: false }]);
    expect(refreshed.liveAuthProbes).toEqual({});
  });

  it('restores the complete ready snapshot when a forced refresh fails', async () => {
    const refreshError = new Error('focus refresh failed');
    const listAgents = vi
      .fn()
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true }])
      .mockRejectedValueOnce(refreshError);
    const backend = {
      agent: { listAgents },
      account: {
        probeLiveAuth: vi.fn(async (agentId: string) => ({
          agentId,
          kind: 'oauth',
          summary: `live-${agentId}`,
          hasCredentials: true,
          health: 'verified' as const,
        })),
      },
    } as unknown as Backend;

    const ready = await loadAgentStatuses(backend);
    const refresh = loadAgentStatuses(backend, { force: true });
    expect(getAgentStatusSnapshot()).toMatchObject({ state: 'ready', refreshing: true });
    expect(getAgentStatusSnapshot().liveAuthProbes).toEqual({});

    await expect(refresh).rejects.toThrow('focus refresh failed');
    const restored = getAgentStatusSnapshot();
    expect(restored).toEqual(ready);
    expect(restored).toMatchObject({ state: 'ready', refreshing: false, error: null });
    expect(liveAuthProbeForAgent(restored, 'claude')).toMatchObject({
      agentId: 'claude',
      kind: 'oauth',
    });
  });

  it('stamps hidden locally and keeps it over a stale listAgents result', async () => {
    const listAgents = vi
      .fn()
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }])
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }]);
    const backend = { agent: { listAgents } } as unknown as Backend;

    await loadAgentStatuses(backend);
    applyAgentHidden('claude', true);
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(true);

    const refreshed = await loadAgentStatuses(backend, { force: true });
    expect(listAgents).toHaveBeenCalledTimes(2);
    expect(refreshed.statuses[0]?.hidden).toBe(true);
  });

  it('does not treat an overlaid stale list as backend confirmation', async () => {
    const listAgents = vi
      .fn()
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }])
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }])
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }]);
    const backend = { agent: { listAgents } } as unknown as Backend;

    await loadAgentStatuses(backend);
    applyAgentHidden('claude', true);
    await loadAgentStatuses(backend, { force: true });
    const stillHidden = await loadAgentStatuses(backend, { force: true });
    expect(stillHidden.statuses[0]?.hidden).toBe(true);
  });

  it('drops the pending hide once listAgents confirms it', async () => {
    const listAgents = vi
      .fn()
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }])
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: true }])
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }]);
    const backend = { agent: { listAgents } } as unknown as Backend;

    await loadAgentStatuses(backend);
    applyAgentHidden('claude', true);
    await loadAgentStatuses(backend, { force: true });
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(true);

    const confirmed = await loadAgentStatuses(backend, { force: true });
    expect(confirmed.statuses[0]?.hidden).toBe(false);
  });

  it('keeps a hide that landed during a failed forced refresh', async () => {
    let rejectSecond!: (error: Error) => void;
    const secondRequest = new Promise<Array<{ agentId: string; installed: boolean; hidden?: boolean }>>(
      (_, reject) => {
        rejectSecond = reject;
      },
    );
    const listAgents = vi
      .fn()
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }])
      .mockImplementationOnce(() => secondRequest);
    const backend = { agent: { listAgents } } as unknown as Backend;

    await loadAgentStatuses(backend);
    const refresh = loadAgentStatuses(backend, { force: true });
    applyAgentHidden('claude', true);
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(true);

    rejectSecond(new Error('focus refresh failed'));
    await expect(refresh).rejects.toThrow('focus refresh failed');
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(true);
  });

  it('applies a pending hide when the first listAgents arrives', async () => {
    applyAgentHidden('claude', true);
    const backend = {
      agent: {
        listAgents: vi.fn(async () => [{ agentId: 'claude', installed: true, hidden: false }]),
      },
    } as unknown as Backend;
    const loaded = await loadAgentStatuses(backend);
    expect(loaded.statuses[0]?.hidden).toBe(true);
  });

  it('does not restore a reverted hide when a stale enrich finishes', async () => {
    const list2 = deferred<Array<{ agentId: string; installed: boolean; hidden?: boolean }>>();
    const enrich = deferred<{
      agentId: string;
      kind: string;
      summary: string;
      hasCredentials: boolean;
      health: 'verified';
    }>();
    const listAgents = vi
      .fn()
      .mockResolvedValueOnce([{ agentId: 'claude', installed: true, hidden: false }])
      .mockImplementationOnce(() => list2.promise);
    const probeLiveAuth = vi.fn((agentId: string) => {
      if (listAgents.mock.calls.length >= 2) return enrich.promise;
      return Promise.resolve({
        agentId,
        kind: 'oauth',
        summary: 'ok',
        hasCredentials: true,
        health: 'verified' as const,
      });
    });
    const backend = {
      agent: { listAgents },
      account: { probeLiveAuth },
    } as unknown as Backend;

    await loadAgentStatuses(backend);
    const refresh = loadAgentStatuses(backend, { force: true });
    applyAgentHidden('claude', true);
    list2.resolve([{ agentId: 'claude', installed: true, hidden: false }]);
    await vi.waitFor(() => {
      expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(true);
      expect(probeLiveAuth).toHaveBeenCalledTimes(2);
    });

    revertAgentHidden('claude', false);
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(false);

    enrich.resolve({
      agentId: 'claude',
      kind: 'oauth',
      summary: 'ok',
      hasCredentials: true,
      health: 'verified',
    });
    await refresh;
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(false);
  });

  it('does not let a pre-reset response overwrite the new store or clear a newer inflight', async () => {
    const staleAgents = deferred<Array<{ agentId: string; installed: boolean }>>();
    const nextAgents = deferred<Array<{ agentId: string; installed: boolean }>>();
    const firstBackend = {
      agent: { listAgents: vi.fn(() => staleAgents.promise) },
    } as unknown as Backend;
    const secondBackend = {
      agent: { listAgents: vi.fn(() => nextAgents.promise) },
    } as unknown as Backend;

    const stale = loadAgentStatuses(firstBackend);
    await Promise.resolve();
    resetAgentStatusStore();
    const next = loadAgentStatuses(secondBackend);
    expect(getAgentStatusSnapshot().state).toBe('loading');

    staleAgents.resolve([{ agentId: 'claude', installed: true }]);
    await stale;

    expect(getAgentStatusSnapshot()).toMatchObject({
      state: 'loading',
      statuses: [],
    });

    nextAgents.resolve([{ agentId: 'codex', installed: true }]);
    const loaded = await next;

    expect(loaded).toMatchObject({
      state: 'ready',
      refreshing: false,
      statuses: [{ agentId: 'codex', installed: true }],
    });
    expect(getAgentStatusSnapshot()).toEqual(loaded);
  });

  it('reverts a local hide stamp', async () => {
    const backend = {
      agent: {
        listAgents: vi.fn(async () => [{ agentId: 'claude', installed: true, hidden: false }]),
      },
    } as unknown as Backend;
    await loadAgentStatuses(backend);
    applyAgentHidden('claude', true);
    revertAgentHidden('claude', false);
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(false);
  });
});
