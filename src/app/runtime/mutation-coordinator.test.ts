import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Backend } from '@/lib/backend/contracts';
import { getAgentStatusSnapshot, resetAgentStatusStore } from './agent-status-store';
import { getConnectionInventorySnapshot, resetConnectionInventoryStore } from './connection-inventory-store';
import { refreshRuntimeReadModels } from './mutation-coordinator';
import { getTicketWalletSnapshot, resetTicketWalletStore } from './ticket-wallet-store';

function failingBackend(): Backend {
  return {
    agent: {
      listAgents: vi.fn(async () => {
        throw new Error('agents down');
      }),
    },
    account: {
      listAccounts: vi.fn(async () => {
        throw new Error('accounts down');
      }),
    },
    provider: {
      listProviders: vi.fn(async () => {
        throw new Error('providers down');
      }),
    },
    ticket: {
      listWallet: vi.fn(async () => {
        throw new Error('wallet down');
      }),
    },
  } as unknown as Backend;
}

describe('refreshRuntimeReadModels', () => {
  beforeEach(() => {
    resetAgentStatusStore();
    resetConnectionInventoryStore();
    resetTicketWalletStore();
  });

  it('does not throw when a follow-up read model refresh fails', async () => {
    await expect(refreshRuntimeReadModels(failingBackend())).resolves.toBeUndefined();
  });

  it('records refresh failures on the shared snapshots', async () => {
    await refreshRuntimeReadModels(failingBackend());

    expect(getAgentStatusSnapshot().error).toBeInstanceOf(Error);
    expect(getConnectionInventorySnapshot().errors.accounts).toBeInstanceOf(Error);
    expect(getConnectionInventorySnapshot().errors.providers).toBeInstanceOf(Error);
    expect(getTicketWalletSnapshot().error).toBeInstanceOf(Error);
  });

  it('refreshes only the requested models', async () => {
    const backend = failingBackend();
    await refreshRuntimeReadModels(backend, { models: ['ticketWallet'] });

    expect(backend.ticket.listWallet).toHaveBeenCalledOnce();
    expect(backend.agent.listAgents).not.toHaveBeenCalled();
    expect(backend.account.listAccounts).not.toHaveBeenCalled();
    expect(getTicketWalletSnapshot().error).toBeInstanceOf(Error);
    expect(getAgentStatusSnapshot().error).toBeNull();
  });
});
