/**
 * Agent list merge helpers + catalog port types.
 * Product agent set comes from runtime catalog, not a static closed list.
 */
import type { AgentMeta } from '@/config/agents';
import type { AgentKey, AgentStatus } from '@/lib/types';
import type { AgentCatalogEntryDto } from './agent-catalog-types';

export type { AgentCatalogEntryDto, LiveOccupancyDto } from './agent-catalog-types';
export {
  catalogOccupancy,
  isCatalogAppendOccupancy,
  isListOccupancy,
  mapCatalogCapabilities,
} from './agent-catalog-types';

function missingAgentStatus(id: AgentKey): AgentStatus {
  return {
    agentId: id,
    installed: false,
    authStatus: 'none',
    authLabel: '未配置',
    running: false,
    envReady: true,
  };
}

/**
 * Always return one row per catalog-driven agent meta (stable order).
 * When `catalog` is empty (not loaded / error), returns detected rows only.
 */
export function mergeAgentListWithCatalog(
  detected: AgentStatus[],
  catalog: readonly AgentMeta[] = [],
): AgentStatus[] {
  const byId = new Map(detected.map((a) => [a.agentId, a]));
  if (catalog.length === 0) {
    return detected.slice();
  }
  return catalog.map((meta) => {
    const found = byId.get(meta.id);
    if (found) {
      // Prefer doctor/detect capabilities; fill from catalog meta when missing.
      if (!found.capabilities && meta.capabilities) {
        return { ...found, capabilities: meta.capabilities };
      }
      return found;
    }
    const missing = missingAgentStatus(meta.id);
    if (meta.capabilities) {
      missing.capabilities = meta.capabilities;
    }
    return missing;
  });
}

export interface AgentCatalogPort {
  listAgentCatalog(): Promise<AgentCatalogEntryDto[]>;
  getAgentCatalogEntry(key: string): Promise<AgentCatalogEntryDto>;
}
