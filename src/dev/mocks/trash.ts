import type { ConnectionTrashItem, RouteMembershipTrashPayload, TrashPort } from '@/lib/backend/contracts';
import type { Account, AgentKey, Provider } from '@/lib/types';

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
    home: account.home === 'route_pool' ? 'route_pool' : 'connections',
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
    home: provider.home === 'route_pool' ? 'route_pool' : 'connections',
    provider: { ...provider },
  });
}

export function moveMockMembershipToTrash(
  agentId: AgentKey,
  label: string,
  payload: RouteMembershipTrashPayload,
): void {
  const now = new Date();
  mockTrash.unshift({
    id: nextTrashId(),
    agentId,
    kind: 'membership',
    sourceId: payload.sourceId,
    label,
    wasCurrent: false,
    deletedAt: now.toISOString(),
    expiresAt: expiryFrom(now),
    home: 'route_pool',
    membership: payload,
  });
}

export function createMockTrashPort({
  restoreAccount,
  restoreProvider,
  restoreMembership,
}: {
  restoreAccount: (account: Account) => void;
  restoreProvider: (provider: Provider) => void;
  restoreMembership?: (payload: RouteMembershipTrashPayload) => void;
}): TrashPort {
  return {
    async list(agentId?: AgentKey, home?: 'connections' | 'route_pool') {
      const now = Date.now();
      mockTrash = mockTrash.filter((item) => Date.parse(item.expiresAt) > now);
      return mockTrash
        .filter((item) => !agentId || item.agentId === agentId)
        .filter((item) => !home || (item.home ?? 'connections') === home)
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
      if (item.membership) restoreMembership?.(item.membership);
      mockTrash = mockTrash.filter((row) => row.id !== id);
    },
    async permanentlyDelete(id) {
      const before = mockTrash.length;
      mockTrash = mockTrash.filter((row) => row.id !== id);
      if (mockTrash.length === before) throw new Error(`trash item not found: ${id}`);
    },
  };
}
