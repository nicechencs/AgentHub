import * as React from 'react';
import {
  getAgentStatusSnapshot,
  loadAgentStatuses,
  subscribeAgentStatuses,
  type AgentStatusSnapshot,
} from './agent-status-store';
import { getBackend } from './backend-runtime';

const AgentStatusContext = React.createContext<AgentStatusSnapshot | null>(null);

export function AgentStatusProvider({ children }: { children: React.ReactNode }) {
  const snapshot = React.useSyncExternalStore(
    subscribeAgentStatuses,
    getAgentStatusSnapshot,
    getAgentStatusSnapshot,
  );

  // Returning to the desktop app is a meaningful boundary for external CLI
  // token rotation. One shared forced reload avoids every page probing alone.
  React.useEffect(() => {
    const onFocus = () => {
      void loadAgentStatuses(getBackend(), { force: true }).catch(() => {});
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
