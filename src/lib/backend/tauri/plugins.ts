import type { PluginPort } from '@/lib/backend/contracts';
import type { PluginInventory } from '@/lib/backend/contracts/plugin-types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:plugins');

export function createTauriPluginPort(): PluginPort {
  return {
    async listInventory() {
      try {
        return await invoke<PluginInventory>('list_plugin_inventory_cmd');
      } catch (e) {
        log.error('listInventory failed', e);
        throw e;
      }
    },
  };
}
