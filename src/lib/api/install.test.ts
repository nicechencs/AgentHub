import { describe, expect, it, vi } from 'vitest';

const installRuntimePort = vi.fn(async () => ({
  ok: true,
  action: 'env_install',
  logs: [],
  message: 'ok',
}));

vi.mock('@/app/runtime', () => ({
  getBackend: () => ({
    install: {
      installRuntime: installRuntimePort,
    },
  }),
}));

import { installRuntime } from './install';

describe('installRuntime façade', () => {
  it('omits channel so the host can pick brew on macOS or winget on Windows', async () => {
    installRuntimePort.mockClear();
    await installRuntime('nodejs');
    expect(installRuntimePort).toHaveBeenCalledWith('nodejs', undefined);
  });

  it('forwards an explicit channel when callers supply one', async () => {
    installRuntimePort.mockClear();
    await installRuntime('git', 'brew');
    expect(installRuntimePort).toHaveBeenCalledWith('git', 'brew');
  });
});
