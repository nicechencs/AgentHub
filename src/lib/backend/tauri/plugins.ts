import type { PluginPort } from '@/lib/backend/contracts';
import type {
  PluginEntry,
  PluginInstallOptions,
  PluginInventory,
  PluginUninstallOptions,
} from '@/lib/backend/contracts/plugin-types';
import type { AgentKey } from '@/lib/types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:plugins');

export function createTauriPluginPort(): PluginPort {
  return {
    async listInventory() {
      try {
        return await invoke<PluginInventory>('list_plugin_inventory');
      } catch (e) {
        log.error('listInventory failed', e);
        throw e;
      }
    },
    async listAvailable(agent: AgentKey) {
      try {
        return await invoke<PluginEntry[]>('list_available_plugins', { agent });
      } catch (e) {
        log.error('listAvailable failed', e);
        throw e;
      }
    },
    async previewInstall(agent: AgentKey, source: string) {
      try {
        return await invoke<PluginEntry>('preview_plugin_install', { agent, source });
      } catch (e) {
        log.error('previewInstall failed', e);
        throw e;
      }
    },
    async install(agent: AgentKey, source: string, options: PluginInstallOptions) {
      try {
        await invoke<void>('install_plugin', {
          agent,
          source,
          confirmed: options.confirmed,
        });
      } catch (e) {
        log.error('install failed', e);
        throw e;
      }
    },
    async uninstall(
      agent: AgentKey,
      name: string,
      marketplace: string | null | undefined,
      options: PluginUninstallOptions,
    ) {
      try {
        await invoke<void>('uninstall_plugin', {
          agent,
          name,
          marketplace: marketplace ?? null,
          keepData: options.keepData,
        });
      } catch (e) {
        log.error('uninstall failed', e);
        throw e;
      }
    },
    async enable(agent: AgentKey, name: string, marketplace?: string | null) {
      try {
        await invoke<void>('enable_plugin', {
          agent,
          name,
          marketplace: marketplace ?? null,
        });
      } catch (e) {
        log.error('enable failed', e);
        throw e;
      }
    },
    async disable(agent: AgentKey, name: string, marketplace?: string | null) {
      try {
        await invoke<void>('disable_plugin', {
          agent,
          name,
          marketplace: marketplace ?? null,
        });
      } catch (e) {
        log.error('disable failed', e);
        throw e;
      }
    },
  };
}
