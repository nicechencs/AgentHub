import type { AgentCatalogPort } from '@/lib/backend/contracts';
import type { AgentCatalogEntryDto } from '@/lib/backend/contracts/agent-catalog-types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:catalog');

export function createTauriCatalogPort(): AgentCatalogPort {
  return {
    async listAgentCatalog() {
      try {
        return await invoke<AgentCatalogEntryDto[]>('list_agent_catalog');
      } catch (e) {
        log.error('list_agent_catalog failed', e);
        throw e;
      }
    },
    async getAgentCatalogEntry(key: string) {
      try {
        return await invoke<AgentCatalogEntryDto>('get_agent_catalog_entry', { key });
      } catch (e) {
        log.error('get_agent_catalog_entry failed', e);
        throw e;
      }
    },
  };
}
