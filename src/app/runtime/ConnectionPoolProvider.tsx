import * as React from 'react';
import { getBackend } from './backend-runtime';
import {
  getConnectionPoolSnapshot,
  loadConnectionPool,
  subscribeConnectionPool,
  type ConnectionPoolSnapshot,
} from './connection-pool-store';

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
