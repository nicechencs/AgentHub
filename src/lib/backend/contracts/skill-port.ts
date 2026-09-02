import type { AgentId, Skill, SkillSyncState } from '@/lib/types';
import type {
  CoreSkill,
  InstalledSkillDto,
  SkillListingDto,
  SkillMarkdownPreviewDto,
  SkillProjectionResultDto,
  SkillsFsChangedPayload,
} from './skill-types';

export interface SkillPort {
  listSkills(): Promise<Skill[]>;
  toggleSkillSync(
    skillId: string,
    agentId: AgentId,
    opts?: { force?: boolean; mode?: 'link' | 'copy' },
  ): Promise<{ state: SkillSyncState; conflict: boolean }>;
  checkConflict(skillId: string, agentId: AgentId): Promise<boolean>;
  syncAll(): Promise<{ synced: number; skipped: number; failed: number }>;
  /** CLI / internal alignment. GUI catalog uses `listSkillCatalog`. */
  listInstalledSkills(): Promise<InstalledSkillDto[]>;
  /** Shared library + `private_source` agent rows (no available/conflict copies). */
  listSkillCatalog(): Promise<InstalledSkillDto[]>;
  installSkillFromSource(source: string, overwrite?: boolean): Promise<CoreSkill>;
  importPrivateSkillToShared(
    skillId: string,
    agentId: AgentId,
    overwrite?: boolean,
  ): Promise<Skill>;
  installSkill(source?: string): Promise<void>;
  uninstallSkill(skillId: string, privateAgent?: AgentId): Promise<void>;
  updateSkill(skillId: string): Promise<CoreSkill>;
  applySkillProjection(
    skillId: string,
    agentId: AgentId,
    mode?: 'link' | 'copy',
  ): Promise<SkillProjectionResultDto>;
  searchSkillMarket(query?: string): Promise<SkillListingDto[]>;
  /** Install from market listing id (e.g. owner/repo/skill from skills.sh). */
  installMarketSkill(skillId: string, overwrite?: boolean): Promise<CoreSkill>;
  openPathInFileManager(path: string): Promise<string>;
  /**
   * Read local SKILL.md for markdown preview.
   * Omit `privateAgent` for shared library; pass agent id for private skills.
   */
  readSkillMarkdown(
    skillId: string,
    privateAgent?: AgentId | null,
  ): Promise<SkillMarkdownPreviewDto>;
  /** Skills under a workspace (`.agents/skills` plus known project folders). */
  listProjectSkills(workspacePath: string): Promise<InstalledSkillDto[]>;
  /** Install into `<workspace>/.agents/skills`. */
  installProjectSkill(
    workspacePath: string,
    source: string,
    overwrite?: boolean,
  ): Promise<CoreSkill>;
  /** Recycle a skill under the workspace. `origin` defaults to `.agents/skills`. */
  uninstallProjectSkill(
    workspacePath: string,
    skillId: string,
    origin?: string,
  ): Promise<void>;
  /** Read `SKILL.md` for a project skill. `origin` defaults to `.agents/skills`. */
  readProjectSkillMarkdown(
    workspacePath: string,
    skillId: string,
    origin?: string | null,
  ): Promise<SkillMarkdownPreviewDto>;
  /** Subscribe to skill-directory changes. Returns an unsubscribe function. */
  onFsChanged(
    handler: (payload?: SkillsFsChangedPayload) => void,
  ): Promise<() => void> | (() => void);
}
