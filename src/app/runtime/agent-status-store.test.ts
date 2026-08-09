import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Backend } from '@/lib/backend/contracts';
import {
  getAgentStatusSnapshot,
  liveAuthProbeForAgent,
  loadAgentStatuses,
  resetAgentStatusStore,
} from './agent-status-store';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe('agent-status-store', () => {
  beforeEach(() => resetAgentStatusStore());

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
    await Promise.resolve();
    await Promise.resolve();
    expect(getAgentStatusSnapshot().state).toBe('loading');
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

    expect(liveAuthProbeForAgent(ready, 'claude')?.agentId).toBe('claude');
    expect(liveAuthProbeForAgent(ready, 'codex')?.agentId).toBe('codex');
    expect(
      liveAuthProbeForAgent(
        { liveAuthProbes: { codex: { ...liveAuthProbeForAgent(ready, 'claude')!, agentId: 'claude' } } },
        'codex',
      ),
    ).toBeUndefined();
  });

  it('keeps backend failure distinct from an empty installed result', async () => {
    const backend = {
      agent: {
        listAgents: vi.fn(async () => {
          throw new Error('agent probe failed');
        }),
      },
    } as unknown as Backend;

    await expect(loadAgentStatuses(backend)).rejects.toThrow('agent probe failed');
    const snapshot = getAgentStatusSnapshot();
    expect(snapshot.state).toBe('error');
    expect(snapshot.statuses).toEqual([]);
    expect(snapshot.error).toBeInstanceOf(Error);
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
});
