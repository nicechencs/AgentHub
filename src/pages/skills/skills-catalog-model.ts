import type { InstalledSkillDto } from '@/lib/api/skill';
import type { AgentId, SkillSyncState } from '@/lib/types';
import { skillsCopy } from './copy';
import type { LocalFilter } from './skills-preview-model';

export const FILTERS: { id: LocalFilter; label: string }[] = [
  { id: 'all', label: skillsCopy.filters.enableAll },
  { id: 'private', label: skillsCopy.filters.enablePrivate },
  { id: 'mapped', label: skillsCopy.filters.enableMapped },
  { id: 'unmapped', label: skillsCopy.filters.enableUnmapped },
  { id: 'conflict', label: skillsCopy.filters.enableConflict },
];

export const cellKey = (skillId: string, agentId: AgentId) => `${skillId}:${agentId}`;

/** 按真实写操作结果更新共享 catalog 行的投影 */
export function applyCatalogCellState(
  rows: InstalledSkillDto[],
  skillId: string,
  agentId: AgentId,
  state: SkillSyncState,
): InstalledSkillDto[] {
  return rows.map((row) => {
    if (row.origin !== 'shared' || row.id !== skillId) return row;
    const projections = (row.projections ?? []).map((p) =>
      p.agent === agentId
        ? {
            ...p,
            state,
            linkKind: state === 'linked' ? p.linkKind : 'none',
          }
        : p,
    );
    return { ...row, projections };
  });
}
