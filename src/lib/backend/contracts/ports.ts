import type { AgentCatalogPort } from './agent-catalog';
import type { AdapterPort } from './adapter';
import type { AccountPort } from './account-port';
import type { AgentPort } from './agent-port';
import type { BackupPort } from './backup-port';
import type { ChatPort } from './chat-port';
import type { ConfigPort } from './config-types';
import type { DashboardPort } from './dashboard-port';
import type { DoctorPort } from './doctor-port';
import type { EnvPort } from './env-port';
import type { InstallPort } from './install-port';
import type { ProjectPort } from './project-port';
import type { ProviderPort } from './provider-port';
import type { SettingsPort } from './settings-port';
import type { SkillPort } from './skill-port';
import type { TicketPort } from './ticket';
import type { TrashPort } from './trash-port';
import type { UpdatePort } from './update-types';
import type { UsagePort } from './usage-port';

export * from './account-port';
export * from './agent-port';
export * from './backup-port';
export * from './chat-port';
export * from './dashboard-port';
export type { DoctorPort } from './doctor-port';
export * from './env-port';
export * from './install-port';
export type { McpPort } from './mcp-types';
export * from './project-port';
export * from './provider-port';
export * from './settings-port';
export * from './skill-port';
export * from './trash-port';
export type { UpdatePort } from './update-types';
export * from './usage-port';

export interface BackendFeatures {
  providerUndoSwitch: boolean;
  providerTestLatency: boolean;
  accountUndoSwitch: boolean;
  backupExport: boolean;
}
export const DEFAULT_BACKEND_FEATURES: BackendFeatures = {
  providerUndoSwitch: false,
  providerTestLatency: false,
  accountUndoSwitch: false,
  backupExport: false,
};
export function resolveBackendFeatures(
  features?: Partial<BackendFeatures> | null,
): BackendFeatures {
  return { ...DEFAULT_BACKEND_FEATURES, ...features };
}
export interface Backend {
  features: BackendFeatures;
  account: AccountPort;
  adapter: AdapterPort;
  ticket: TicketPort;
  agent: AgentPort;
  catalog: AgentCatalogPort;
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
  update: UpdatePort;
  trash: TrashPort;
  mcp: import('./mcp-types').McpPort;
}
export type CreateBackend = () => Backend;
