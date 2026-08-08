/**
 * Application composition root — selects and holds the Backend instance.
 */
import type { Backend } from '@/lib/backend/contracts';
import { createBackend } from '@/lib/backend/current';

export {
  getAgentCatalogSnapshot,
  loadAgentCatalog,
  resetAgentCatalogStore,
  seedAgentCatalog,
  subscribeAgentCatalog,
} from './agent-catalog-store';
export type { AgentCatalogSnapshot, AgentCatalogStatus } from './agent-catalog-store';
export { AgentCatalogProvider, useAgentCatalog, useAgentCatalogOptional } from './AgentCatalogProvider';

let instance: Backend | null = null;

export function getBackend(): Backend {
  if (!instance) {
    instance = createBackend();
  }
  return instance;
}

/** Tests / advanced: replace backend instance. */
export function setBackend(backend: Backend): void {
  instance = backend;
}

export function resetBackend(): void {
  instance = null;
}

export type { Backend };
