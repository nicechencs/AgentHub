import type { PluginPort } from '@/lib/backend/contracts';
import type { PluginInventory } from '@/lib/backend/contracts/plugin-types';
import type { AgentId } from '@/lib/types';
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
    async enable(agent: AgentId, name: string, marketplace?: string | null) {
      try {
        await invoke<void>('enable_plugin_cmd', {
          agent,
          name,
          marketplace: marketplace ?? null,
        });
      } catch (e) {
        log.error('enable failed', e);
        throw e;
      }
    },
    async disable(agent: AgentId, name: string, marketplace?: string | null) {
      try {
        await invoke<void>('disable_plugin_cmd', {
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
