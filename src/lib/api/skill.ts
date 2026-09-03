/**
 * Skill API façade — delegates to app runtime backend.
 */
import { getBackend } from '@/app/runtime';
import type { CoreSkill, InstalledSkillDto, SkillListingDto, SkillMarkdownPreviewDto, SkillProjectionResultDto, SkillsFsChangedPayload } from '@/lib/backend/contracts/skill-types';
import type { AgentKey, Skill, SkillMapStatus, SkillSyncState } from '@/lib/types';

export type {
  CoreSkillSyncState,
  CoreSkillMapStatus,
  CoreSkillLinkKind,
  CoreSkillProjection,
  CoreSkill,
  SkillSyncReport,
  InstalledSkillDto,
  SkillCopyLocation,
  SkillListingDto,
  SkillMarkdownPreviewDto,
  SkillProjectionResultDto,
  SkillsFsChangedPayload,
} from '@/lib/backend/contracts/skill-types';

export {
  mapCoreSkill,
  isMappedState,
  isActionableMapStatus,
  mapStatusLabel,
  mapMapStatus,
  workspacePresenceLabel,
  isPrivateInstalledOrigin,
  resolveWorkspacePresence,
  canAdoptWorkspaceSkill,
} from '@/lib/backend/contracts/skill-map';
export type { WorkspacePresence } from '@/lib/backend/contracts/skill-map';

export async function listSkills(): Promise<Skill[]> {
  return getBackend().skill.listSkills();
}

export async function toggleSkillSync(
  skillId: string,
  agentId: AgentKey,
  opts: { force?: boolean; mode?: 'link' | 'copy' } = {},
): Promise<{ state: SkillSyncState; conflict: boolean }> {
  return getBackend().skill.toggleSkillSync(skillId, agentId, opts);
}

export async function checkConflict(skillId: string, agentId: AgentKey): Promise<boolean> {
  return getBackend().skill.checkConflict(skillId, agentId);
}

export async function syncAll(): Promise<{ synced: number; skipped: number; failed: number }> {
  return getBackend().skill.syncAll();
}

export async function listInstalledSkills(): Promise<InstalledSkillDto[]> {
  return getBackend().skill.listInstalledSkills();
}

export async function listSkillCatalog(): Promise<InstalledSkillDto[]> {
  return getBackend().skill.listSkillCatalog();
}

export async function installSkillFromSource(
  source: string,
  overwrite = false,
): Promise<CoreSkill> {
  return getBackend().skill.installSkillFromSource(source, overwrite);
}

export async function importPrivateSkillToShared(
  skillId: string,
  agentId: AgentKey,
  overwrite = false,
): Promise<Skill> {
  return getBackend().skill.importPrivateSkillToShared(skillId, agentId, overwrite);
}

export async function installSkill(source?: string): Promise<void> {
  return getBackend().skill.installSkill(source);
}

export async function uninstallSkill(
  skillId: string,
  privateAgent?: AgentKey,
): Promise<void> {
  return getBackend().skill.uninstallSkill(skillId, privateAgent);
}

export async function updateSkill(skillId: string): Promise<CoreSkill> {
  return getBackend().skill.updateSkill(skillId);
}

export async function applySkillProjection(
  skillId: string,
  agentId: AgentKey,
  mode: 'link' | 'copy' = 'link',
): Promise<SkillProjectionResultDto> {
  return getBackend().skill.applySkillProjection(skillId, agentId, mode);
}

export async function searchSkillMarket(query = ''): Promise<SkillListingDto[]> {
  return getBackend().skill.searchSkillMarket(query);
}

export async function installMarketSkill(
  skillId: string,
  overwrite = false,
): Promise<CoreSkill> {
  return getBackend().skill.installMarketSkill(skillId, overwrite);
}

export async function openPathInFileManager(path: string): Promise<string> {
  return getBackend().skill.openPathInFileManager(path);
}

/** Read local SKILL.md for markdown preview (shared library or private agent skill). */
export async function readSkillMarkdown(
  skillId: string,
  privateAgent?: AgentKey | null,
): Promise<SkillMarkdownPreviewDto> {
  return getBackend().skill.readSkillMarkdown(skillId, privateAgent);
}

export async function listProjectSkills(workspacePath: string): Promise<InstalledSkillDto[]> {
  return getBackend().skill.listProjectSkills(workspacePath);
}

export async function installProjectSkill(
  workspacePath: string,
  source: string,
  overwrite = false,
): Promise<CoreSkill> {
  return getBackend().skill.installProjectSkill(workspacePath, source, overwrite);
}

export async function uninstallProjectSkill(
  workspacePath: string,
  skillId: string,
  origin?: string,
): Promise<void> {
  return getBackend().skill.uninstallProjectSkill(workspacePath, skillId, origin);
}

export async function readProjectSkillMarkdown(
  workspacePath: string,
  skillId: string,
  origin?: string | null,
): Promise<SkillMarkdownPreviewDto> {
  return getBackend().skill.readProjectSkillMarkdown(workspacePath, skillId, origin);
}

/** Subscribe to skill-directory changes. Browser mock is a no-op. */
export async function onSkillsFsChanged(
  handler: (payload?: SkillsFsChangedPayload) => void,
): Promise<() => void> {
  return getBackend().skill.onFsChanged(handler);
}

// re-export type for mapStatus consumers
export type { SkillMapStatus };
