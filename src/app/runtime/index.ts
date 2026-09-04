/**
 * Application composition root — selects and holds the Backend instance.
 */
export { getBackend, resetBackend, setBackend } from './backend-runtime';

export {
  getAgentCatalogSnapshot,
  loadAgentCatalog,
  seedAgentCatalog,
  subscribeAgentCatalog,
} from './agent-catalog-store';
export type { AgentCatalogSnapshot, AgentCatalogStatus } from './agent-catalog-store';
export { AgentCatalogProvider, useAgentCatalog } from './AgentCatalogProvider';
export {
  applyAgentHidden,
  getAgentStatusSnapshot,
  liveAuthProbeForAgent,
  loadAgentStatuses,
  revertAgentHidden,
  subscribeAgentStatuses,
} from './agent-status-store';
export type { AgentStatusLoadState, AgentStatusSnapshot } from './agent-status-store';
export {
  AgentStatusProvider,
  useAgentStatuses,
  useAgentStatusesOptional,
} from './AgentStatusProvider';

export {
  accountsForAgent,
  beginConnectionInventoryMutation,
  connectionCountsByAgent,
  endConnectionInventoryMutation,
  getConnectionInventorySnapshot,
  loadConnectionInventory,
  markConnectionCurrent,
  notifyConnectionInventoryChanged,
  providersForAgent,
  subscribeConnectionInventory,
} from './connection-inventory-store';
export type {
  ConnectionInventoryLoadState,
  ConnectionInventorySnapshot,
} from './connection-inventory-store';
export { useConnectionInventory } from './ConnectionInventoryProvider';

export {
  getTicketWalletSnapshot,
  loadTicketWallet,
  notifyTicketWalletChanged,
  removeTicketFromWalletSnapshot,
  subscribeTicketWallet,
} from './ticket-wallet-store';
export type {
  TicketWalletLoadState,
  TicketWalletSnapshot,
} from './ticket-wallet-store';
export { useTicketWallet } from './TicketWalletProvider';
export { refreshRuntimeReadModels } from './mutation-coordinator';
export type { RuntimeReadModel } from './mutation-coordinator';

export {
  getAppUpdateAvailable,
  setAppUpdateAvailable,
  subscribeAppUpdate,
} from './app-update-store';
export { useAppUpdateAvailable } from './useAppUpdateAvailable';

export type { Backend } from '@/lib/backend/contracts';
