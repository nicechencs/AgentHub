import type { AgentId, SkillLinkKind, SkillMapStatus, SkillSyncState } from '@/lib/types';

export type CoreSkillSyncState = SkillSyncState;
export type CoreSkillMapStatus = SkillMapStatus;
export type CoreSkillLinkKind = SkillLinkKind;

export interface CoreSkillProjection {
  agent: AgentId;
  state: CoreSkillSyncState;
  linkKind?: CoreSkillLinkKind;
  targetDir?: string | null;
  resolvedTarget?: string | null;
  mapStatus?: CoreSkillMapStatus;
}

export interface CoreSkill {
  id: string;
  name: string;
  description: string;
  sourceDir: string;
  projections: CoreSkillProjection[];
}

export interface SkillSyncReport {
  synced: { skill: string; agent: AgentId }[];
  skipped: { skill: string; agent: AgentId }[];
  failed: { skill: string; agent: AgentId; code: string; error: string }[];
}

export interface InstalledSkillDto {
  id: string;
  name: string;
  description: string;
  sourceDir: string;
  rootLabel: string;
  rootDir: string;
  origin: string;
  projectable: boolean;
  mapStatus?: SkillMapStatus;
  source?: {
    kind: string;
    locator: string;
    version?: string | null;
    installedAt: string;
    updatedAt?: string | null;
  } | null;
  projections: CoreSkillProjection[];
}

export interface SkillListingDto {
  id: string;
  name: string;
  description: string;
  version?: string | null;
  providerId: string;
  installed: boolean;
  /** 市场官网详情页；有则支持外链打开 */
  detailUrl?: string | null;
}

export interface SkillProjectResultDto {
  skillId: string;
  agent: AgentId;
  requestedMode: 'link' | 'copy';
  appliedLinkKind: SkillLinkKind;
  fellBack: boolean;
  targetDir: string;
}

/** Local `SKILL.md` body for GUI markdown preview. */
export interface SkillMarkdownPreviewDto {
  skillId: string;
  name: string;
  /** Absolute path to the SKILL.md file that was read. */
  path: string;
  content: string;
  /** True when content was cut to the backend size cap. */
  truncated: boolean;
}

export interface CoreProviderPreset {
  agent: AgentId;
  id: string;
  label: string;
  format: 'json' | 'toml';
  template: string;
}
