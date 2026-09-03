import * as React from 'react';
import {
  getAgentStatusSnapshot,
  loadAgentStatuses,
  subscribeAgentStatuses,
  type AgentStatusSnapshot,
} from './agent-status-store';
import { getBackend } from './backend-runtime';
import { notifyConnectionInventoryChanged } from './connection-inventory-store';

const AgentStatusContext = React.createContext<AgentStatusSnapshot | null>(null);

export function AgentStatusProvider({ children }: { children: React.ReactNode }) {
  const snapshot = React.useSyncExternalStore(
    subscribeAgentStatuses,
    getAgentStatusSnapshot,
    getAgentStatusSnapshot,
  );

  // Boot / first paint: if main preload was skipped or still idle, kick once.
  React.useEffect(() => {
    if (getAgentStatusSnapshot().state === 'idle') {
      void loadAgentStatuses(getBackend()).catch(() => {});
    }
  }, []);

  // Returning to the desktop app is a meaningful boundary for external CLI
  // token rotation. One shared forced reload avoids every page probing alone.
  React.useEffect(() => {
    const onFocus = () => {
      const backend = getBackend();
      void (async () => {
        if (backend.account.reconcileAccounts) {
          try {
            await backend.account.reconcileAccounts();
            await notifyConnectionInventoryChanged(backend);
          } catch {
            // Keep the last pool; agent status still re-detects.
          }
        }
        await loadAgentStatuses(backend, { force: true });
      })().catch(() => {});
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  return (
    <AgentStatusContext.Provider value={snapshot}>
      {children}
    </AgentStatusContext.Provider>
  );
}

export function useAgentStatuses(): AgentStatusSnapshot & { reload: () => Promise<void> } {
  const snapshot = React.useContext(AgentStatusContext);
  if (!snapshot) throw new Error('useAgentStatuses must be used within AgentStatusProvider');

  const reload = React.useCallback(async () => {
    await loadAgentStatuses(getBackend(), { force: true });
  }, []);

  return { ...snapshot, reload };
}

export function useAgentStatusesOptional(): AgentStatusSnapshot {
  return React.useSyncExternalStore(
    subscribeAgentStatuses,
    getAgentStatusSnapshot,
    getAgentStatusSnapshot,
  );
}
