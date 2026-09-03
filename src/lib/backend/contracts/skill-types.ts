import type { AgentKey, SkillLinkKind, SkillMapStatus, SkillSyncState } from '@/lib/types';

export type CoreSkillSyncState = SkillSyncState;
export type CoreSkillMapStatus = SkillMapStatus;
export type CoreSkillLinkKind = SkillLinkKind;

export interface CoreSkillProjection {
  agent: AgentKey;
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
  synced: { skill: string; agent: AgentKey }[];
  skipped: { skill: string; agent: AgentKey }[];
  failed: { skill: string; agent: AgentKey; code: string; error: string }[];
}

/** One physical private copy after GUI grouping. */
export interface SkillCopyLocation {
  agentId: AgentKey;
  sourceDir: string;
  rootDir: string;
  rootLabel: string;
}

/** Installed / catalog row. `listSkillCatalog` reuses this DTO (no extra type). */
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
  /** Private catalog fingerprint; GUI groups identical copies by `(id, contentHash)`. */
  contentHash?: string | null;
  /** GUI-only: merged private copies. Backend catalog does not send this. */
  copies?: SkillCopyLocation[];
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

export interface SkillProjectionResultDto {
  skillId: string;
  agent: AgentKey;
  requestedMode: 'link' | 'copy';
  appliedLinkKind: SkillLinkKind;
  fellBack: boolean;
  targetDir: string;
}

/** Debounced skill-directory change from the desktop filesystem watcher. */
export interface SkillsFsChangedPayload {
  source?: string;
  roots?: number;
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
  agent: AgentKey;
  id: string;
  label: string;
  format: 'json' | 'toml';
  template: string;
}
