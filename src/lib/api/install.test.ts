import { describe, expect, it, vi } from 'vitest';

const installRuntimePort = vi.fn(async () => ({
  ok: true,
  action: 'env_install',
  logs: [],
  message: 'ok',
}));
const launchAgentProgramPort = vi.fn(async () => {});
const onProgressPort = vi.fn(() => () => {});

vi.mock('@/app/runtime', () => ({
  getBackend: () => ({
    install: {
      installRuntime: installRuntimePort,
      launchAgentProgram: launchAgentProgramPort,
      onProgress: onProgressPort,
    },
  }),
}));

import { installRuntime, launchAgentProgram, onInstallProgress } from './install';

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

describe('launchAgentProgram façade', () => {
  it('forwards agentId and kind, not a client path', async () => {
    launchAgentProgramPort.mockClear();
    await launchAgentProgram('codex', 'cli');
    expect(launchAgentProgramPort).toHaveBeenCalledWith('codex', 'cli');
    await launchAgentProgram('workbuddy', 'app');
    expect(launchAgentProgramPort).toHaveBeenCalledWith('workbuddy', 'app');
  });
});

describe('onInstallProgress façade', () => {
  it('forwards the handler to InstallPort.onProgress', async () => {
    const handler = vi.fn();
    onProgressPort.mockClear();
    const unsub = await onInstallProgress(handler);
    expect(onProgressPort).toHaveBeenCalledWith(handler);
    expect(typeof unsub).toBe('function');
  });
});
