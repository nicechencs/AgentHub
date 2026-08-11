import type { McpPort } from '@/lib/backend/contracts';
import type { McpInventory } from '@/lib/backend/contracts/mcp-types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:mcp');

export function createTauriMcpPort(): McpPort {
  return {
    async listInventory() {
      try {
        return await invoke<McpInventory>('list_mcp_inventory_cmd');
      } catch (e) {
        log.error('listInventory failed', e);
        throw e;
      }
    },
  };
}
