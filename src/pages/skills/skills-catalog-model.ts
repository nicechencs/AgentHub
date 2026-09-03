import type { InstalledSkillDto } from '@/lib/api/skill';
import type { AgentKey, SkillLinkKind, SkillSyncState } from '@/lib/types';

export const cellKey = (skillId: string, agentId: AgentKey) => `${skillId}:${agentId}`;

/** 按真实写操作结果更新共享 catalog 行的投影 */
export function applyCatalogCellState(
  rows: InstalledSkillDto[],
  skillId: string,
  agentId: AgentKey,
  state: SkillSyncState,
): InstalledSkillDto[] {
  return rows.map((row) => {
    if (row.origin !== 'shared' || row.id !== skillId) return row;
    const projections = (row.projections ?? []).map((p) =>
      p.agent === agentId
        ? {
            ...p,
            state,
            linkKind: (state === 'linked'
              ? p.linkKind && p.linkKind !== 'none'
                ? p.linkKind
                : 'junction'
              : 'none') as SkillLinkKind,
          }
        : p,
    );
    return { ...row, projections };
  });
}
