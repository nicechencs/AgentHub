/**
 * Application composition root — selects and holds the Backend instance.
 */
export { getBackend, resetBackend, setBackend } from './backend-runtime';

export {
  getAgentCatalogSnapshot,
  loadAgentCatalog,
  resetAgentCatalogStore,
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
  resetAgentStatusStore,
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
  beginConnectionPoolMutation,
  connectionCountsByAgent,
  endConnectionPoolMutation,
  getConnectionPoolSnapshot,
  loadConnectionPool,
  markConnectionCurrent,
  notifyConnectionPoolChanged,
  providersForAgent,
  resetConnectionPoolStore,
  subscribeConnectionPool,
} from './connection-pool-store';
export type {
  ConnectionPoolLoadState,
  ConnectionPoolSnapshot,
} from './connection-pool-store';
export { useConnectionPool } from './ConnectionPoolProvider';

export {
  getTicketWalletSnapshot,
  loadTicketWallet,
  notifyTicketWalletChanged,
  resetTicketWalletStore,
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
  resetAppUpdateStore,
  setAppUpdateAvailable,
  subscribeAppUpdate,
} from './app-update-store';
export { useAppUpdateAvailable } from './useAppUpdateAvailable';

export type { Backend } from '@/lib/backend/contracts';
