/**
 * Declared reset list for shared runtime stores.
 * `setBackend` / `resetBackend` must call this; do not reset stores ad hoc.
 */
import { resetAgentCatalogStore } from './agent-catalog-store';
import { resetAgentStatusStore } from './agent-status-store';
import { resetAppUpdateStore } from './app-update-store';
import { resetConnectionInventoryStore } from './connection-inventory-store';
import { resetTicketWalletStore } from './ticket-wallet-store';

export const RUNTIME_STORE_RESETS = [
  resetAgentCatalogStore,
  resetAgentStatusStore,
  resetConnectionInventoryStore,
  resetTicketWalletStore,
  resetAppUpdateStore,
] as const;

export function resetRuntimeContext(): void {
  for (const reset of RUNTIME_STORE_RESETS) reset();
}
