import { beforeEach, describe, expect, it, vi } from 'vitest';

const {
  installDetailedPort,
  installPort,
  upgradeDetailedPort,
  upgradePort,
  uninstallDetailedPort,
  uninstallPort,
  refreshRuntimeReadModels,
} = vi.hoisted(() => ({
  installDetailedPort: vi.fn(),
  installPort: vi.fn(),
  upgradeDetailedPort: vi.fn(),
  upgradePort: vi.fn(),
  uninstallDetailedPort: vi.fn(),
  uninstallPort: vi.fn(),
  refreshRuntimeReadModels: vi.fn(),
}));

vi.mock('@/app/runtime', () => ({
  getBackend: () => ({
    agent: {
      installAgentDetailed: installDetailedPort,
      installAgent: installPort,
      upgradeAgentDetailed: upgradeDetailedPort,
      upgradeAgent: upgradePort,
      uninstallAgentDetailed: uninstallDetailedPort,
      uninstallAgent: uninstallPort,
    },
  }),
  refreshRuntimeReadModels,
}));

import {
  installAgent,
  installAgentDetailed,
  uninstallAgent,
  uninstallAgentDetailed,
  upgradeAgent,
  upgradeAgentDetailed,
} from './agent';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const outcome = { ok: true, action: 'install', logs: [], message: 'ok' };
const status = {
  agentId: 'claude' as const,
  installed: true,
  hidden: false,
  authStatus: 'none' as const,
  authLabel: '未配置',
  running: false,
};

function expectAgentStatusRefreshOnly(): void {
  expect(refreshRuntimeReadModels).toHaveBeenCalledOnce();
  expect(refreshRuntimeReadModels).toHaveBeenCalledWith(expect.anything(), {
    models: ['agentStatus'],
  });
}

describe('agent façade status refresh', () => {
  beforeEach(() => {
    installDetailedPort.mockReset();
    installPort.mockReset();
    upgradeDetailedPort.mockReset();
    upgradePort.mockReset();
    uninstallDetailedPort.mockReset();
    uninstallPort.mockReset();
    refreshRuntimeReadModels.mockReset();
  });

  it('does not resolve install until the shared status refresh finishes', async () => {
    const refresh = deferred<void>();
    installDetailedPort.mockResolvedValue(outcome);
    refreshRuntimeReadModels.mockReturnValue(refresh.promise);

    let settled = false;
    const install = installAgentDetailed('claude', 'npm').then((result) => {
      settled = true;
      return result;
    });

    await Promise.resolve();
    expect(settled).toBe(false);
    expectAgentStatusRefreshOnly();

    refresh.resolve();
    await expect(install).resolves.toEqual(outcome);
    expect(settled).toBe(true);
  });

  it('refreshes agent status after install, upgrade, and uninstall', async () => {
    refreshRuntimeReadModels.mockResolvedValue(undefined);
    installPort.mockResolvedValue(status);
    upgradePort.mockResolvedValue(status);
    upgradeDetailedPort.mockResolvedValue(outcome);
    uninstallPort.mockResolvedValue(undefined);
    uninstallDetailedPort.mockResolvedValue(outcome);

    await expect(installAgent('claude', 'npm')).resolves.toEqual(status);
    await expect(upgradeAgent('claude')).resolves.toEqual(status);
    await expect(upgradeAgentDetailed('claude')).resolves.toEqual(outcome);
    await expect(uninstallAgent('claude', false)).resolves.toBeUndefined();
    await expect(uninstallAgentDetailed('claude', true)).resolves.toEqual(outcome);

    expect(refreshRuntimeReadModels).toHaveBeenCalledTimes(5);
    for (const call of refreshRuntimeReadModels.mock.calls) {
      expect(call[1]).toEqual({ models: ['agentStatus'] });
    }
  });

  it('still returns the mutation result when the follow-up status refresh fails', async () => {
    installDetailedPort.mockResolvedValue(outcome);
    refreshRuntimeReadModels.mockRejectedValue(new Error('status refresh failed'));

    await expect(installAgentDetailed('claude', 'npm')).resolves.toEqual(outcome);
  });
});
