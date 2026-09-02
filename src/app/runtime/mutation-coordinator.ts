/**
 * After a domain write, refresh shared read models once.
 * Write success is independent of refresh: failures stay on the store snapshots.
 */
import type { Backend } from '@/lib/backend/contracts';
import { clearLiveAuthProbeCache } from '@/lib/backend/contracts/live-auth-probe-cache';
import type { AgentKey } from '@/lib/types';
import { loadAgentStatuses } from './agent-status-store';
import { notifyConnectionInventoryChanged } from './connection-inventory-store';
import { notifyTicketWalletChanged } from './ticket-wallet-store';

export type RuntimeReadModel = 'agentStatus' | 'connectionInventory' | 'ticketWallet';

const ALL_READ_MODELS: readonly RuntimeReadModel[] = [
  'agentStatus',
  'connectionInventory',
  'ticketWallet',
];

export async function refreshRuntimeReadModels(
  backend: Backend,
  opts: {
    agentId?: AgentKey;
    clearProbe?: boolean;
    models?: readonly RuntimeReadModel[];
  } = {},
): Promise<void> {
  if (opts.clearProbe && opts.agentId) {
    clearLiveAuthProbeCache(opts.agentId);
  }
  const models = opts.models ?? ALL_READ_MODELS;
  const jobs: Promise<unknown>[] = [];
  if (models.includes('agentStatus')) {
    jobs.push(loadAgentStatuses(backend, { force: true }));
  }
  if (models.includes('connectionInventory')) {
    jobs.push(notifyConnectionInventoryChanged(backend));
  }
  if (models.includes('ticketWallet')) {
    jobs.push(notifyTicketWalletChanged(backend));
  }
  await Promise.allSettled(jobs);
}
