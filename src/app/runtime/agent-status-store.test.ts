import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Backend } from '@/lib/backend/contracts';
import {
  getAgentStatusSnapshot,
  loadAgentStatuses,
  resetAgentStatusStore,
} from './agent-status-store';

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
});
