import * as React from 'react';
import { getBackend } from './backend-runtime';
import {
  getConnectionPoolSnapshot,
  loadConnectionPool,
  subscribeConnectionPool,
  type ConnectionPoolSnapshot,
} from './connection-pool-store';

const ConnectionPoolContext = React.createContext<ConnectionPoolSnapshot | null>(null);

/** Optional wrapper. Hooks subscribe to the store directly and do not require it. */
export function ConnectionPoolProvider({ children }: { children: React.ReactNode }) {
  const snapshot = React.useSyncExternalStore(
    subscribeConnectionPool,
    getConnectionPoolSnapshot,
    getConnectionPoolSnapshot,
  );
  return (
    <ConnectionPoolContext.Provider value={snapshot}>{children}</ConnectionPoolContext.Provider>
  );
}

export function useConnectionPool(): ConnectionPoolSnapshot & {
  reload: () => Promise<void>;
  ensureLoaded: () => Promise<void>;
} {
  const snapshot = React.useSyncExternalStore(
    subscribeConnectionPool,
    getConnectionPoolSnapshot,
    getConnectionPoolSnapshot,
  );
  const reload = React.useCallback(async () => {
    await loadConnectionPool(getBackend(), { force: true });
  }, []);
  const ensureLoaded = React.useCallback(async () => {
    await loadConnectionPool(getBackend());
  }, []);
  return { ...snapshot, reload, ensureLoaded };
}

export function useConnectionPoolOptional(): ConnectionPoolSnapshot {
  return React.useSyncExternalStore(
    subscribeConnectionPool,
    getConnectionPoolSnapshot,
    getConnectionPoolSnapshot,
  );
}
