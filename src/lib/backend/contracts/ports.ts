import type {
  Account,
  AgentId,
  AgentProject,
  AgentProjectExcerpt,
  AgentSession,
  AgentStatus,
  AppSettings,
  BackupMeta,
  ChatEvent,
  ChatMessage,
  Conversation,
  DashboardAlert,
  LogLevel,
  ParserHealth,
  ProjectMetadataFile,
  Provider,
  RuntimeDetect,
  RuntimeId,
  Skill,
  SkillSyncState,
  SwitchPreview,
  UsageRecord,
  UsageTrendPoint,
} from '@/lib/types';
import type { AgentCatalogPort } from './agent-catalog';
import type { DoctorMapped, DoctorReport } from './doctor-port';
import type { InstallOutcome } from './install-types';
import type {
  CoreProviderPreset,
  CoreSkill,
  InstalledSkillDto,
  SkillListingDto,
  SkillMarkdownPreviewDto,
  SkillProjectResultDto,
} from './skill-types';
import type { UsageAvailability, UsageQuery } from './usage-types';
import type { ConfigPort } from './config-types';
import type { UpdatePort } from './update-types';
import type { AdapterPort } from './adapter';
import { normalizeAuthHealth } from './auth-state';

/** PKCE start result from backend. */
export interface OAuthStartInfo {
  state: string;
  authorizeUrl: string;
  redirectUri: string;
  agentId: AgentId;
  /** Pi multi-provider key when applicable. */
  providerKey?: string | null;
  browserOpened: boolean;
}

export interface OAuthWaitInfo {
  state: string;
  agentId: AgentId;
  status: 'waiting' | 'callbackReceived' | 'succeeded' | 'failed';
  error?: string | null;
}

export type OAuthFlowKind = 'pkce' | 'deviceCode';

/** One selectable OAuth login target (Pi has multiple). */
export interface OAuthLoginOption {
  id: string;
  agentId: AgentId;
  label: string;
  description: string;
  flow: OAuthFlowKind;
  authJsonKey?: string | null;
}

export interface DeviceOAuthStartInfo {
  state: string;
  agentId: AgentId;
  providerKey: string;
  userCode: string;
  verificationUri: string;
  verificationUriComplete?: string | null;
  intervalSecs: number;
  expiresInSecs: number;
}

export interface DeviceOAuthPollInfo {
  state: string;
  status: 'pending' | 'slowDown' | 'complete' | 'completing' | 'failed' | 'expired';
  error?: string | null;
}

/** Read-only live authentication probe; never contains credential material. */
export interface AuthState {
  /** Core wire field. Older adapters may return agentId instead. */
  agent: AgentId;
  kind?: string | null;
  summary: string;
  hasCredentials: boolean;
  /** Non-secret live-file revision used to detect external token rotation. */
  revision?: string | null;
  /** Optional backend verification result (old backends omit it). */
  health?: import('./auth-state').AuthHealth;
  /** Redacted source label (settings/auth file/live/etc.). */
  source?: string | null;
}

/** Normalized probe consumed by browser pages; keeps agentId for old callers. */
export interface LiveAuthProbe {
  agentId: AgentId;
  kind?: string | null;
  summary: string;
  hasCredentials: boolean;
  /** Non-secret live-file revision used to detect external token rotation. */
  revision?: string | null;
  health?: import('./auth-state').AuthHealth;
  source?: string | null;
}

/** Accept both current core AuthState (`agent`) and legacy JS probe (`agentId`). */
export function normalizeAuthState(
  raw: Partial<AuthState> & { agentId?: AgentId },
  fallbackAgentId: AgentId,
): LiveAuthProbe {
  const agentId = raw.agentId ?? raw.agent ?? fallbackAgentId;
  return {
    agentId,
    kind: raw.kind ?? null,
    summary: typeof raw.summary === 'string' ? raw.summary : '',
    hasCredentials: raw.hasCredentials === true,
    revision: raw.revision ?? null,
    health: normalizeAuthHealth(raw.health),
    source: raw.source ?? null,
  };
}

export interface AccountPort {
  listAccounts(agentId?: AgentId): Promise<Account[]>;
  probeLiveAuth(agentId: AgentId): Promise<LiveAuthProbe>;
  switchAccount(agentId: AgentId, accountId: string): Promise<void>;
  undoSwitchAccount(agentId: AgentId): Promise<boolean>;
  addApiKeyAccount(
    agentId: AgentId,
    key: string,
    label?: string | null,
    envKey?: string | null,
  ): Promise<Account>;
  /** Update API Key account label and/or key. Omit/empty key keeps the stored secret. */
  updateApiKeyAccount(
    agentId: AgentId,
    accountId: string,
    opts: { label?: string | null; key?: string | null },
  ): Promise<Account>;
  importCurrentLogin(agentId: AgentId): Promise<Account>;
  /** Whether any OAuth login option is available for this agent. */
  oauthSupported(agentId: AgentId): Promise<boolean>;
  /** List OAuth login options (Pi returns multi-provider catalog). */
  listOAuthOptions(agentId: AgentId): Promise<OAuthLoginOption[]>;
  /** Start loopback PKCE; opens system browser when openBrowser=true. */
  startOAuth(
    agentId: AgentId,
    openBrowser?: boolean,
    providerKey?: string | null,
  ): Promise<OAuthStartInfo>;
  /** Block until callback or timeout. */
  waitOAuth(state: string, timeoutSecs?: number): Promise<OAuthWaitInfo>;
  /** Exchange code for the given PKCE state and store account. */
  finishOAuth(state: string): Promise<Account>;
  /** Device-code flow (Pi xAI). */
  startDeviceOAuth(agentId: AgentId, providerKey: string): Promise<DeviceOAuthStartInfo>;
  pollDeviceOAuth(state: string): Promise<DeviceOAuthPollInfo>;
  finishDeviceOAuth(state: string): Promise<Account>;
  /**
   * Convenience: start + wait + finish for agents that support OAuth.
   * Prefer start/wait/finish for UI progress. Mock may implement only this.
   */
  completeOAuth(agentId: AgentId, providerKey?: string | null): Promise<Account>;
  deleteAccount(agentId: AgentId, accountId: string): Promise<void>;
  refreshToken(agentId: AgentId, accountId: string): Promise<void>;
  /** Force-refresh upstream 5h/7d quota windows for OAuth (Codex/Claude). */
  refreshQuota?(agentId: AgentId, accountId: string): Promise<Account>;
}

export interface ProviderPort {
  listProviders(agentId?: AgentId): Promise<Provider[]>;
  upsertProvider(p: Provider): Promise<Provider>;
  deleteProvider(agentId: AgentId, providerId: string): Promise<void>;
  importProviderLive(agentId: AgentId, name?: string): Promise<Provider>;
  switchPreview(agentId: AgentId, toProviderId: string): Promise<SwitchPreview>;
  switchProvider(agentId: AgentId, toProviderId: string): Promise<void>;
  undoSwitch(agentId: AgentId): Promise<boolean>;
  testLatency(agentId: AgentId, providerId: string): Promise<number>;
  listProviderPresets(agentId?: AgentId): Promise<CoreProviderPreset[]>;
}

export type ConnectionTrashKind = 'account' | 'provider';

/** A deleted connection retained for 30 days and available for restore. */
export interface ConnectionTrashItem {
  id: string;
  agentId: AgentId;
  kind: ConnectionTrashKind;
  sourceId: string;
  label: string;
  wasCurrent: boolean;
  deletedAt: string;
  expiresAt: string;
  /** Redacted account/provider payload returned by the backend. */
  account?: Account;
  provider?: Provider;
}

export interface TrashPort {
  list(agentId?: AgentId): Promise<ConnectionTrashItem[]>;
  restore(id: string): Promise<void>;
  permanentlyDelete(id: string): Promise<void>;
}

export interface BackupPort {
  listBackups(agentId?: AgentId): Promise<BackupMeta[]>;
  createBackup(agentId: AgentId, note?: string): Promise<BackupMeta>;
  restoreBackup(backupId: string): Promise<void>;
  deleteBackup(backupId: string): Promise<void>;
  exportBackup(backupId: string): Promise<void>;
}

export interface ChatPort {
  listConversations(): Promise<Conversation[]>;
  createConversation(agentIds: AgentId[], cwd?: string | null): Promise<Conversation>;
  updateConversation(
    id: string,
    patch: {
      title?: string;
      agentIds?: AgentId[];
      cwd?: string | null;
      allowDangerous?: boolean;
    },
  ): Promise<Conversation>;
  deleteConversation(id: string): Promise<void>;
  listChatMessages(conversationId: string): Promise<ChatMessage[]>;
  chatSend(
    conversationId: string,
    prompt: string,
    onEvent: (ev: ChatEvent) => void,
  ): Promise<void>;
  chatCancel(conversationId: string): Promise<void>;
}

export interface ProjectPort {
  listAgentProjects(
    agentId?: AgentId | null,
    includeHidden?: boolean,
  ): Promise<AgentProject[]>;
  listAgentProjectSessions(projectId: string): Promise<AgentSession[]>;
  getProjectMetadata(): Promise<ProjectMetadataFile>;
  upsertProjectMeta(
    projectId: string,
    patch: { hidden?: boolean; alias?: string | null },
  ): Promise<void>;
  setShowHiddenProjects(show: boolean): Promise<void>;
  deleteAgentProject(id: string): Promise<void>;
  deleteAgentProjects(ids: string[]): Promise<number>;
  getAgentProjectExcerpts(ids: string[]): Promise<AgentProjectExcerpt[]>;
}

export interface EnvPort {
  listRuntimes(): Promise<RuntimeDetect[]>;
  getRuntime(id: RuntimeId): Promise<RuntimeDetect>;
  installRuntime(id: RuntimeId, channel?: string): Promise<RuntimeDetect>;
  installRuntimeDetailed(id: RuntimeId, channel?: string): Promise<InstallOutcome>;
  installRuntimesBatch(targets: RuntimeId[], channel?: string): Promise<RuntimeDetect[]>;
}

export interface AgentPort {
  listAgents(): Promise<AgentStatus[]>;
  getAgent(agentId: AgentId): Promise<AgentStatus>;
  installAgent(
    agentId: AgentId,
    channel: string,
    opts?: { installDeps?: boolean },
  ): Promise<AgentStatus>;
  installAgentDetailed(
    agentId: AgentId,
    channel: string,
    opts?: { installDeps?: boolean },
  ): Promise<InstallOutcome>;
  upgradeAgent(agentId: AgentId): Promise<AgentStatus>;
  upgradeAgentDetailed(agentId: AgentId): Promise<InstallOutcome>;
  uninstallAgent(agentId: AgentId, deleteConfig: boolean): Promise<void>;
  uninstallAgentDetailed(agentId: AgentId, deleteConfig: boolean): Promise<InstallOutcome>;
  openAgentConfig(agentId: AgentId): Promise<string | null>;
  /**
   * Probe remote latest for agents (npm registry, cached).
   * Empty / omitted agentIds → all agents. force bypasses TTL cache.
   */
  checkAgentUpdates(
    agentIds?: AgentId[],
    force?: boolean,
  ): Promise<import('@/lib/types').AgentUpdateInfo[]>;
}

export interface SettingsPort {
  getSettings(): Promise<AppSettings>;
  updateSettings(patch: Partial<AppSettings>): Promise<AppSettings>;
  openLogsDir(): Promise<string>;
  /** Open http(s) URL in the system browser (Tauri cannot rely on window.open). */
  openExternalUrl(url: string): Promise<void>;
  logLevelOptions: { value: LogLevel; label: string }[];
}

export interface SkillPort {
  listSkills(): Promise<Skill[]>;
  toggleSkillSync(
    skillId: string,
    agentId: AgentId,
    opts?: { force?: boolean },
  ): Promise<{ state: SkillSyncState; conflict: boolean }>;
  checkConflict(skillId: string, agentId: AgentId): Promise<boolean>;
  syncAll(): Promise<{ synced: number; skipped: number; failed: number }>;
  listInstalledSkills(): Promise<InstalledSkillDto[]>;
  installSkillFromSource(source: string, overwrite?: boolean): Promise<CoreSkill>;
  importPrivateSkillToShared(
    skillId: string,
    agentId: AgentId,
    overwrite?: boolean,
  ): Promise<Skill>;
  installSkill(source?: string): Promise<void>;
  uninstallSkill(skillId: string, privateAgent?: AgentId): Promise<void>;
  updateSkill(skillId: string): Promise<CoreSkill>;
  projectSkill(
    skillId: string,
    agentId: AgentId,
    mode?: 'link' | 'copy',
  ): Promise<SkillProjectResultDto>;
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
}

/** Result of a usage collect pass (mirrors core CollectResult). */
export interface UsageCollectResult {
  inserted: number;
  skipped: number;
  failed: number;
  agents: Array<{
    agentId: AgentId;
    supported: boolean;
    records: number;
    failRatePct?: number;
    skipped?: number;
  }>;
  missingPricingModels?: string[];
}

export interface UsagePort {
  /** 明确区分「已接入零数据」与「尚未接入」 */
  getAvailability(): Promise<UsageAvailability>;
  queryUsage(q: UsageQuery): Promise<UsageRecord[]>;
  usageTrend(days: number, agentId?: AgentId | 'all'): Promise<UsageTrendPoint[]>;
  listModels(): Promise<string[]>;
  parserHealth(): Promise<ParserHealth[]>;
  /** Models lacking embedded pricing in recent usage_records */
  missingPricingModels?(days?: number): Promise<string[]>;
  collectUsage(onProgress?: (pct: number) => void): Promise<UsageCollectResult | void>;
}

export interface DashboardPort {
  listAlerts(): Promise<DashboardAlert[]>;
  dismissAlert(id: string): Promise<void>;
}

export interface DoctorPort {
  getDoctorReport(force?: boolean): Promise<DoctorReport>;
  loadDoctorMapped(): Promise<DoctorMapped>;
  refreshDoctor(): Promise<DoctorMapped>;
  tryLoadDoctorMapped(): Promise<DoctorMapped | null>;
  tryRefreshDoctor(): Promise<DoctorMapped | null>;
}

export interface InstallPort {
  /** Core catalog install channels (allowlisted URLs / npm packages). */
  listInstallCatalog(): Promise<import('./install-types').AgentInstallCatalogEntryDto[]>;
  installRuntime(runtimeId: RuntimeId, channel?: string): Promise<InstallOutcome>;
  installAgentCmd(
    agentId: AgentId,
    channel: string,
    installDeps?: boolean,
  ): Promise<InstallOutcome>;
  upgradeAgentCmd(agentId: AgentId): Promise<InstallOutcome>;
  uninstallAgentCmd(agentId: AgentId, purgeConfig: boolean): Promise<InstallOutcome>;
  openAgentConfigDir(agentId: AgentId): Promise<string>;
}

export type { UpdatePort } from './update-types';
export type { McpPort } from './mcp-types';

export interface Backend {
  account: AccountPort;
  /** Read-only route compatibility preview; does not apply or start anything. */
  adapter: AdapterPort;
  agent: AgentPort;
  /** Read-only agent directory (keys, capabilities, install channels). */
  catalog: AgentCatalogPort;
  /** Native config schema / read / validate / apply (P08). */
  config: ConfigPort;
  backup: BackupPort;
  chat: ChatPort;
  env: EnvPort;
  project: ProjectPort;
  provider: ProviderPort;
  settings: SettingsPort;
  skill: SkillPort;
  usage: UsagePort;
  dashboard: DashboardPort;
  doctor: DoctorPort;
  install: InstallPort;
  /** Desktop self-update (check / one-click install). */
  update: UpdatePort;
  trash: TrashPort;
  /** Read-only MCP config inventory (not full MCP management). */
  mcp: import('./mcp-types').McpPort;
}

export type CreateBackend = () => Backend;
