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
  getAgentStatusSnapshot,
  liveAuthProbeForAgent,
  loadAgentStatuses,
  resetAgentStatusStore,
  subscribeAgentStatuses,
} from './agent-status-store';
export type { AgentStatusLoadState, AgentStatusSnapshot } from './agent-status-store';
export {
  AgentStatusProvider,
  useAgentStatuses,
  useAgentStatusesOptional,
} from './AgentStatusProvider';

export {
  getAppUpdateAvailable,
  resetAppUpdateStore,
  setAppUpdateAvailable,
  subscribeAppUpdate,
} from './app-update-store';
export { useAppUpdateAvailable } from './useAppUpdateAvailable';

export type { Backend } from '@/lib/backend/contracts';
