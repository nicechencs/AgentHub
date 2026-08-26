import type { Backend } from '@/lib/backend/contracts';
import { createBackend } from '@/lib/backend/current';
import { resetAgentCatalogStore } from './agent-catalog-store';
import { resetAgentStatusStore } from './agent-status-store';
import { resetConnectionPoolStore } from './connection-pool-store';
import { resetTicketWalletStore } from './ticket-wallet-store';

let instance: Backend | null = null;

export function getBackend(): Backend {
  if (!instance) instance = createBackend();
  return instance;
}

/** Tests / advanced: replace backend instance. */
function resetRuntimeStores(): void {
  resetAgentCatalogStore();
  resetAgentStatusStore();
  resetConnectionPoolStore();
  resetTicketWalletStore();
}

export function setBackend(backend: Backend): void {
  instance = backend;
  resetRuntimeStores();
}

export function resetBackend(): void {
  instance = null;
  resetRuntimeStores();
}
