import { AGENT_IDS } from '@/config/agents';
import type { AgentKey, Skill, SkillLinkKind, SkillMapStatus, SkillProjection, SkillSyncState } from '@/lib/types';
import type { CoreSkill, CoreSkillLinkKind, CoreSkillMapStatus } from './skill-types';

function mapLinkKind(k?: CoreSkillLinkKind | null): SkillLinkKind {
  switch (k) {
    case 'symlink':
    case 'junction':
    case 'hardlink':
    case 'none':
      return k;
    default:
      return 'none';
  }
}

export function mapMapStatus(
  status: CoreSkillMapStatus | SkillMapStatus | undefined,
  state: SkillSyncState,
): SkillMapStatus {
  if (status) return status as SkillMapStatus;
  switch (state) {
    case 'unsupported':
      return 'agent_unsupported';
    case 'foreign':
    case 'conflict':
      return 'conflict';
    default:
      return 'available';
  }
}

/** Core Skill → UI Skill */
export function mapCoreSkill(s: CoreSkill): Skill {
  const projectionByAgent = {} as Record<AgentKey, SkillSyncState>;
  const conflicts: AgentKey[] = [];
  const projections: SkillProjection[] = [];

  for (const id of AGENT_IDS) {
    projectionByAgent[id] = 'unsupported';
  }

  for (const proj of s.projections ?? []) {
    const state = proj.state as SkillSyncState;
    projectionByAgent[proj.agent] = state;
    if (state === 'foreign' || state === 'conflict') {
      conflicts.push(proj.agent);
    }
    projections.push({
      agent: proj.agent,
      state,
      linkKind: mapLinkKind(proj.linkKind),
      targetDir: proj.targetDir ?? null,
      resolvedTarget: proj.resolvedTarget ?? null,
      mapStatus: mapMapStatus(proj.mapStatus, state),
    });
  }

  return {
    id: s.id,
    name: s.name,
    description: s.description,
    sourceDir: s.sourceDir,
    projections,
    projectionByAgent,
    conflicts,
  };
}

export function isMappedState(state: SkillSyncState): boolean {
  return state === 'linked' || state === 'copied';
}

export function isActionableMapStatus(status?: SkillMapStatus | null): boolean {
  return status === 'available' || status === 'conflict' || status == null;
}

export function mapStatusLabel(status: SkillMapStatus): string {
  switch (status) {
    case 'available':
      return '可启用到此工具';
    case 'private_source':
      return '只在本工具里，需先加入共享库';
    case 'agent_unsupported':
      return '该工具不支持技能';
    case 'agent_not_installed':
      return '该工具尚未安装';
    case 'target_unavailable':
      return '技能目录不可用';
    case 'conflict':
      return '可启用，但目标已有不同内容';
    default:
      return status;
  }
}

/**
 * 各agent技能列表用的短徽章（不替代 mapStatusLabel 的完整说明）。
 *
 * - origin=shared → 共享库
 * - agent + private_source → 只在本工具（可加入共享库）
 * - agent + available → 已在共享库（同名已在共享库：同步副本或链接）
 * - agent + conflict → 有冲突
 */
export function workspacePresenceLabel(
  origin: string,
  mapStatus?: SkillMapStatus | null,
): string {
  if (origin === 'shared') return '共享库';
  switch (mapStatus ?? 'private_source') {
    case 'private_source':
      return '只在本工具';
    case 'conflict':
      return '内容不同';
    case 'available':
      // Agent 目录下的 available = 与共享库内容一致（或链接到共享库）
      return '已在共享库';
    default:
      return mapStatusLabel(mapStatus ?? 'private_source');
  }
}

/** 是否为各agent技能行（非共享库 origin） */
export function isPrivateInstalledOrigin(origin: string): boolean {
  return origin !== 'shared';
}

export type WorkspacePresence =
  | 'private_only'
  | 'in_library'
  | 'conflict'
  | 'shared'
  | 'other';

export function resolveWorkspacePresence(
  origin: string,
  mapStatus?: SkillMapStatus | null,
): WorkspacePresence {
  if (origin === 'shared') return 'shared';
  if (mapStatus === 'conflict') return 'conflict';
  if (mapStatus === 'private_source' || mapStatus == null) return 'private_only';
  // Agent 根下 mapStatus=available：同 id 已在共享库
  if (mapStatus === 'available') return 'in_library';
  return 'other';
}

/** 只在本工具 / 有冲突 才允许加入共享库 */
export function canAdoptWorkspaceSkill(
  origin: string,
  mapStatus?: SkillMapStatus | null,
): boolean {
  const p = resolveWorkspacePresence(origin, mapStatus);
  return p === 'private_only' || p === 'conflict';
}
