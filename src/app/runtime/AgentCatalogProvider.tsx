import * as React from 'react';
import {
  getAgentCatalogSnapshot,
  subscribeAgentCatalog,
  type AgentCatalogSnapshot,
} from './agent-catalog-store';

const AgentCatalogContext = React.createContext<AgentCatalogSnapshot | null>(null);

export function AgentCatalogProvider({ children }: { children: React.ReactNode }) {
  const snap = React.useSyncExternalStore(
    subscribeAgentCatalog,
    getAgentCatalogSnapshot,
    getAgentCatalogSnapshot,
  );
  return (
    <AgentCatalogContext.Provider value={snap}>{children}</AgentCatalogContext.Provider>
  );
}

/** Catalog load status + entries. Throws if used outside provider. */
export function useAgentCatalog(): AgentCatalogSnapshot {
  const ctx = React.useContext(AgentCatalogContext);
  if (!ctx) {
    throw new Error('useAgentCatalog must be used within AgentCatalogProvider');
  }
  return ctx;
}
