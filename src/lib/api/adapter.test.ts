import { beforeEach, describe, expect, it, vi } from 'vitest';

const { applyPort, removePort, enrollPort, refreshRuntimeReadModels } = vi.hoisted(() => ({
  applyPort: vi.fn(),
  removePort: vi.fn(),
  enrollPort: vi.fn(),
  refreshRuntimeReadModels: vi.fn(),
}));

vi.mock('@/app/runtime', () => ({
  getBackend: () => ({
    adapter: {
      apply: applyPort,
      remove: removePort,
      enrollNativeToGateway: enrollPort,
    },
  }),
  refreshRuntimeReadModels,
}));

import { applyAdapter, enrollNativeToGateway, removeAdapter } from './adapter';

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
    models: ['connectionPool'],
  });
}

function expectBindRefresh(): void {
  expect(refreshRuntimeReadModels).toHaveBeenCalledOnce();
  expect(refreshRuntimeReadModels).toHaveBeenCalledWith(expect.anything(), {
    models: ['connectionPool', 'ticketWallet'],
  });
}

describe('adapter façade pool refresh', () => {
  beforeEach(() => {
    applyPort.mockReset();
    removePort.mockReset();
    enrollPort.mockReset();
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
