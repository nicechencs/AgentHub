import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createTauriPluginPort } from './plugins';

const invokeMock = vi.fn();
vi.mock('./invoke', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

beforeEach(() => invokeMock.mockReset());

describe('Tauri plugin port', () => {
  it('forwards the stable install source when removing a Pi extension', async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    const port = createTauriPluginPort();

    await port.uninstall('pi', 'team-tools', 'npm', 'npm:team-tools@1.4', {
      keepData: true,
    });

    expect(invokeMock).toHaveBeenCalledWith('uninstall_plugin', {
      agent: 'pi',
      name: 'team-tools',
      marketplace: 'npm',
      installSource: 'npm:team-tools@1.4',
      keepData: true,
    });
  });

  it('forwards null for an unavailable legacy install source', async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    const port = createTauriPluginPort();

    await port.uninstall('claude', 'demo', 'official', undefined, { keepData: false });

    expect(invokeMock).toHaveBeenCalledWith('uninstall_plugin', {
      agent: 'claude',
      name: 'demo',
      marketplace: 'official',
      installSource: null,
      keepData: false,
    });
  });
});
