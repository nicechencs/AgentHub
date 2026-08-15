/**
 * Shared connection-pool state (accounts + providers).
 * Adapter, Connections, and badges subscribe here so route changes do not
 * turn one listAccounts/listProviders pass into several competing requests.
 */
import type { Account, AgentId, Provider } from '@/lib/types';
import type { Backend } from '@/lib/backend/contracts';
import { logger } from '@/lib/logger';

const log = logger.scope('runtime:connection-pool');

export type ConnectionPoolLoadState = 'idle' | 'loading' | 'ready' | 'partial' | 'error';

export type ConnectionPoolSnapshot = {
  state: ConnectionPoolLoadState;
  accounts: Account[];
  providers: Provider[];
  refreshing: boolean;
  errors: {
    accounts?: unknown;
    providers?: unknown;
  };
};

type Listener = () => void;

let snapshot: ConnectionPoolSnapshot = {
  state: 'idle',
  accounts: [],
  providers: [],
  refreshing: false,
  errors: {},
};

let inflight: Promise<ConnectionPoolSnapshot> | null = null;
let epoch = 0;
let mutationDepth = 0;
let notifyPending = false;
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of listeners) listener();
}

function setSnapshot(next: ConnectionPoolSnapshot): void {
  snapshot = next;
  emit();
}

function isFulfilled<T>(result: PromiseSettledResult<T>): result is PromiseFulfilledResult<T> {
  return result.status === 'fulfilled';
}

function errorCode(error: unknown): string {
  if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string') {
    return error.code;
  }
  if (error instanceof Error && error.name) return error.name;
  return 'unknown';
}

function hasCompletedPoolData(state: ConnectionPoolLoadState): boolean {
  return state === 'ready' || state === 'partial';
}

export function getConnectionPoolSnapshot(): ConnectionPoolSnapshot {
  return snapshot;
}

export function subscribeConnectionPool(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetConnectionPoolStore(): void {
  epoch += 1;
  inflight = null;
  mutationDepth = 0;
  notifyPending = false;
  setSnapshot({
    state: 'idle',
    accounts: [],
    providers: [],
    refreshing: false,
    errors: {},
  });
}

/**
 * Apply a local isCurrent change so the list and header stay aligned while
 * the follow-up listAccounts/listProviders refresh is still in flight.
 */
export function markConnectionCurrent(
  agentId: AgentId,
  source: 'account' | 'provider',
  id: string,
): void {
  setSnapshot({
    ...snapshot,
    accounts: snapshot.accounts.map((account) => ({
      ...account,
      isCurrent: account.agentId === agentId
        ? source === 'account' && account.id === id
        : account.isCurrent,
    })),
    providers: snapshot.providers.map((provider) => ({
      ...provider,
      isCurrent: provider.agentId === agentId
        ? source === 'provider' && provider.id === id
        : provider.isCurrent,
    })),
  });
}

/** Collapse N façade notifies (e.g. delete-all) into one refresh. */
export function beginConnectionPoolMutation(): void {
  mutationDepth += 1;
}

export function endConnectionPoolMutation(
  backend: Backend,
): Promise<ConnectionPoolSnapshot> {
  mutationDepth = Math.max(0, mutationDepth - 1);
  if (mutationDepth > 0) return Promise.resolve(snapshot);
  if (!notifyPending) return Promise.resolve(snapshot);
  notifyPending = false;
  return loadConnectionPool(backend, { force: true });
}

export function accountsForAgent(accounts: Account[], agentId: AgentId): Account[] {
  return accounts.filter((account) => account.agentId === agentId);
}

export function providersForAgent(providers: Provider[], agentId: AgentId): Provider[] {
  return providers.filter((provider) => provider.agentId === agentId);
}

export function connectionCountsByAgent(
  accounts: Account[],
  providers: Provider[],
  agentIds: readonly AgentId[],
): Partial<Record<AgentId, number>> {
  const counts: Partial<Record<AgentId, number>> = {};
  for (const agentId of agentIds) {
    counts[agentId] =
      accountsForAgent(accounts, agentId).length + providersForAgent(providers, agentId).length;
  }
  return counts;
}

/**
 * 全量拉取 listAccounts() + listProviders()（不带 agentId）。
 */
export async function loadConnectionPool(
  backend: Backend,
  opts: { force?: boolean } = {},
): Promise<ConnectionPoolSnapshot> {
  if (!opts.force && hasCompletedPoolData(snapshot.state)) return snapshot;
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
      return loadConnectionPool(backend, { force: true });
    }
    if (!opts.force) return snapshot;
    // A force request that arrived during an older fetch must not be lost.
    // If another waiter already started the follow-up fetch, join it.
    if (inflight && inflight !== active) {
      await inflight;
      return snapshot;
    }
    return loadConnectionPool(backend, { force: true });
  }

  const previousSnapshot = snapshot;
  const startedEpoch = epoch;
  const isBackgroundRefresh = opts.force === true && hasCompletedPoolData(previousSnapshot.state);

  if (isBackgroundRefresh) {
    setSnapshot({
      ...previousSnapshot,
      refreshing: true,
      errors: {},
    });
  } else {
    setSnapshot({
      state: 'loading',
      accounts: previousSnapshot.accounts,
      providers: previousSnapshot.providers,
      refreshing: false,
      errors: {},
    });
  }

  let request!: Promise<ConnectionPoolSnapshot>;
  request = (async () => {
    try {
      const [accountsResult, providersResult] = await Promise.allSettled([
        Promise.resolve().then(() => backend.account.listAccounts()),
        Promise.resolve().then(() => backend.provider.listProviders()),
      ]);

      if (startedEpoch !== epoch) return snapshot;

      const accountsOk = isFulfilled(accountsResult);
      const providersOk = isFulfilled(providersResult);
      const accountsError = accountsOk ? undefined : accountsResult.reason;
      const providersError = providersOk ? undefined : providersResult.reason;

      if (!accountsOk) {
        log.warn('connection pool accounts load failed', { errorCode: errorCode(accountsError) });
      }
      if (!providersOk) {
        log.warn('connection pool providers load failed', { errorCode: errorCode(providersError) });
      }

      if (!accountsOk && !providersOk) {
        log.error('connection pool load failed', {
          accounts: errorCode(accountsError),
          providers: errorCode(providersError),
        });
        const failed: ConnectionPoolSnapshot = {
          state: hasCompletedPoolData(previousSnapshot.state) ? previousSnapshot.state : 'error',
          accounts: previousSnapshot.accounts,
          providers: previousSnapshot.providers,
          refreshing: false,
          errors: {
            accounts: accountsError,
            providers: providersError,
          },
        };
        setSnapshot(failed);
        return failed;
      }

      const next: ConnectionPoolSnapshot = {
        state: accountsOk && providersOk ? 'ready' : 'partial',
        accounts: accountsOk ? accountsResult.value : previousSnapshot.accounts,
        providers: providersOk ? providersResult.value : previousSnapshot.providers,
        refreshing: false,
        errors: {
          ...(accountsError ? { accounts: accountsError } : {}),
          ...(providersError ? { providers: providersError } : {}),
        },
      };
      setSnapshot(next);
      return next;
    } finally {
      if (inflight === request) inflight = null;
    }
  })();
  inflight = request;

  return request;
}

/** 失效并强制刷新。Adapter apply/remove、Connections switch/delete/import 之后调用。 */
export function notifyConnectionPoolChanged(
  backend: Backend,
): Promise<ConnectionPoolSnapshot> {
  if (mutationDepth > 0) {
    notifyPending = true;
    return Promise.resolve(snapshot);
  }
  return loadConnectionPool(backend, { force: true });
}
