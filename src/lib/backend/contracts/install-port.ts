import type { AgentKey, RuntimeId } from '@/lib/types';
import type { AgentInstallCatalogEntryDto, InstallOutcome, InstallProgressPayload } from './install-types';

export interface InstallPort {
  /** Core catalog install channels (allowlisted URLs / npm packages). */
  listInstallCatalog(): Promise<AgentInstallCatalogEntryDto[]>;
  installRuntime(runtimeId: RuntimeId, channel?: string): Promise<InstallOutcome>;
  installAgentCmd(
    agentId: AgentKey,
    channel: string,
    installDeps?: boolean,
  ): Promise<InstallOutcome>;
  upgradeAgentCmd(agentId: AgentKey): Promise<InstallOutcome>;
  uninstallAgentCmd(agentId: AgentKey, purgeConfig: boolean): Promise<InstallOutcome>;
  openAgentConfigDir(agentId: AgentKey): Promise<string>;
  /** Start this Agent's CLI in a new terminal, or its desktop app. Path is resolved from detect. */
  launchAgentProgram(agentId: AgentKey, kind: 'cli' | 'app'): Promise<void>;
  getAgentLivePaths(agentId: AgentKey): Promise<{
    config: string;
    auth?: string | null;
    extra?: string[];
    openDir: string;
  }>;
  /** Subscribe to live install/upgrade/uninstall log lines. Returns an unsubscribe function. */
  onProgress(
    handler: (payload: InstallProgressPayload) => void,
  ): Promise<() => void> | (() => void);
}
