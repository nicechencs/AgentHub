import { beforeEach, describe, expect, it, vi } from 'vitest';

const { restorePort, refreshRuntimeReadModels } = vi.hoisted(() => ({
  restorePort: vi.fn(),
  refreshRuntimeReadModels: vi.fn(),
}));

vi.mock('@/app/runtime', () => ({
  getBackend: () => ({
    trash: {
      restore: restorePort,
    },
  }),
  refreshRuntimeReadModels,
}));

import { restoreConnectionTrash } from './trash';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe('trash façade pool refresh', () => {
  beforeEach(() => {
    restorePort.mockReset();
    refreshRuntimeReadModels.mockReset();
  });

  it('does not resolve restore until the shared pool refresh finishes', async () => {
    const pool = deferred<void>();
    restorePort.mockResolvedValue(undefined);
    refreshRuntimeReadModels.mockReturnValue(pool.promise);

    let settled = false;
    const restore = restoreConnectionTrash('trash-1').then(() => {
      settled = true;
    });

    await Promise.resolve();
    expect(settled).toBe(false);
    expect(refreshRuntimeReadModels).toHaveBeenCalledOnce();
    expect(refreshRuntimeReadModels).toHaveBeenCalledWith(expect.anything(), {
      models: ['connectionPool'],
    });

    pool.resolve();
    await restore;
    expect(settled).toBe(true);
  });

  it('still returns after restore when the follow-up pool refresh fails', async () => {
    restorePort.mockResolvedValue(undefined);
    refreshRuntimeReadModels.mockRejectedValue(new Error('pool refresh failed'));

    await expect(restoreConnectionTrash('trash-1')).resolves.toBeUndefined();
  });
});
