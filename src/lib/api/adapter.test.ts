import { beforeEach, describe, expect, it, vi } from 'vitest';

const { applyPort, removePort, enrollPort, attachPort, forkPort, syncPort, refreshRuntimeReadModels } = vi.hoisted(() => ({
  applyPort: vi.fn(),
  removePort: vi.fn(),
  enrollPort: vi.fn(),
  attachPort: vi.fn(),
  forkPort: vi.fn(),
  syncPort: vi.fn(),
  refreshRuntimeReadModels: vi.fn(),
}));

vi.mock('@/app/runtime', () => ({
  getBackend: () => ({
    adapter: {
      apply: applyPort,
      remove: removePort,
      enrollNativeToGateway: enrollPort,
      attachPoolOwnedAuthorization: attachPort,
      forkConnectionAuthorization: forkPort,
      syncConnectionAuthorizations: syncPort,
    },
  }),
  refreshRuntimeReadModels,
}));

import { applyAdapter, attachPoolOwnedAuthorization, enrollNativeToGateway, forkConnectionAuthorization, removeAdapter, syncConnectionAuthorizations } from './adapter';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function expectPoolRefreshOnly(): void {
  expect(refreshRuntimeReadModels).toHaveBeenCalledOnce();
  expect(refreshRuntimeReadModels).toHaveBeenCalledWith(expect.anything(), {
    models: ['connectionInventory'],
  });
}

function expectBindRefresh(): void {
  expect(refreshRuntimeReadModels).toHaveBeenCalledOnce();
  expect(refreshRuntimeReadModels).toHaveBeenCalledWith(expect.anything(), {
    models: ['connectionInventory', 'ticketWallet'],
  });
}

describe('adapter façade pool refresh', () => {
  beforeEach(() => {
    applyPort.mockReset();
    removePort.mockReset();
    enrollPort.mockReset();
    attachPort.mockReset();
    syncPort.mockReset();
    refreshRuntimeReadModels.mockReset();
  });

  it('does not resolve apply until the shared pool refresh finishes', async () => {
    const pool = deferred<void>();
    applyPort.mockResolvedValue({ profile: { id: 'profile-1' } });
    refreshRuntimeReadModels.mockReturnValue(pool.promise);

    let settled = false;
    const apply = applyAdapter({
      sourceKind: 'provider',
      sourceId: 'source-1',
      targetAgentId: 'codex',
    }).then((result) => {
      settled = true;
      return result;
    });

    await Promise.resolve();
    expect(settled).toBe(false);
    expectBindRefresh();

    pool.resolve();
    await expect(apply).resolves.toEqual({ profile: { id: 'profile-1' } });
    expect(settled).toBe(true);
  });

  it('does not resolve remove until the shared pool refresh finishes', async () => {
    const pool = deferred<void>();
    removePort.mockResolvedValue(undefined);
    refreshRuntimeReadModels.mockReturnValue(pool.promise);

    let settled = false;
    const remove = removeAdapter('profile-1').then(() => {
      settled = true;
    });

    await Promise.resolve();
    expect(settled).toBe(false);

    pool.resolve();
    await remove;
    expect(settled).toBe(true);
    expectPoolRefreshOnly();
  });

  it('does not resolve enroll until the shared pool refresh finishes', async () => {
    const pool = deferred<void>();
    enrollPort.mockResolvedValue({ enabled: true, pools: [] });
    refreshRuntimeReadModels.mockReturnValue(pool.promise);

    let settled = false;
    const enroll = enrollNativeToGateway('profile-1').then((result) => {
      settled = true;
      return result;
    });

    await Promise.resolve();
    expect(settled).toBe(false);

    pool.resolve();
    await expect(enroll).resolves.toEqual({ enabled: true, pools: [] });
    expect(settled).toBe(true);
    expectPoolRefreshOnly();
  });

  it('refreshes connection and ticket lists after attaching a pool-owned authorization', async () => {
    attachPort.mockResolvedValue({ id: 'pool-1', members: [] });
    refreshRuntimeReadModels.mockResolvedValue(undefined);
    await expect(attachPoolOwnedAuthorization({
      sourceKind: 'provider',
      sourceId: 'codex-api',
      targetAgentId: 'codex',
      surface: 'responses',
    })).resolves.toEqual({ id: 'pool-1', members: [] });
    expectBindRefresh();
  });

  it('refreshes connection and ticket lists after copying an official login', async () => {
    forkPort.mockResolvedValue({
      sourceKind: 'account',
      sourceId: 'grok-copy',
      originalSourceId: 'grok-1',
      copied: true,
    });
    refreshRuntimeReadModels.mockResolvedValue(undefined);
    await expect(forkConnectionAuthorization('account', 'grok-1')).resolves.toEqual({
      sourceKind: 'account',
      sourceId: 'grok-copy',
      originalSourceId: 'grok-1',
      copied: true,
    });
    expectBindRefresh();
  });

  it('refreshes connection and ticket lists after syncing from Connections', async () => {
    syncPort.mockResolvedValue({ added: 2, skipped: 1 });
    refreshRuntimeReadModels.mockResolvedValue(undefined);
    await expect(syncConnectionAuthorizations()).resolves.toEqual({ added: 2, skipped: 1 });
    expectBindRefresh();
  });

  it('still returns the mutation result when the follow-up pool refresh fails', async () => {
    applyPort.mockResolvedValue({ profile: { id: 'profile-1' } });
    refreshRuntimeReadModels.mockRejectedValue(new Error('pool refresh failed'));

    await expect(applyAdapter({
      sourceKind: 'provider',
      sourceId: 'source-1',
      targetAgentId: 'codex',
    })).resolves.toEqual({ profile: { id: 'profile-1' } });
  });
});
