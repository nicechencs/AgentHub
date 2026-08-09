import type { ConnectionTrashItem, TrashPort } from '@/lib/backend/contracts';
import type { Account, AgentId, Provider } from '@/lib/types';

let counter = 0;
let mockTrash: ConnectionTrashItem[] = [];

function nextTrashId(): string {
  counter += 1;
  return `mock-trash-${Date.now()}-${counter}`;
}

function expiryFrom(now: Date): string {
  return new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000).toISOString();
}

export function resetMockTrash(): void {
  counter = 0;
  mockTrash = [];
}

export function moveMockAccountToTrash(account: Account): void {
  const now = new Date();
  mockTrash.unshift({
    id: nextTrashId(),
    agentId: account.agentId,
    kind: 'account',
    sourceId: account.id,
    label: account.label,
    wasCurrent: account.isCurrent,
    deletedAt: now.toISOString(),
    expiresAt: expiryFrom(now),
    account: { ...account },
  });
}

export function moveMockProviderToTrash(provider: Provider): void {
  const now = new Date();
  mockTrash.unshift({
    id: nextTrashId(),
    agentId: provider.agentId,
    kind: 'provider',
    sourceId: provider.id,
    label: provider.name,
    wasCurrent: provider.isCurrent,
    deletedAt: now.toISOString(),
    expiresAt: expiryFrom(now),
    provider: { ...provider },
  });
}

export function createMockTrashPort({
  restoreAccount,
  restoreProvider,
}: {
  restoreAccount: (account: Account) => void;
  restoreProvider: (provider: Provider) => void;
}): TrashPort {
  return {
    async list(agentId?: AgentId) {
      const now = Date.now();
      mockTrash = mockTrash.filter((item) => Date.parse(item.expiresAt) > now);
      return mockTrash
        .filter((item) => !agentId || item.agentId === agentId)
        .map((item) => ({
          ...item,
          account: item.account ? { ...item.account } : undefined,
          provider: item.provider ? { ...item.provider } : undefined,
        }));
    },
    async restore(id) {
      const item = mockTrash.find((row) => row.id === id);
      if (!item) throw new Error(`trash item not found: ${id}`);
      if (item.account) restoreAccount({ ...item.account, isCurrent: false });
      if (item.provider) restoreProvider({ ...item.provider, isCurrent: false });
      mockTrash = mockTrash.filter((row) => row.id !== id);
    },
    async permanentlyDelete(id) {
      const before = mockTrash.length;
      mockTrash = mockTrash.filter((row) => row.id !== id);
      if (mockTrash.length === before) throw new Error(`trash item not found: ${id}`);
    },
  };
}
