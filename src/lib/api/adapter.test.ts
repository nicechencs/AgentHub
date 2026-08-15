import { beforeEach, describe, expect, it, vi } from 'vitest';

const { applyPort, removePort, notifyConnectionPoolChanged } = vi.hoisted(() => ({
  applyPort: vi.fn(),
  removePort: vi.fn(),
  notifyConnectionPoolChanged: vi.fn(),
}));

vi.mock('@/app/runtime', () => ({
  getBackend: () => ({
    adapter: {
      apply: applyPort,
      remove: removePort,
    },
  }),
  notifyConnectionPoolChanged,
}));

import { applyAdapter, removeAdapter } from './adapter';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe('adapter façade pool refresh', () => {
  beforeEach(() => {
    applyPort.mockReset();
    removePort.mockReset();
    notifyConnectionPoolChanged.mockReset();
  });

  it('does not resolve apply until the shared pool refresh finishes', async () => {
    const pool = deferred<void>();
    applyPort.mockResolvedValue({ profile: { id: 'profile-1' } });
    notifyConnectionPoolChanged.mockReturnValue(pool.promise);

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
    expect(notifyConnectionPoolChanged).toHaveBeenCalledOnce();

    pool.resolve();
    await expect(apply).resolves.toEqual({ profile: { id: 'profile-1' } });
    expect(settled).toBe(true);
  });

  it('does not resolve remove until the shared pool refresh finishes', async () => {
    const pool = deferred<void>();
    removePort.mockResolvedValue(undefined);
    notifyConnectionPoolChanged.mockReturnValue(pool.promise);

    let settled = false;
    const remove = removeAdapter('profile-1').then(() => {
      settled = true;
    });

    await Promise.resolve();
    expect(settled).toBe(false);

    pool.resolve();
    await remove;
    expect(settled).toBe(true);
  });

  it('still returns the mutation result when the follow-up pool refresh fails', async () => {
    applyPort.mockResolvedValue({ profile: { id: 'profile-1' } });
    notifyConnectionPoolChanged.mockRejectedValue(new Error('pool refresh failed'));

    await expect(applyAdapter({
      sourceKind: 'provider',
      sourceId: 'source-1',
      targetAgentId: 'codex',
    })).resolves.toEqual({ profile: { id: 'profile-1' } });
  });
});
