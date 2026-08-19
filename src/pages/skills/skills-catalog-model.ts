import type { InstalledSkillDto } from '@/lib/api/skill';
import type { AgentId, SkillSyncState } from '@/lib/types';

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
