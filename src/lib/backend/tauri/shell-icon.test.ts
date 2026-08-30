import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';
import { invokeSetShellIcon } from './shell-icon';

const invokeMock = vi.fn();
let tauriRuntime = false;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  isTauri: () => tauriRuntime,
}));

describe('shell icon invoke', () => {
  beforeEach(() => {
    tauriRuntime = false;
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('is fail-closed outside Tauri', async () => {
    await expect(invokeSetShellIcon([0, 0, 0, 255], 1, 1, 'indigo')).rejects.toBeInstanceOf(
      BackendUnavailableError,
    );
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('forwards rgba to set_shell_icon when in Tauri', async () => {
    tauriRuntime = true;
    invokeMock.mockResolvedValueOnce(undefined);
    const rgba = [79, 70, 229, 255];
    await invokeSetShellIcon(rgba, 128, 128, 'blue');
    expect(invokeMock).toHaveBeenCalledWith('set_shell_icon', {
      rgba,
      width: 128,
      height: 128,
      accentId: 'blue',
    });
  });
});
