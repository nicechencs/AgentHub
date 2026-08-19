import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  getAgentStatusSnapshot,
  getBackend,
  loadAgentStatuses,
  resetBackend,
  setBackend,
} from '@/app/runtime';
import type { Backend } from '@/lib/backend/contracts';
import type { AuthStatus } from '@/lib/types';
import { setAgentHidden } from './agent';

function agentBackend(overrides: {
  listAgents: Backend['agent']['listAgents'];
  setAgentHidden: Backend['agent']['setAgentHidden'];
}): Backend {
  return {
    agent: {
      listAgents: overrides.listAgents,
      setAgentHidden: overrides.setAgentHidden,
    },
  } as unknown as Backend;
}

describe('setAgentHidden', () => {
  afterEach(() => {
    resetBackend();
  });

  it('stamps hidden without forcing a status reload', async () => {
    const listAgents = vi.fn(async () => [
      {
        agentId: 'claude',
        installed: true,
        hidden: false,
        authStatus: 'none' as AuthStatus,
        authLabel: '未配置',
        running: false,
      },
    ]);
    const persist = vi.fn(async () => {});
    setBackend(agentBackend({ listAgents, setAgentHidden: persist }));

    await loadAgentStatuses(getBackend());
    expect(listAgents).toHaveBeenCalledTimes(1);

    await setAgentHidden('claude', true);

    expect(persist).toHaveBeenCalledOnce();
    expect(persist).toHaveBeenCalledWith('claude', true);
    expect(listAgents).toHaveBeenCalledTimes(1);
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(true);
  });

  it('reverts the stamp when persist fails', async () => {
    const listAgents = vi.fn(async () => [
      {
        agentId: 'claude',
        installed: true,
        hidden: false,
        authStatus: 'none' as AuthStatus,
        authLabel: '未配置',
        running: false,
      },
    ]);
    const persist = vi.fn(async () => {
      throw new Error('disk');
    });
    setBackend(agentBackend({ listAgents, setAgentHidden: persist }));

    await loadAgentStatuses(getBackend());
    await expect(setAgentHidden('claude', true)).rejects.toThrow('disk');
    expect(getAgentStatusSnapshot().statuses[0]?.hidden).toBe(false);
    expect(listAgents).toHaveBeenCalledTimes(1);
  });
});
