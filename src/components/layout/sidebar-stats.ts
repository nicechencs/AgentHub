import { installedCatalogAgents } from '@/components/layout/sidebar-agents';
import {
  applyStoredAgentOrder,
  hiddenAgentIdSet,
  visibleInstalledIds,
} from '@/lib/agent-visibility';
import type { AgentId, AgentStatus } from '@/lib/types';

type StatusRow = Pick<AgentStatus, 'agentId' | 'installed' | 'hidden'>;
type UpdateRow = Pick<AgentStatus, 'installed' | 'version' | 'latestVersion'>;

/** Catalog update pin: installed and latestVersion differs from version. */
export function agentHasCatalogUpdate(
  status: UpdateRow | null | undefined,
): boolean {
  return Boolean(
    status?.installed &&
      status.latestVersion &&
      status.version !== status.latestVersion,
  );
}

export type SidebarInstallStats<T extends { id: string }> = {
  hiddenIds: Set<AgentId>;
  installedCount: number;
  visibleTotal: number;
  orderedInstalledMetas: T[];
};

/**
 * Chrome projection of catalog install counts.
 * `visibleTotal` is unhidden catalog rows, not the installed count.
 */
export function sidebarInstallStats<T extends { id: string }>(
  catalog: readonly T[],
  statuses: readonly StatusRow[],
  storedOrder: readonly string[] = [],
): SidebarInstallStats<T> {
  const hiddenIds = hiddenAgentIdSet(statuses);
  const orderedInstalledMetas = applyStoredAgentOrder(
    installedCatalogAgents(catalog, statuses),
    (meta) => meta.id,
    storedOrder,
  );
  return {
    hiddenIds,
    installedCount: visibleInstalledIds(statuses).length,
    visibleTotal: catalog.filter((meta) => !hiddenIds.has(meta.id)).length,
    orderedInstalledMetas,
  };
}
