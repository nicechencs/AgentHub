import * as React from 'react';
import { getBackend } from './backend-runtime';
import {
  getTicketWalletSnapshot,
  loadTicketWallet,
  subscribeTicketWallet,
  type TicketWalletSnapshot,
} from './ticket-wallet-store';

export function useTicketWallet(): TicketWalletSnapshot & {
  reload: () => Promise<void>;
  ensureLoaded: () => Promise<void>;
} {
  const snapshot = React.useSyncExternalStore(
    subscribeTicketWallet,
    getTicketWalletSnapshot,
    getTicketWalletSnapshot,
  );
  const reload = React.useCallback(async () => {
    await loadTicketWallet(getBackend(), { force: true });
  }, []);
  const ensureLoaded = React.useCallback(async () => {
    await loadTicketWallet(getBackend());
  }, []);
  return { ...snapshot, reload, ensureLoaded };
}
