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
export { AgentCatalogProvider, useAgentCatalog, useAgentCatalogOptional } from './AgentCatalogProvider';
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
export {
  ConnectionPoolProvider,
  useConnectionPool,
  useConnectionPoolOptional,
} from './ConnectionPoolProvider';

export {
  getAppUpdateAvailable,
  resetAppUpdateStore,
  setAppUpdateAvailable,
  subscribeAppUpdate,
} from './app-update-store';
export { useAppUpdateAvailable } from './useAppUpdateAvailable';

export {
  getBridgePresenceSnapshot,
  loadBridgePresence,
  notifyBridgePresenceChanged,
  resetBridgePresenceStore,
  shouldShowBridgesNav,
  subscribeBridgePresence,
} from './bridge-presence-store';
export type { BridgePresenceSnapshot } from './bridge-presence-store';
export { useBridgePresence } from './useBridgePresence';

export type { Backend } from '@/lib/backend/contracts';
