/**
 * Declared reset list for shared runtime stores.
 * `setBackend` / `resetBackend` must call this; do not reset stores ad hoc.
 */
import { resetAgentCatalogStore } from './agent-catalog-store';
import { resetAgentStatusStore } from './agent-status-store';
import { resetAppUpdateStore } from './app-update-store';
import { resetConnectionPoolStore } from './connection-pool-store';
import { resetTicketWalletStore } from './ticket-wallet-store';

export const RUNTIME_STORE_RESETS = [
  resetAgentCatalogStore,
  resetAgentStatusStore,
  resetConnectionPoolStore,
  resetTicketWalletStore,
  resetAppUpdateStore,
] as const;

export function resetRuntimeContext(): void {
  for (const reset of RUNTIME_STORE_RESETS) reset();
}
