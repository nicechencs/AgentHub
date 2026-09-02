import * as React from 'react';
import { getBackend } from './backend-runtime';
import {
  getConnectionInventorySnapshot,
  loadConnectionInventory,
  subscribeConnectionInventory,
  type ConnectionInventorySnapshot,
} from './connection-inventory-store';

export function useConnectionInventory(): ConnectionInventorySnapshot & {
  reload: () => Promise<void>;
  ensureLoaded: () => Promise<void>;
} {
  const snapshot = React.useSyncExternalStore(
    subscribeConnectionInventory,
    getConnectionInventorySnapshot,
    getConnectionInventorySnapshot,
  );
  const reload = React.useCallback(async () => {
    await loadConnectionInventory(getBackend(), { force: true });
  }, []);
  const ensureLoaded = React.useCallback(async () => {
    await loadConnectionInventory(getBackend());
  }, []);
  return { ...snapshot, reload, ensureLoaded };
}

/** @deprecated Use useConnectionInventory */
export const useConnectionPool = useConnectionInventory;
