/**
 * Shared connection-inventory state (accounts + providers).
 * Adapter, Connections, and badges subscribe here so route changes do not
 * turn one listAccounts/listProviders pass into several competing requests.
 *
 * This is NOT the product「连接池」(RoutePool). It caches all saved logins
 * and providers for UI reads.
 */
import type { AccountAuthView } from '@/lib/backend/contracts/account-map';
import { unwrapAccounts, wrapBareAccount } from '@/lib/backend/contracts/account-map';
import type { Account, AgentKey, Provider } from '@/lib/types';
import type { Backend } from '@/lib/backend/contracts';
import { logger } from '@/lib/logger';

const log = logger.scope('runtime:connection-inventory');

export type ConnectionInventoryLoadState = 'idle' | 'loading' | 'ready' | 'partial' | 'error';

export type ConnectionInventorySnapshot = {
  state: ConnectionInventoryLoadState;
  accounts: Account[];
  accountViews: AccountAuthView[];
  providers: Provider[];
  refreshing: boolean;
  errors: {
    accounts?: unknown;
    providers?: unknown;
  };
};

type Listener = () => void;

let snapshot: ConnectionInventorySnapshot = {
  state: 'idle',
  accounts: [],
  accountViews: [],
  providers: [],
  refreshing: false,
  errors: {},
};

function snapshotWithViews(
  views: AccountAuthView[],
  rest: Omit<ConnectionInventorySnapshot, 'accounts' | 'accountViews'>,
): ConnectionInventorySnapshot {
  return {
    ...rest,
    accountViews: views,
    accounts: unwrapAccounts(views),
  };
}

function normalizeAccountViews(rows: readonly AccountAuthView[] | readonly Account[]): AccountAuthView[] {
  return rows.map((row) =>
    'savedAuth' in row && 'account' in row ? row : wrapBareAccount(row),
  );
}

let inflight: Promise<ConnectionInventorySnapshot> | null = null;
let epoch = 0;
let mutationDepth = 0;
let notifyPending = false;
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of listeners) listener();
}

function setSnapshot(next: ConnectionInventorySnapshot): void {
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

function hasCompletedInventoryData(state: ConnectionInventoryLoadState): boolean {
  return state === 'ready' || state === 'partial';
}

export function getConnectionInventorySnapshot(): ConnectionInventorySnapshot {
  return snapshot;
}

export function subscribeConnectionInventory(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetConnectionInventoryStore(): void {
  epoch += 1;
  inflight = null;
  mutationDepth = 0;
  notifyPending = false;
  setSnapshot({
    state: 'idle',
    accounts: [],
    accountViews: [],
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
  agentId: AgentKey,
  source: 'account' | 'provider',
  id: string,
): void {
  const views = (snapshot.accountViews.length
    ? snapshot.accountViews
    : snapshot.accounts.map(wrapBareAccount)
  ).map((view) => ({
    ...view,
    account: {
      ...view.account,
      isCurrent: view.account.agentId === agentId
        ? source === 'account' && view.account.id === id
        : view.account.isCurrent,
    },
  }));
  setSnapshot(snapshotWithViews(views, {
    state: snapshot.state,
    providers: snapshot.providers.map((provider) => ({
      ...provider,
      isCurrent: provider.agentId === agentId
        ? source === 'provider' && provider.id === id
        : provider.isCurrent,
    })),
    refreshing: snapshot.refreshing,
    errors: snapshot.errors,
  }));
}

/** Collapse N façade notifies (e.g. delete-all) into one refresh. */
export function beginConnectionInventoryMutation(): void {
  mutationDepth += 1;
}

export function endConnectionInventoryMutation(
  backend: Backend,
): Promise<ConnectionInventorySnapshot> {
  mutationDepth = Math.max(0, mutationDepth - 1);
  if (mutationDepth > 0) return Promise.resolve(snapshot);
  if (!notifyPending) return Promise.resolve(snapshot);
  notifyPending = false;
  return loadConnectionInventory(backend, { force: true });
}

export function accountsForAgent(accounts: Account[], agentId: AgentKey): Account[] {
  return accounts.filter((account) => account.agentId === agentId);
}

export function providersForAgent(providers: Provider[], agentId: AgentKey): Provider[] {
  return providers.filter((provider) => provider.agentId === agentId);
}

export function connectionCountsByAgent(
  accounts: Account[],
  providers: Provider[],
  agentIds: readonly AgentKey[],
): Partial<Record<AgentKey, number>> {
  const counts: Partial<Record<AgentKey, number>> = {};
  for (const agentId of agentIds) {
    counts[agentId] =
      accountsForAgent(accounts, agentId).length + providersForAgent(providers, agentId).length;
  }
  return counts;
}

/**
 * 全量拉取 listAccounts() + listProviders()（不带 agentId）。
 */
export async function loadConnectionInventory(
  backend: Backend,
  opts: { force?: boolean } = {},
): Promise<ConnectionInventorySnapshot> {
  if (!opts.force && hasCompletedInventoryData(snapshot.state)) return snapshot;
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
      return loadConnectionInventory(backend, { force: true });
    }
    if (!opts.force) return snapshot;
    // A force request that arrived during an older fetch must not be lost.
    // If another waiter already started the follow-up fetch, join it.
    if (inflight && inflight !== active) {
      await inflight;
      return snapshot;
    }
    return loadConnectionInventory(backend, { force: true });
  }

  const previousSnapshot = snapshot;
  const startedEpoch = epoch;
  const isBackgroundRefresh = opts.force === true && hasCompletedInventoryData(previousSnapshot.state);

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
      accountViews: previousSnapshot.accountViews,
      providers: previousSnapshot.providers,
      refreshing: false,
      errors: {},
    });
  }

  let request!: Promise<ConnectionInventorySnapshot>;
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
        log.warn('connection inventory accounts load failed', { errorCode: errorCode(accountsError) });
      }
      if (!providersOk) {
        log.warn('connection inventory providers load failed', { errorCode: errorCode(providersError) });
      }

      if (!accountsOk && !providersOk) {
        log.error('connection inventory load failed', {
          accounts: errorCode(accountsError),
          providers: errorCode(providersError),
        });
        const failed: ConnectionInventorySnapshot = {
          state: hasCompletedInventoryData(previousSnapshot.state) ? previousSnapshot.state : 'error',
          accounts: previousSnapshot.accounts,
          accountViews: previousSnapshot.accountViews,
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

      const views = accountsOk
        ? normalizeAccountViews(accountsResult.value)
        : previousSnapshot.accountViews;
      const next = snapshotWithViews(views, {
        state: accountsOk && providersOk ? 'ready' : 'partial',
        providers: providersOk ? providersResult.value : previousSnapshot.providers,
        refreshing: false,
        errors: {
          ...(accountsError ? { accounts: accountsError } : {}),
          ...(providersError ? { providers: providersError } : {}),
        },
      });
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
export function notifyConnectionInventoryChanged(
  backend: Backend,
): Promise<ConnectionInventorySnapshot> {
  if (mutationDepth > 0) {
    notifyPending = true;
    return Promise.resolve(snapshot);
  }
  return loadConnectionInventory(backend, { force: true });
}

// ---------------------------------------------------------------------------
// Deprecated aliases (N-03) — prefer ConnectionInventory* names above.
// ---------------------------------------------------------------------------

/** @deprecated Use ConnectionInventoryLoadState */
export type ConnectionPoolLoadState = ConnectionInventoryLoadState;

/** @deprecated Use ConnectionInventorySnapshot */
export type ConnectionPoolSnapshot = ConnectionInventorySnapshot;

/** @deprecated Use getConnectionInventorySnapshot */
export const getConnectionPoolSnapshot = getConnectionInventorySnapshot;

/** @deprecated Use subscribeConnectionInventory */
export const subscribeConnectionPool = subscribeConnectionInventory;

/** @deprecated Use resetConnectionInventoryStore */
export const resetConnectionPoolStore = resetConnectionInventoryStore;

/** @deprecated Use beginConnectionInventoryMutation */
export const beginConnectionPoolMutation = beginConnectionInventoryMutation;

/** @deprecated Use endConnectionInventoryMutation */
export const endConnectionPoolMutation = endConnectionInventoryMutation;

/** @deprecated Use loadConnectionInventory */
export const loadConnectionPool = loadConnectionInventory;

/** @deprecated Use notifyConnectionInventoryChanged */
export const notifyConnectionPoolChanged = notifyConnectionInventoryChanged;
