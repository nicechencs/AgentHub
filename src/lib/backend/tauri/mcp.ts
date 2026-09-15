import type { McpPort } from '@/lib/backend/contracts';
import type {
  McpCatalogEntry,
  McpInventory,
  McpProbeResult,
  McpServerSpec,
  McpWriteResult,
} from '@/lib/backend/contracts/mcp-types';
import type { AgentKey } from '@/lib/types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:mcp');

export function createTauriMcpPort(): McpPort {
  return {
    async listInventory() {
      try {
        return await invoke<McpInventory>('list_mcp_inventory');
      } catch (e) {
        log.error('listInventory failed', e);
        throw e;
      }
    },
    async listCatalog() {
      try {
        return await invoke<McpCatalogEntry[]>('list_mcp_catalog');
      } catch (e) {
        log.error('listCatalog failed', e);
        throw e;
      }
    },
    async probeServer(spec) {
      try {
        return await invoke<McpProbeResult>('probe_mcp_server', { spec });
      } catch (e) {
        log.error('probeServer failed', e);
        throw e;
      }
    },
    async upsertServer(agent, spec) {
      try {
        return await invoke<McpWriteResult>('upsert_mcp_server', { agent, spec });
      } catch (e) {
        log.error('upsertServer failed', e);
        throw e;
      }
    },
    async setServerEnabled(agent, name, enabled) {
      try {
        return await invoke<McpWriteResult>('set_mcp_server_enabled', {
          agent,
          name,
          enabled,
        });
      } catch (e) {
        log.error('setServerEnabled failed', e);
        throw e;
      }
    },
  };
}

// Keep AgentKey referenced for editors that tree-shake type-only imports oddly.
export type { AgentKey, McpServerSpec };
