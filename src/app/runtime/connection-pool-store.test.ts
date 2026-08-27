import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Backend } from '@/lib/backend/contracts';
import { wrapBareAccount } from '@/lib/backend/contracts/account-map';
import type { Account, Provider } from '@/lib/types';
import {
  accountsForAgent,
  beginConnectionPoolMutation,
  connectionCountsByAgent,
  endConnectionPoolMutation,
  getConnectionPoolSnapshot,
  loadConnectionPool,
  markConnectionCurrent,
  notifyConnectionPoolChanged,
  providersForAgent,
  resetConnectionPoolStore,
} from './connection-pool-store';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function account(agentId: string, id: string): Account {
  return {
    id,
    agentId,
    kind: 'oauth',
    label: id,
    isCurrent: false,
    tokenValid: true,
  };
}

function provider(agentId: string, id: string): Provider {
  return {
    id,
    agentId,
    name: id,
    preset: 'custom',
    configText: '{}',
    configFormat: 'json',
    isCurrent: false,
  };
}

function poolBackend(opts: {
  listAccounts: ReturnType<typeof vi.fn>;
  listProviders: ReturnType<typeof vi.fn>;
}): Backend {
  return {
    account: { listAccounts: opts.listAccounts },
    provider: { listProviders: opts.listProviders },
  } as unknown as Backend;
}

describe('connection-pool-store', () => {
  beforeEach(() => resetConnectionPoolStore());

  it('reuses a completed snapshot instead of force-refreshing on a later non-force load', async () => {
    const listAccounts = vi.fn(async () => [wrapBareAccount(account('claude', 'acc-1'))]);
    const listProviders = vi.fn(async () => [provider('claude', 'prov-1')]);
    const backend = poolBackend({ listAccounts, listProviders });

    const first = await loadConnectionPool(backend);
    const second = await loadConnectionPool(backend);

    expect(listAccounts).toHaveBeenCalledOnce();
    expect(listProviders).toHaveBeenCalledOnce();
    expect(second).toBe(first);
  });

  it('deduplicates concurrent loads so the backend is called once', async () => {
    const listAccounts = vi.fn(async () => [wrapBareAccount(account('claude', 'acc-1'))]);
    const listProviders = vi.fn(async () => [provider('claude', 'prov-1')]);
    const backend = poolBackend({ listAccounts, listProviders });

    const [first, second] = await Promise.all([
      loadConnectionPool(backend),
      loadConnectionPool(backend),
    ]);

    expect(listAccounts).toHaveBeenCalledOnce();
    expect(listProviders).toHaveBeenCalledOnce();
    expect(listAccounts).toHaveBeenCalledWith();
    expect(listProviders).toHaveBeenCalledWith();
    expect(first.state).toBe('ready');
    expect(second.accounts).toEqual(first.accounts);
    expect(second.providers).toEqual(first.providers);
  });

  it('keeps the successful side and marks partial when only one pool fails', async () => {
    const accountsError = new Error('accounts down');
    const providers = [provider('codex', 'prov-1')];
    const backend = poolBackend({
      listAccounts: vi.fn(async () => {
        throw accountsError;
      }),
      listProviders: vi.fn(async () => providers),
    });

    const loaded = await loadConnectionPool(backend);

    expect(loaded.state).toBe('partial');
    expect(loaded.accounts).toEqual([]);
    expect(loaded.providers).toEqual(providers);
    expect(loaded.errors.accounts).toBe(accountsError);
    expect(loaded.errors.providers).toBeUndefined();
    expect(loaded.refreshing).toBe(false);
  });

  it('marks error when both pools fail without clearing unrelated later data', async () => {
    const accountsError = new Error('accounts down');
    const providersError = new Error('providers down');
    const backend = poolBackend({
      listAccounts: vi.fn(async () => {
        throw accountsError;
      }),
      listProviders: vi.fn(async () => {
        throw providersError;
      }),
    });

    const loaded = await loadConnectionPool(backend);
    expect(loaded.state).toBe('error');
    expect(loaded.accounts).toEqual([]);
    expect(loaded.providers).toEqual([]);
    expect(loaded.errors.accounts).toBe(accountsError);
    expect(loaded.errors.providers).toBe(providersError);
  });

  it('keeps previous accounts when a forced refresh only fails that side', async () => {
    const previousAccounts = [account('claude', 'acc-1')];
    const previousProviders = [provider('claude', 'prov-1')];
    const nextProviders = [provider('claude', 'prov-2')];
    const refreshError = new Error('accounts refresh failed');
    const listAccounts = vi
      .fn()
      .mockResolvedValueOnce(previousAccounts)
      .mockRejectedValueOnce(refreshError);
    const listProviders = vi
      .fn()
      .mockResolvedValueOnce(previousProviders)
      .mockResolvedValueOnce(nextProviders);
    const backend = poolBackend({ listAccounts, listProviders });

    await loadConnectionPool(backend);
    const refreshed = await loadConnectionPool(backend, { force: true });

    expect(refreshed.state).toBe('partial');
    expect(refreshed.accounts).toEqual(previousAccounts);
    expect(refreshed.providers).toEqual(nextProviders);
    expect(refreshed.errors.accounts).toBe(refreshError);
    expect(refreshed.errors.providers).toBeUndefined();
  });

  it('keeps previous data and exposes errors when a forced refresh fails on both sides', async () => {
    const previousAccounts = [account('claude', 'acc-1')];
    const previousProviders = [provider('claude', 'prov-1')];
    const accountsError = new Error('accounts refresh failed');
    const providersError = new Error('providers refresh failed');
    const listAccounts = vi
      .fn()
      .mockResolvedValueOnce(previousAccounts)
      .mockRejectedValueOnce(accountsError);
    const listProviders = vi
      .fn()
      .mockResolvedValueOnce(previousProviders)
      .mockRejectedValueOnce(providersError);
    const backend = poolBackend({ listAccounts, listProviders });

    const ready = await loadConnectionPool(backend);
    const refresh = loadConnectionPool(backend, { force: true });
    expect(getConnectionPoolSnapshot()).toMatchObject({
      state: 'ready',
      refreshing: true,
      accounts: previousAccounts,
      providers: previousProviders,
    });

    const failed = await refresh;
    expect(failed).not.toEqual(ready);
    expect(failed).toMatchObject({
      state: 'ready',
      refreshing: false,
      accounts: previousAccounts,
      providers: previousProviders,
    });
    expect(failed.errors.accounts).toBe(accountsError);
    expect(failed.errors.providers).toBe(providersError);
    expect(getConnectionPoolSnapshot()).toEqual(failed);
  });

  it('keeps the ready snapshot renderable while a forced refresh is pending', async () => {
    const nextAccounts = deferred<Account[]>();
    const nextProviders = deferred<Provider[]>();
    const listAccounts = vi
      .fn()
      .mockResolvedValueOnce([account('claude', 'acc-1')])
      .mockImplementationOnce(() => nextAccounts.promise);
    const listProviders = vi
      .fn()
      .mockResolvedValueOnce([provider('claude', 'prov-1')])
      .mockImplementationOnce(() => nextProviders.promise);
    const backend = poolBackend({ listAccounts, listProviders });

    const ready = await loadConnectionPool(backend);
    expect(ready.state).toBe('ready');

    const refresh = loadConnectionPool(backend, { force: true });
    const pending = getConnectionPoolSnapshot();
    expect(pending).toMatchObject({ state: 'ready', refreshing: true });
    expect(pending.accounts).toEqual(ready.accounts);
    expect(pending.providers).toEqual(ready.providers);

    nextAccounts.resolve([account('codex', 'acc-2')]);
    nextProviders.resolve([provider('codex', 'prov-2')]);
    const refreshed = await refresh;
    expect(refreshed).toMatchObject({ state: 'ready', refreshing: false });
    expect(refreshed.accounts).toEqual([account('codex', 'acc-2')]);
    expect(refreshed.providers).toEqual([provider('codex', 'prov-2')]);
  });

  it('runs a fresh request when a forced refresh arrives during an in-flight request', async () => {
    const firstAccounts = deferred<Account[]>();
    const firstProviders = deferred<Provider[]>();
    const listAccounts = vi
      .fn()
      .mockImplementationOnce(() => firstAccounts.promise)
      .mockResolvedValueOnce([account('codex', 'acc-2')]);
    const listProviders = vi
      .fn()
      .mockImplementationOnce(() => firstProviders.promise)
      .mockResolvedValueOnce([provider('codex', 'prov-2')]);
    const backend = poolBackend({ listAccounts, listProviders });

    const initial = loadConnectionPool(backend);
    await Promise.resolve();
    expect(getConnectionPoolSnapshot().state).toBe('loading');
    const forced = loadConnectionPool(backend, { force: true });

    firstAccounts.resolve([account('claude', 'acc-1')]);
    firstProviders.resolve([provider('claude', 'prov-1')]);
    await initial;
    const refreshed = await forced;

    expect(listAccounts).toHaveBeenCalledTimes(2);
    expect(listProviders).toHaveBeenCalledTimes(2);
    expect(refreshed.accounts[0]?.id).toBe('acc-2');
    expect(refreshed.providers[0]?.id).toBe('prov-2');
  });

  it('notifyConnectionPoolChanged force-refreshes the pool', async () => {
    const listAccounts = vi
      .fn()
      .mockResolvedValueOnce([account('claude', 'acc-1')])
      .mockResolvedValueOnce([account('kimi', 'acc-2')]);
    const listProviders = vi
      .fn()
      .mockResolvedValueOnce([provider('claude', 'prov-1')])
      .mockResolvedValueOnce([provider('kimi', 'prov-2')]);
    const backend = poolBackend({ listAccounts, listProviders });

    await loadConnectionPool(backend);
    const notified = await notifyConnectionPoolChanged(backend);

    expect(listAccounts).toHaveBeenCalledTimes(2);
    expect(notified.accounts[0]?.agentId).toBe('kimi');
    expect(notified.providers[0]?.agentId).toBe('kimi');
  });

  it('marks the current connection before a refresh so the list does not flash the old row', async () => {
    const listAccounts = vi
      .fn()
      .mockResolvedValueOnce([
        { ...account('claude', 'acc-1'), isCurrent: true },
        account('claude', 'acc-2'),
      ])
      .mockImplementationOnce(() => new Promise<Account[]>(() => undefined));
    const listProviders = vi
      .fn()
      .mockResolvedValueOnce([provider('claude', 'prov-1')])
      .mockImplementationOnce(() => new Promise<Provider[]>(() => undefined));
    const backend = poolBackend({ listAccounts, listProviders });

    await loadConnectionPool(backend);
    markConnectionCurrent('claude', 'account', 'acc-2');
    void notifyConnectionPoolChanged(backend);

    const pending = getConnectionPoolSnapshot();
    expect(pending.refreshing).toBe(true);
    expect(pending.accounts.find((row) => row.id === 'acc-1')?.isCurrent).toBe(false);
    expect(pending.accounts.find((row) => row.id === 'acc-2')?.isCurrent).toBe(true);
    expect(pending.providers.every((row) => !row.isCurrent)).toBe(true);
  });

  it('does not move another agent current flag when marking a connection', async () => {
    const backend = poolBackend({
      listAccounts: vi.fn(async () => [
        { ...account('claude', 'acc-1'), isCurrent: true },
        { ...account('codex', 'acc-2'), isCurrent: true },
      ]),
      listProviders: vi.fn(async () => [
        { ...provider('claude', 'prov-1'), isCurrent: false },
        { ...provider('codex', 'prov-2'), isCurrent: true },
      ]),
    });

    await loadConnectionPool(backend);
    markConnectionCurrent('claude', 'provider', 'prov-1');

    const next = getConnectionPoolSnapshot();
    expect(next.accounts.find((row) => row.id === 'acc-1')?.isCurrent).toBe(false);
    expect(next.accounts.find((row) => row.id === 'acc-2')?.isCurrent).toBe(true);
    expect(next.providers.find((row) => row.id === 'prov-1')?.isCurrent).toBe(true);
    expect(next.providers.find((row) => row.id === 'prov-2')?.isCurrent).toBe(true);
  });

  it('collapses nested mutation notifies into one refresh', async () => {
    const listAccounts = vi
      .fn()
      .mockResolvedValueOnce([account('claude', 'acc-1')])
      .mockResolvedValueOnce([account('claude', 'acc-2')]);
    const listProviders = vi
      .fn()
      .mockResolvedValueOnce([provider('claude', 'prov-1')])
      .mockResolvedValueOnce([provider('claude', 'prov-2')]);
    const backend = poolBackend({ listAccounts, listProviders });

    await loadConnectionPool(backend);
    beginConnectionPoolMutation();
    await notifyConnectionPoolChanged(backend);
    await notifyConnectionPoolChanged(backend);
    expect(listAccounts).toHaveBeenCalledTimes(1);
    expect(getConnectionPoolSnapshot().accounts[0]?.id).toBe('acc-1');

    await endConnectionPoolMutation(backend);
    expect(listAccounts).toHaveBeenCalledTimes(2);
    expect(getConnectionPoolSnapshot().accounts[0]?.id).toBe('acc-2');
  });

  it('does not let a pre-reset response overwrite the new store or clear a newer inflight', async () => {
    const staleAccounts = deferred<Account[]>();
    const staleProviders = deferred<Provider[]>();
    const nextAccounts = deferred<Account[]>();
    const nextProviders = deferred<Provider[]>();
    const firstBackend = poolBackend({
      listAccounts: vi.fn(() => staleAccounts.promise),
      listProviders: vi.fn(() => staleProviders.promise),
    });
    const secondBackend = poolBackend({
      listAccounts: vi.fn(() => nextAccounts.promise),
      listProviders: vi.fn(() => nextProviders.promise),
    });

    const stale = loadConnectionPool(firstBackend);
    await Promise.resolve();
    resetConnectionPoolStore();
    const next = loadConnectionPool(secondBackend);
    expect(getConnectionPoolSnapshot().state).toBe('loading');

    staleAccounts.resolve([account('claude', 'stale-acc')]);
    staleProviders.resolve([provider('claude', 'stale-prov')]);
    await stale;

    expect(getConnectionPoolSnapshot()).toMatchObject({
      state: 'loading',
      accounts: [],
      providers: [],
    });

    nextAccounts.resolve([account('codex', 'fresh-acc')]);
    nextProviders.resolve([provider('codex', 'fresh-prov')]);
    const loaded = await next;

    expect(loaded).toMatchObject({
      state: 'ready',
      accounts: [account('codex', 'fresh-acc')],
      providers: [provider('codex', 'fresh-prov')],
    });
    expect(getConnectionPoolSnapshot()).toEqual(loaded);
  });

  it('filters accounts and providers by agent and counts both together', () => {
    const accounts = [account('claude', 'acc-1'), account('codex', 'acc-2'), account('claude', 'acc-3')];
    const providers = [provider('claude', 'prov-1'), provider('kimi', 'prov-2')];

    expect(accountsForAgent(accounts, 'claude').map((row) => row.id)).toEqual(['acc-1', 'acc-3']);
    expect(providersForAgent(providers, 'claude').map((row) => row.id)).toEqual(['prov-1']);
    expect(connectionCountsByAgent(accounts, providers, ['claude', 'codex', 'kimi', 'grok'])).toEqual({
      claude: 3,
      codex: 1,
      kimi: 1,
      grok: 0,
    });
  });
});
