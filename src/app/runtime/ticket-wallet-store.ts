/**
 * Shared ticket-wallet snapshot so Dashboard, Connections, Routes, and Chat
 * do not each issue list_ticket_wallet on every mount.
 */
import type { Backend, TicketWallet } from '@/lib/backend/contracts';
import { logger } from '@/lib/logger';

const log = logger.scope('runtime:ticket-wallet');

export type TicketWalletLoadState = 'idle' | 'loading' | 'ready' | 'error';

export type TicketWalletSnapshot = {
  state: TicketWalletLoadState;
  wallet: TicketWallet | null;
  refreshing: boolean;
  error: unknown | null;
};

type Listener = () => void;

let snapshot: TicketWalletSnapshot = {
  state: 'idle',
  wallet: null,
  refreshing: false,
  error: null,
};

let inflight: Promise<TicketWalletSnapshot> | null = null;
let epoch = 0;
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of listeners) listener();
}

function setSnapshot(next: TicketWalletSnapshot): void {
  snapshot = next;
  emit();
}

function errorCode(error: unknown): string {
  if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string') {
    return error.code;
  }
  if (error instanceof Error && error.name) return error.name;
  return 'unknown';
}

function hasWalletData(state: TicketWalletLoadState, wallet: TicketWallet | null): boolean {
  return (state === 'ready' || state === 'error') && wallet != null;
}

export function getTicketWalletSnapshot(): TicketWalletSnapshot {
  return snapshot;
}

export function subscribeTicketWallet(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetTicketWalletStore(): void {
  epoch += 1;
  inflight = null;
  setSnapshot({
    state: 'idle',
    wallet: null,
    refreshing: false,
    error: null,
  });
}

export async function loadTicketWallet(
  backend: Backend,
  opts: { force?: boolean } = {},
): Promise<TicketWalletSnapshot> {
  if (!opts.force && snapshot.state === 'ready' && snapshot.wallet) return snapshot;
  if (inflight) {
    const active = inflight;
    const waitEpoch = epoch;
    try {
      await active;
    } catch (error) {
      if (!opts.force) throw error;
    }
    if (waitEpoch !== epoch) {
      if (!opts.force) return snapshot;
      return loadTicketWallet(backend, { force: true });
    }
    if (!opts.force) return snapshot;
    if (inflight && inflight !== active) {
      await inflight;
      return snapshot;
    }
    return loadTicketWallet(backend, { force: true });
  }

  const previousSnapshot = snapshot;
  const startedEpoch = epoch;
  const isBackgroundRefresh =
    opts.force === true && hasWalletData(previousSnapshot.state, previousSnapshot.wallet);

  if (isBackgroundRefresh) {
    setSnapshot({
      ...previousSnapshot,
      refreshing: true,
      error: null,
    });
  } else {
    setSnapshot({
      state: 'loading',
      wallet: previousSnapshot.wallet,
      refreshing: false,
      error: null,
    });
  }

  let request!: Promise<TicketWalletSnapshot>;
  request = (async () => {
    try {
      const wallet = await backend.ticket.listWallet();
      if (startedEpoch !== epoch) return snapshot;
      const next: TicketWalletSnapshot = {
        state: 'ready',
        wallet,
        refreshing: false,
        error: null,
      };
      setSnapshot(next);
      return next;
    } catch (error) {
      log.warn('ticket wallet load failed', { errorCode: errorCode(error) });
      if (startedEpoch !== epoch) return snapshot;
      const failed: TicketWalletSnapshot = {
        state: previousSnapshot.wallet ? previousSnapshot.state : 'error',
        wallet: previousSnapshot.wallet,
        refreshing: false,
        error,
      };
      setSnapshot(failed);
      if (!failed.wallet) throw error;
      return failed;
    } finally {
      if (inflight === request) inflight = null;
    }
  })();
  inflight = request;
  return request;
}

export function notifyTicketWalletChanged(backend: Backend): Promise<TicketWalletSnapshot> {
  return loadTicketWallet(backend, { force: true });
}
