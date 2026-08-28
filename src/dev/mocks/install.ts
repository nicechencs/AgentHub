import type { InstallPort } from '@/lib/backend/contracts';
import { unavailableError } from '@/lib/backend/contracts/errors';
import { MOCK_INSTALL_CATALOG } from './fixtures/install-catalog';

export function createMockInstallPort(): InstallPort {
  const deny = (name: string) => {
    throw unavailableError(name, 'mock 模式不走 Tauri install command；请使用 env/agent mock 流程');
  };

  return {
    listInstallCatalog: async () => MOCK_INSTALL_CATALOG.map((e) => ({
      ...e,
      channels: e.channels.map((c) => ({ ...c, requires: [...c.requires] })),
    })),
    installRuntime: async () => deny('installRuntime'),
    installAgentCmd: async () => deny('installAgentCmd'),
    upgradeAgentCmd: async () => deny('upgradeAgentCmd'),
    uninstallAgentCmd: async () => deny('uninstallAgentCmd'),
    openAgentConfigDir: async () => deny('openAgentConfigDir'),
    getAgentLivePaths: async () => deny('getAgentLivePaths'),
    onProgress: () => () => {},
  };
}
